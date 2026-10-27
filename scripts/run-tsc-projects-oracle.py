#!/usr/bin/env python3
"""Compile a fixed project set with tsc 6.0.3 on Node and on jsse --node and diff the results (issue #1243).

    TSC_DIR=/path/to/typescript uv run python scripts/run-tsc-projects-oracle.py [options] [case ...]

TSC_DIR is an installed typescript package; without it (or with another version) the script
prints SKIP and exits 0 unless --require is given. Each case in scripts/tsc-fixtures/cases.txt
and in every cases-<topic>.txt (pinned in expected-<topic>.txt; a leading NAME=value sets an
environment variable, @pty:ROWSxCOLS runs on a pseudo-terminal, @stats normalises the numbers of
--diagnostics output) runs lib/_tsc.js on a fresh copy of a project tree on both engines. Stdout,
stderr, the exit code and every file the compiler created, changed or deleted must be identical after
normalising only the work-tree path, the TypeScript package path and h:mm:ss AM clock stamps (plus
the moving numbers of @stats output, traces and CPU profiles: scripts/tsc_oracle_norm.py).
Prints a pass/fail table; exits 1 on any difference.

Options: --jobs N (default 4), --timeout SECONDS (default 900), --ci (plain output, GitHub
annotations, full diffs), --list, --require, --pin (rewrite the expected-*.txt pins from Node's
digests; merges into the existing pins, so single cases can be re-pinned), --no-node (compare jsse with the pinned digests instead of running Node), --fixtures DIR.

Every cases-<topic>.txt next to cases.txt adds cases pinned in expected-<topic>.txt. A step of a
case that starts with @ is run by the harness itself between tsc invocations (@utimes, @replace,
@remove, @mtimes-after; see scenario_step), so multi-step scenarios do not depend on the clock.
"""

import argparse
import difflib
import glob
import hashlib
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from tsc_oracle_norm import normalise_file, normalise_stats

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIX = os.path.join(ROOT, "scripts", "tsc-fixtures")
PINNED = os.path.join(FIX, "expected-projects.txt")
CLOCK = re.compile(r"\b\d{1,2}:\d{2}:\d{2}\s?[AP]M\b")
DROP_ENV = ("NODE_OPTIONS", "FORCE_COLOR", "NO_COLOR", "COLUMNS")
PTY_RUN = os.path.join(ROOT, "scripts", "tty-fixtures", "pty-run.py")
ENV_TOKEN = re.compile(r"[A-Z_][A-Z0-9_]*=")
PTY_TOKEN = re.compile(r"@pty:(\d+)x(\d+)$")


def case_files():
    """cases.txt (pinned in expected-projects.txt) and each cases-<topic>.txt (pinned in expected-<topic>.txt)."""
    files = [(os.path.join(FIX, "cases.txt"), PINNED)]
    for path in sorted(glob.glob(os.path.join(FIX, "cases-*.txt"))):
        topic = os.path.basename(path)[len("cases-") : -len(".txt")]
        files.append((path, os.path.join(FIX, f"expected-{topic}.txt")))
    return files


def load_cases():
    cases = []
    seen = set()
    for path, pinfile in case_files():
        with open(path, encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith("#"):
                    name, project, exit_codes, args = (
                        p.strip() for p in line.split("|", 3)
                    )
                    if name in seen:
                        raise SystemExit(f"duplicate case name {name} in {path}")
                    seen.add(name)
                    cases.append(
                        (
                            name,
                            project,
                            exit_codes,
                            [shlex.split(a) for a in args.split(" ;; ")],
                            pinfile,
                        )
                    )
    return cases


def load_pins():
    pins = {}
    for _, pinfile in case_files():
        try:
            with open(pinfile, encoding="utf-8") as f:
                pins.update(row.split() for row in f if row.strip())
        except FileNotFoundError:
            pass
    return pins


def utimes(path, when):
    if os.path.isdir(path) and not os.path.islink(path):
        for dirpath, dirs, names in os.walk(path):
            for n in dirs + names:
                os.utime(os.path.join(dirpath, n), (when, when), follow_symlinks=False)
    os.utime(path, (when, when), follow_symlinks=False)


SCENARIO_STEPS = ("@utimes", "@replace", "@remove", "@mtimes-after")


def scenario_step(tree, step):
    """Run a harness step of a case (` ;; @name args`); returns text for the case's output or None.

    @utimes EPOCH PATH...  set atime and mtime of the paths (a directory: everything below it)
    @replace PATH OLD NEW  replace the text in a file
    @remove PATH...        delete files or trees
    @mtimes-after EPOCH PATH...  print whether each path's mtime is after EPOCH
    """
    op, args = step[0], step[1:]
    if op == "@utimes":
        for p in args[1:]:
            utimes(os.path.join(tree, p), int(args[0]))
    elif op == "@replace":
        path = os.path.join(tree, args[0])
        with open(path, encoding="utf-8", newline="") as f:
            text = f.read()
        if args[1] not in text:
            raise SystemExit(f"@replace: {args[1]!r} not found in {args[0]}")
        with open(path, "w", encoding="utf-8", newline="") as f:
            f.write(text.replace(args[1], args[2], 1))
    elif op == "@remove":
        for p in args:
            full = os.path.join(tree, p)
            if os.path.isdir(full):
                shutil.rmtree(full)
            else:
                os.remove(full)
    elif op == "@mtimes-after":
        return "".join(
            f"{p}: {'after' if os.stat(os.path.join(tree, p)).st_mtime > int(args[0]) else 'not after'}\n"
            for p in args[1:]
        )
    else:
        raise SystemExit(f"unknown scenario step {op}")
    return None


ESCAPE = re.compile(r"\\(?:([rn])|u([0-9a-fA-F]{4}))")
ESCAPED = (
    (".utf16le.esc", "utf-16-le"),
    (".utf16be.esc", "utf-16-be"),
    (".esc", "utf-8"),
)


def decode_escapes(text):
    """Decode \\r, \\n and \\uXXXX only (a surrogate pair gives one astral character); anything else is an error."""
    out = ESCAPE.sub(
        lambda m: (
            {"r": "\r", "n": "\n"}[m.group(1)]
            if m.group(1)
            else chr(int(m.group(2), 16))
        ),
        text,
    )
    if "\\" in out:
        raise ValueError(f"unsupported escape in {text!r}")
    return out.encode("utf-16", "surrogatepass").decode("utf-16")


def expand_escaped(tree):
    """Write <name>.esc as <name>, decoding \\r, \\n and \\uXXXX escapes (UTF-8; <name>.utf16le.esc and .utf16be.esc
    give UTF-16), for sources the repository hooks would normalise or reject: CRLF, lone CR, BOM, no final newline, UTF-16."""
    for dirpath, _, names in os.walk(tree):
        for n in names:
            for suffix, codec in ESCAPED:
                if n.endswith(suffix):
                    p = os.path.join(dirpath, n)
                    with open(p, encoding="ascii") as f:
                        text = f.read().removesuffix("\n")
                    with open(p[: -len(suffix)], "wb") as f:
                        f.write(decode_escapes(text).encode(codec))
                    os.remove(p)
                    break


def split_invocation(tokens):
    """Leading NAME=value tokens set the environment, @pty:ROWSxCOLS runs on a pseudo-terminal, @stats
    normalises the moving numbers of --diagnostics output."""
    env, pty, stats = {}, None, False
    while tokens and (
        ENV_TOKEN.match(tokens[0])
        or PTY_TOKEN.match(tokens[0])
        or tokens[0] == "@stats"
    ):
        tok = tokens.pop(0)
        if tok == "@stats":
            stats = True
        elif m := PTY_TOKEN.match(tok):
            pty = (m.group(1), m.group(2))
        else:
            k, _, v = tok.partition("=")
            env[k] = v
    return env, pty, stats, tokens


def snapshot(tree):
    files = {}
    for dirpath, dirs, names in os.walk(tree):
        for n in dirs + names:
            p = os.path.join(dirpath, n)
            rel = os.path.relpath(p, tree)
            if os.path.islink(p):
                files[rel] = b"-> " + os.readlink(p).encode("utf-8", "surrogateescape")
            elif os.path.isdir(p):
                files[rel] = b"<dir>"
            else:
                with open(p, "rb") as f:
                    files[rel] = f.read()
    return files


def run_case(engine, exe, tsc_dir, work, case, timeout):
    name, project, _, invocations, _ = case
    tree = os.path.join(work, engine, name)
    shutil.copytree(os.path.join(FIX, "projects", project), tree)
    expand_escaped(tree)
    before = snapshot(tree)
    env = {k: v for k, v in os.environ.items() if k not in DROP_ENV}
    cmd = (
        [exe]
        + (["--node"] if engine == "jsse" else [])
        + [os.path.join(tsc_dir, "lib", "_tsc.js")]
    )
    out, err, codes, stats = [], [], [], []
    start = time.monotonic()
    for i, tokens in enumerate(invocations):
        if tokens[0] in SCENARIO_STEPS:
            try:
                note = scenario_step(tree, tokens)
            except OSError as e:
                raise SystemExit(f"{name}: step {' '.join(tokens)} failed: {e}")
            if note is not None:
                out.append(note.encode())
                err.append(b"")
            continue
        extra, pty, stat, args = split_invocation(list(tokens))
        stats.append(stat)
        run = cmd + args
        if pty:
            run = [sys.executable, "-I", PTY_RUN, "--rows", pty[0], "--cols", pty[1]]
            run += ["--timeout", str(max(timeout - 5, 1)), "--"] + cmd + args
        try:
            r = subprocess.run(
                run,
                cwd=tree,
                env={**env, **extra},
                capture_output=True,
                timeout=timeout,
            )
            codes.append(str(r.returncode))
            out.append(r.stdout)
            err.append(r.stderr)
        except subprocess.TimeoutExpired as e:
            codes.append("timeout")
            out.append(e.stdout or b"")
            err.append(e.stderr or b"")
            break
    elapsed = time.monotonic() - start
    after = snapshot(tree)

    def norm(b):
        t = b.decode("utf-8", "surrogateescape")
        t = t.replace(tree, "<DIR>").replace(tsc_dir, "<TSC>")
        t = re.sub(re.escape(os.path.join(work, engine)) + r"(?!\w)", "<WORK>", t)
        t = t.replace(work, "<ROOT>")
        return CLOCK.sub("<TIME>", t)

    files = {
        p: normalise_file(p, norm(c)) for p, c in after.items() if before.get(p) != c
    }
    files.update({p: "<deleted>" for p in before if p not in after})

    def joined(parts, moving=()):
        return "".join(
            f"--- invocation {i + 1}\n{normalise_stats(norm(p)) if i < len(moving) and moving[i] else norm(p)}"
            for i, p in enumerate(parts)
        )

    return {
        "exit": " ".join(codes),
        "stdout": joined(out, stats),
        "stderr": joined(err),
        "files": files,
        "secs": elapsed,
    }


def digest(res):
    h = hashlib.sha256()
    for part in (res["exit"], res["stdout"], res["stderr"]):
        h.update(part.encode("utf-8", "surrogateescape") + b"\0")
    for p in sorted(res["files"]):
        h.update(
            p.encode()
            + b"\0"
            + res["files"][p].encode("utf-8", "surrogateescape")
            + b"\0"
        )
    return h.hexdigest()


def diff_parts(a, b):
    bad = {}
    for key in ("exit", "stdout", "stderr"):
        if a[key] != b[key]:
            bad[key] = (a[key], b[key], "node", "jsse")
    for p in sorted(set(a["files"]) | set(b["files"])):
        if a["files"].get(p) != b["files"].get(p):
            bad["file " + p] = (
                a["files"].get(p, "<missing>"),
                b["files"].get(p, "<missing>"),
                "node",
                "jsse",
            )
    return bad


def render(label, bad, limit):
    lines = []
    for key, (x, y, left, right) in bad.items():
        d = list(
            difflib.unified_diff(
                x.splitlines(), y.splitlines(), left, right, lineterm="", n=2
            )
        )
        lines.append(f"  [{label}] {key} differs")
        lines += ["    " + row for row in d[:limit]]
        if len(d) > limit:
            lines.append(f"    ... {len(d) - limit} more diff lines")
    return "\n".join(lines)


def main():
    global FIX, PINNED
    ap = argparse.ArgumentParser()
    ap.add_argument("cases", nargs="*")
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--timeout", type=int, default=900)
    ap.add_argument("--ci", action="store_true")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--require", action="store_true")
    ap.add_argument("--pin", action="store_true")
    ap.add_argument("--no-node", action="store_true")
    ap.add_argument(
        "--fixtures",
        default=FIX,
        help="directory with cases.txt, projects/ and expected-projects.txt",
    )
    o = ap.parse_args()
    FIX = os.path.abspath(o.fixtures)
    PINNED = os.path.join(FIX, "expected-projects.txt")

    cases = load_cases()
    if o.cases:
        unknown = [c for c in o.cases if c not in [c[0] for c in cases]]
        if unknown:
            sys.exit(f"unknown case: {', '.join(unknown)}")
        cases = [c for c in cases if c[0] in o.cases]
    if o.list:
        print("\n".join(c[0] for c in cases))
        return 0

    def skip(msg):
        print(("ERROR: " if o.require else "SKIP: ") + msg)
        return 3 if o.require else 0

    if not os.environ.get("TSC_DIR"):
        return skip(
            "TSC_DIR is not set; point it at an installed typescript 6.0.3 package to run the tsc project oracle"
        )
    tsc_dir = os.path.realpath(os.environ["TSC_DIR"])
    manifest = os.path.join(tsc_dir, "package.json")
    if not os.path.isfile(
        os.path.join(tsc_dir, "lib", "_tsc.js")
    ) or not os.path.isfile(manifest):
        return skip(f"{tsc_dir} is not a typescript package (no lib/_tsc.js)")
    with open(manifest, encoding="utf-8") as f:
        version = re.search(r'"version"\s*:\s*"([^"]+)"', f.read())
    version = version.group(1) if version else "unknown"
    if version != "6.0.3":
        return skip(f"the tsc project oracle targets typescript 6.0.3, found {version}")
    node = shutil.which("node")
    if not node and not o.no_node:
        return skip(
            "node not found; the oracle needs it (or --no-node with the pinned digests)"
        )
    jsse = os.path.abspath(
        os.environ.get("JSSE", os.path.join(ROOT, "target", "release", "jsse"))
    )
    if not os.access(jsse, os.X_OK):
        print("Building jsse (release)...", flush=True)
        subprocess.run(["cargo", "build", "--release"], cwd=ROOT, check=True)
    if o.pin and (o.no_node or not node):
        sys.exit("--pin needs Node: it pins Node's results")
    pins = load_pins() if o.no_node else {}
    print(
        f"typescript {version} from {tsc_dir} (jsse: {jsse}, {len(cases)} cases, {o.jobs} jobs)",
        flush=True,
    )

    # --traceResolution lists every ancestor directory, so the pins depend on the depth of the work dir
    base = (
        "/tmp"
        if os.path.realpath("/tmp") == "/tmp" and os.access("/tmp", os.W_OK)
        else None
    )
    work = os.path.realpath(tempfile.mkdtemp(prefix="tsc-oracle-", dir=base))

    def one(case):
        name = case[0]
        ref = (
            None
            if o.no_node
            else run_case("node", node, tsc_dir, work, case, o.timeout)
        )
        got = run_case("jsse", jsse, tsc_dir, work, case, o.timeout)
        if o.no_node:
            ok = pins.get(name) == digest(got)
            bad = (
                {}
                if ok
                else {
                    "digest": (
                        pins.get(name, "<unpinned>"),
                        digest(got),
                        "pinned",
                        "jsse",
                    )
                }
            )
        else:
            bad = diff_parts(ref, got)
        for engine, res in (("node", ref), ("jsse", got)):
            if res is not None and res["exit"] != case[2]:
                bad[f"{engine} exit status"] = (
                    case[2],
                    res["exit"],
                    "fixture expects",
                    engine,
                )
        if not o.ci:
            print(f"{'ok  ' if not bad else 'FAIL'} {name}", flush=True)
        return case, ref, got, bad

    try:
        with ThreadPoolExecutor(o.jobs) as pool:
            results = list(pool.map(one, cases))
        if o.pin:
            pins = load_pins()
            for case, ref, _, bad in results:
                if "node exit status" not in bad:
                    pins[case[0]] = digest(ref)
            for _, pinfile in case_files():
                order = [c[0] for c in load_cases() if c[4] == pinfile]
                with open(pinfile, "w", encoding="utf-8") as f:
                    for name in order:
                        if name in pins:
                            f.write(f"{name} {pins[name]}\n")
                print(f"wrote {pinfile}")
        print(f"\n{'case':<26}{'node s':>9}{'jsse s':>9}  result")
        failed = 0
        for case, ref, got, bad in results:
            failed += bool(bad)
            node_s = "-" if ref is None else f"{ref['secs']:.1f}"
            what = "PASS" if not bad else "FAIL (" + ", ".join(bad) + ")"
            print(f"{case[0]:<26}{node_s:>9}{got['secs']:>9.1f}  {what}")
        for case, _, _, bad in results:
            if bad:
                print(f"\n{case[0]}:\n" + render(case[0], bad, 400 if o.ci else 40))
                if o.ci:
                    print(
                        f"::error title=tsc oracle::{case[0]} differs between node and jsse: {', '.join(bad)}"
                    )
        print(f"\ntsc project oracle: {len(results) - failed}/{len(results)} passed")
        return 1 if failed else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
