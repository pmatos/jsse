#!/usr/bin/env python3
"""Build the TypeScript compiler with `hereby local` on Node and on jsse --node and diff the builds (issue #1253).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-hereby-local-oracle.py [options]

TS_SRC_DIR is a TypeScript v6.0.3 checkout with `npm ci` done. The script copies it twice (copy-on-write
where the file system allows), removes the generated files from both copies, runs `hereby local` in one
under Node and in the other under `jsse --node` (so the type-check of src/ by node_modules/typescript and
the esbuild bundles run on jsse), then

  1. compares the build logs (task order is nondeterministic, so as sorted lines, durations and paths
     normalised),
  2. compares every file under built/ and the two generated sources (diagnosticInformationMap.generated.ts,
     diagnosticMessages.generated.json) by relative path, size and sha256 after replacing the copy's path,
  3. packages the jsse-built compiler (built/local as lib/) and runs the tsc oracles
     (scripts/run-tsc-oracle.sh --full and scripts/run-tsc-projects-oracle.py) on it, so the compiler built on jsse is the one the harness runs.

jsse has no WebAssembly, which dtsBundler.mjs needs (dprint formats the d.ts bundles), so `hereby local` ends
with `Error in dts-services ... WebAssembly is not defined` after every other task has finished. Unless --full is
given that one failure is a known gap (reported, `DTS_FILES` left out of the comparison); once jsse has
WebAssembly the build exits 0 and everything is compared. --full makes the failure an error.

Without TS_SRC_DIR (or with another TypeScript version) it prints SKIP and exits 0 unless --require is given.
Exits 1 on any difference. Options: --jsse PATH (else $JSSE, else target/release/jsse), --work DIR (kept),
--timeout SECONDS per build (default 7200), --harness-jobs N, --skip-harness, --require, --ci.
"""

import argparse
import difflib
import hashlib
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DROP_ENV = (
    "NODE_OPTIONS",
    "FORCE_COLOR",
    "NO_COLOR",
    "COLUMNS",
    "NODE_DISABLE_COLORS",
    "CI",
)
# dtsBundler.mjs formats the declaration bundles with dprint's WebAssembly plugin and jsse has no WebAssembly:
# `dts-services` fails with this signature after every other task has finished, and these four files are missing.
DTS_GAP = ("Error in dts-services", "WebAssembly is not defined")
DTS_FILES = (
    "built/local/typescript.d.ts",
    "built/local/typescript.internal.d.ts",
    "built/local/tsserverlibrary.d.ts",
    "built/local/tsserverlibrary.internal.d.ts",
)
GENERATED = (
    "src/compiler/diagnosticInformationMap.generated.ts",
    "src/compiler/diagnosticMessages.generated.json",
)
DURATION = re.compile(r"(?<= in )\d[\d.]*(?:ms|s|m|h|d)(?: \d[\d.]*(?:ms|s|m|h|d))*")
EXEC_LINE = re.compile(r"^> \S+ ")
TIMING = re.compile(r"^(real|user|sys)\s")


def tree_spellings(tree):
    """Every way a build prints the copy's path, longest first (hereby shortens $HOME to ~)."""
    names = {tree, os.path.realpath(tree)}
    home = os.path.expanduser("~")
    names |= {"~" + n[len(home) :] for n in list(names) if n.startswith(home + os.sep)}
    return sorted(names, key=len, reverse=True)


def normalise_log(text, tree, exe):
    """Make two build logs comparable: paths, durations and the interpreter named by `> <exe> args` lines."""
    for old in tree_spellings(tree):
        text = text.replace(old, "<TREE>")
    out = []
    for line in text.splitlines():
        if TIMING.match(line):
            continue
        line = DURATION.sub("<T>", line)
        if EXEC_LINE.match(line):
            line = "> <EXEC> " + line.split(" ", 2)[2]
        out.append(line.replace(exe, "<EXEC>"))
    return out


def file_digest(path, tree):
    """sha256 of a file with the copy's own path made constant (maps and bundles may embed it)."""
    with open(path, "rb") as f:
        data = f.read()
    for old in tree_spellings(tree):
        data = data.replace(old.encode(), b"<TREE>")
    return hashlib.sha256(data).hexdigest(), len(data)


def snapshot(tree):
    """{relative path: (sha256, size)} for built/ and the generated sources."""
    files = {}
    for dirpath, _, names in os.walk(os.path.join(tree, "built")):
        for name in names:
            full = os.path.join(dirpath, name)
            rel = os.path.relpath(full, tree)
            if os.path.islink(full):
                files[rel] = ("link:" + os.readlink(full), 0)
            else:
                files[rel] = file_digest(full, tree)
    for rel in GENERATED:
        full = os.path.join(tree, rel)
        if os.path.exists(full):
            files[rel] = file_digest(full, tree)
    return files


def compare_snapshots(a, b):
    """Lines describing the differences between two snapshots (empty when identical)."""
    lines = []
    for rel in sorted(set(a) | set(b)):
        if rel not in b:
            lines.append(f"only on node: {rel}")
        elif rel not in a:
            lines.append(f"only on jsse: {rel}")
        elif a[rel] != b[rel]:
            lines.append(
                f"differs: {rel} (node {a[rel][1]} bytes, jsse {b[rel][1]} bytes)"
            )
    return lines


def diff_logs(node_lines, jsse_lines, limit=60):
    a, b = sorted(node_lines), sorted(jsse_lines)
    if a == b:
        return []
    diff = list(difflib.unified_diff(a, b, "node", "jsse", lineterm="", n=1))
    if len(diff) > limit:
        diff = diff[:limit] + [f"... {len(diff) - limit} more diff lines"]
    return diff


def check_checkout(ts_dir):
    """Return None when ts_dir is a usable TypeScript 6.0.3 checkout, else the reason it is not."""
    pkg = os.path.join(ts_dir, "package.json")
    if not os.path.isfile(pkg):
        return f"{ts_dir} has no package.json"
    with open(pkg, encoding="utf-8") as f:
        info = json.load(f)
    if info.get("name") != "typescript" or not str(info.get("version", "")).startswith(
        "6.0.3"
    ):
        return f"{ts_dir} is TypeScript {info.get('version')}, not 6.0.3"
    for rel in (
        "node_modules/hereby/bin/hereby.js",
        "node_modules/typescript/lib/tsc.js",
        "Herebyfile.mjs",
    ):
        if not os.path.isfile(os.path.join(ts_dir, rel)):
            return f"{ts_dir} lacks {rel} (run npm ci)"
    return None


def clone_clean(src, dst):
    """Copy a checkout to dst without its build output or other ignored files (node_modules is kept)."""
    copy_tree(src, dst)
    ignored = subprocess.run(
        ["git", "-C", src, "status", "--ignored", "--porcelain"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.splitlines()
    for entry in ignored:
        if not entry.startswith("!! "):
            continue
        rel = entry[3:].rstrip("/")
        if rel == "node_modules" or rel.startswith("node_modules/"):
            continue
        target = os.path.join(dst, rel)
        if os.path.isdir(target) and not os.path.islink(target):
            shutil.rmtree(target)
        elif os.path.lexists(target):
            os.remove(target)


def is_dts_gap(code, err):
    """True when a failed build is only the d.ts bundling step missing WebAssembly."""
    if code != 1 or not all(marker in err for marker in DTS_GAP):
        return False
    failed = re.findall(r"^Error in (\S+)", err, re.M)
    return all(task == "dts-services" for task in failed)


def hereby_local(engine, exe, tree, timeout):
    """Run `hereby local` in tree; return (exit code, stdout, stderr, seconds)."""
    env = {k: v for k, v in os.environ.items() if k not in DROP_ENV}
    launch = [exe] + (["--node"] if engine == "jsse" else [])
    argv = launch + [
        os.path.join("node_modules", "hereby", "bin", "hereby.js"),
        "local",
    ]
    start = time.monotonic()
    proc = subprocess.Popen(
        argv,
        cwd=tree,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        errors="replace",
        start_new_session=True,
    )
    try:
        out, err = proc.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        out, err = proc.communicate()
        return (
            124,
            out,
            err + f"\ntimed out after {timeout}s\n",
            time.monotonic() - start,
        )
    return proc.returncode, out, err, time.monotonic() - start


def copy_tree(src, dst):
    subprocess.run(["cp", "-a", "--reflink=auto", src, dst], check=True)


def package_compiler(tree, pkg):
    """Lay the built compiler out as a typescript package (lib/ is built/local) for the tsc oracles."""
    shutil.rmtree(pkg, ignore_errors=True)
    os.makedirs(pkg)
    copy_tree(os.path.join(tree, "built", "local"), os.path.join(pkg, "lib"))
    for rel in ("bin", "package.json"):
        copy_tree(os.path.join(tree, rel), os.path.join(pkg, rel))


def run_harness(pkg, jsse, jobs, ci):
    argv = [
        "uv",
        "run",
        "python",
        os.path.join(ROOT, "scripts", "run-tsc-projects-oracle.py"),
        "--jobs",
        str(jobs),
        "--require",
    ]
    if ci:
        argv.append("--ci")
    env = {**os.environ, "TSC_DIR": pkg, "JSSE": jsse}
    startup = [
        os.path.join(ROOT, "scripts", "run-tsc-oracle.sh"),
        "--full",
        "--no-timing",
    ]
    codes = [subprocess.run(a, env=env).returncode for a in (startup, argv)]
    return max(codes)


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--jsse")
    ap.add_argument("--work")
    ap.add_argument("--timeout", type=int, default=7200)
    ap.add_argument("--harness-jobs", type=int, default=4)
    ap.add_argument(
        "--full",
        action="store_true",
        help="run the whole `local` task, d.ts bundles included (needs WebAssembly on jsse)",
    )
    ap.add_argument("--skip-harness", action="store_true")
    ap.add_argument("--require", action="store_true")
    ap.add_argument("--ci", action="store_true")
    o = ap.parse_args()

    def skip(msg):
        if o.require:
            print(f"ERROR: {msg}", file=sys.stderr)
            return 1
        print(f"SKIP: {msg}")
        return 0

    if not os.environ.get("TS_SRC_DIR"):
        return skip(
            "TS_SRC_DIR is not set; point it at a TypeScript v6.0.3 checkout (npm ci done) to run this oracle"
        )
    ts_dir = os.path.realpath(os.environ["TS_SRC_DIR"])
    problem = check_checkout(ts_dir)
    if problem:
        return skip(problem)
    jsse = os.path.abspath(
        o.jsse
        or os.environ.get("JSSE")
        or os.path.join(ROOT, "target", "release", "jsse")
    )
    if not os.access(jsse, os.X_OK):
        print(f"jsse binary not found or not executable: {jsse}", file=sys.stderr)
        return 1
    node = shutil.which("node")
    if not node:
        return skip("node is not on PATH")

    work = (
        os.path.abspath(o.work)
        if o.work
        else tempfile.mkdtemp(prefix="jsse-hereby-local-")
    )
    os.makedirs(work, exist_ok=True)
    print(
        f"hereby local from {ts_dir} (node: {node}, jsse: {jsse}, work: {work})",
        flush=True,
    )
    engines = [("node", node), ("jsse", jsse)]
    trees = {name: os.path.join(work, name, "ts") for name, _ in engines}
    for name, _ in engines:
        shutil.rmtree(os.path.join(work, name), ignore_errors=True)
        os.makedirs(os.path.join(work, name))
        clone_clean(ts_dir, trees[name])

    results = {}
    for name, exe in engines:
        print(f"hereby local on {name} ...", flush=True)
        code, out, err, secs = hereby_local(name, exe, trees[name], o.timeout)
        results[name] = (code, out, err, secs)
        for stream, text in (("stdout", out), ("stderr", err)):
            with open(
                os.path.join(work, f"{name}-{stream}.log"), "w", encoding="utf-8"
            ) as f:
                f.write(text)
        print(f"  exit {code} in {secs:.1f}s", flush=True)

    problems = []
    gap = not o.full and is_dts_gap(results["jsse"][0], results["jsse"][2])
    for name, _ in engines:
        if results[name][0] != 0 and not (gap and name == "jsse"):
            problems.append(
                f"hereby exited {results[name][0]} on {name}:\n"
                + results[name][2][-3000:]
            )
    if gap:
        print(
            "  known gap: dts-services needs WebAssembly; the d.ts bundles are not compared"
        )
    logs = {
        name: normalise_log(results[name][1] + results[name][2], trees[name], exe)
        for name, exe in engines
    }
    log_diff = diff_logs(logs["node"], logs["jsse"])
    if log_diff and not problems and not gap:
        problems.append("build logs differ:\n" + "\n".join(log_diff))
    shots = {name: snapshot(trees[name]) for name, _ in engines}
    if gap:
        for rel in DTS_FILES:
            shots["node"].pop(rel, None)
    print(
        f"  built files: node {len(shots['node'])}, jsse {len(shots['jsse'])}",
        flush=True,
    )
    file_diff = compare_snapshots(shots["node"], shots["jsse"])
    if file_diff:
        problems.append("build output differs:\n" + "\n".join(file_diff[:80]))

    print(f"\n{'engine':<8}{'seconds':>10}")
    for name, _ in engines:
        print(f"{name:<8}{results[name][3]:>10.1f}")
    if problems:
        for p in problems:
            print(
                ("::error title=hereby local oracle::" if o.ci else "") + p,
                file=sys.stderr,
            )
        print(
            f"\nFAIL: hereby local differs between node and jsse (work dir kept: {work})"
        )
        return 1
    print("builds identical: logs and every file under built/ match")

    if not o.skip_harness:
        pkg = os.path.join(work, "pkg-jsse")
        package_compiler(trees["jsse"], pkg)
        print(f"\ntsc project oracle on the jsse-built compiler ({pkg})", flush=True)
        if run_harness(pkg, jsse, o.harness_jobs, o.ci) != 0:
            print(
                f"\nFAIL: the tsc oracle differs on the jsse-built compiler (work dir kept: {work})"
            )
            return 1
    if not o.work:
        shutil.rmtree(work, ignore_errors=True)
    print(
        "\nOK: hereby local on jsse --node matches Node"
        + (" except the d.ts bundles (no WebAssembly)" if gap else "")
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
