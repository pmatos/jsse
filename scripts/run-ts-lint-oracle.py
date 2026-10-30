#!/usr/bin/env python3
"""Run TypeScript's `hereby lint` (ESLint 10 and typescript-eslint) on Node and on jsse --node and diff them (#1295).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-lint-oracle.py [options] [case ...]

TS_SRC_DIR is a TypeScript v6.0.3 checkout with `npm ci` done and the diagnostics generated (the repository this
oracle was written for is ts-suite/pin.json). Without it the script prints SKIP and exits 0 unless --require.
Each case (scripts/ts_lint_cases.py) runs one or more steps in a private tree per engine; the tree is never the
checkout. Compared between Node and jsse: the exit status of every step, its stdout and stderr (the tree path,
durations and the interpreter named by `> exe args` lines normalised), and every file the steps created, changed or
removed in the tree (ESLint's `--fix` rewrites and the `.eslintcache` file, mtimes normalised).

The default cases are the quick ones: ESLint's version and `--print-config` for a source, a script and a d.ts file;
the control files of scripts/ts-lint-fixtures (about thirty diagnostics of every kind, with `--fix`, the formatters,
a warning that `--max-warnings 0` turns into a failure, the cache) linted in the scripts program and in the type-aware
program of src/. The case `full` runs `hereby lint` over the whole repository twice (the second run reads the cache)
and takes about 45 s on Node and over an hour on jsse; name it to run it. Node must satisfy each case's own checks
(a control that reports nothing would make the comparison vacuous). A case that differs on jsse must be listed in
ts-suite/lint-known-failures.txt (`case<TAB>issue`); a listed case that now matches fails the run.

Options: --jobs N (default 4, at most 4), --timeout SECONDS (per step, default 7200), --jsse PATH (else $JSSE, else
target/release/jsse), --known FILE, --work DIR (kept), --list, --require, --verbose, --no-jsse.
Cases: names, globs, `cheap`, `all`.
"""

import argparse
import difflib
import hashlib
import os
import re
import shutil
import sys
import tempfile
import json
from concurrent.futures import ThreadPoolExecutor

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(SCRIPTS)
sys.path.insert(0, SCRIPTS)
import oracle_procs  # noqa: E402
import ts_lint_cases as lc  # noqa: E402

FIXTURES = os.path.join(SCRIPTS, "ts-lint-fixtures")
KNOWN = os.path.join(ROOT, "ts-suite", "lint-known-failures.txt")
DROP_ENV = (
    "NODE_OPTIONS",
    "FORCE_COLOR",
    "NO_COLOR",
    "COLUMNS",
    "NODE_DISABLE_COLORS",
    "CI",
)
LINKED = ("node_modules", "tests", "built", "lib", ".git")
FIXED_MTIME = 1700000000
DURATION = re.compile(r"(?<= in )\d[\d.]*(?:ms|s|m|h|d)(?: \d[\d.]*(?:ms|s|m|h|d))*")
EXEC_LINE = re.compile(r"^> \S+ ")
MTIME = re.compile(r'("mtime":\s*)\d+')
GENERATED_ON = re.compile(r"(Generated on ).*")


def tree_spellings(tree):
    names = {tree, os.path.realpath(tree)}
    home = os.path.expanduser("~")
    names |= {"~" + n[len(home) :] for n in list(names) if n.startswith(home + os.sep)}
    return sorted(names, key=len, reverse=True)


def normalise(text, tree, exe):
    """Make the text of two engines comparable: tree paths, durations, the interpreter, cache mtimes."""
    for old in tree_spellings(tree):
        text = text.replace(old, "<TREE>")
    for name in {exe, os.path.realpath(exe)}:
        text = text.replace(name, "<EXEC>")
    out = []
    for line in text.splitlines():
        line = DURATION.sub("<T>", line)
        if EXEC_LINE.match(line):
            line = "> <EXEC> " + line.split(" ", 2)[2]
        line = MTIME.sub(r"\g<1>0", line.replace(exe, "<EXEC>"))
        out.append(GENERATED_ON.sub(r"\g<1><DATE>", line))
    return "\n".join(out)


def normalise_cache(text):
    """ESLint's `.eslintcache` (a flatted JSON array) with each `hashOfConfig` made a number by first use.

    The hash covers the config, which names the tree (`tsconfigRootDir`), so two trees never share one; what must
    agree between engines is which files share a config.
    """
    try:
        data = json.loads(text)
    except ValueError:
        return text
    slots = {}
    for entry in data:
        if isinstance(entry, dict) and "hashOfConfig" in entry:
            ref = entry["hashOfConfig"]
            if isinstance(ref, str) and ref.isdigit() and int(ref) < len(data):
                slots.setdefault(int(ref), len(slots))
    for index, number in slots.items():
        data[index] = f"config-{number}"
    return json.dumps(data, separators=(",", ":"))


def check_checkout(ts_dir):
    """None when ts_dir can run the oracle, else the reason it cannot."""
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
        "node_modules/eslint/bin/eslint.js",
        "node_modules/hereby/bin/hereby.js",
        "eslint.config.mjs",
        "src/compiler/diagnosticInformationMap.generated.ts",
    ):
        if not os.path.isfile(os.path.join(ts_dir, rel)):
            return f"{ts_dir} lacks {rel} (run npm ci and hereby generate-diagnostics)"
    return None


def copy(src, dst):
    import subprocess

    subprocess.run(["cp", "-a", "--reflink=auto", src, dst], check=True)


def make_tree(ts_dir, tree, case):
    """The case's private tree: the whole checkout, or its sources with the heavy directories linked."""
    if case.full:
        copy(ts_dir, tree)
    else:
        os.makedirs(tree)
        for name in sorted(os.listdir(ts_dir)):
            src, dst = os.path.join(ts_dir, name), os.path.join(tree, name)
            if name == ".git":
                continue
            if name in LINKED:
                os.symlink(src, dst)
            else:
                copy(src, dst)
    cache = os.path.join(tree, ".eslintcache")
    if os.path.exists(cache):
        os.remove(cache)
    for rel in case.controls:
        dst = os.path.join(tree, rel)
        shutil.copyfile(os.path.join(FIXTURES, lc.FIXTURES[rel]), dst)
        os.utime(dst, (FIXED_MTIME, FIXED_MTIME))


def snapshot(tree):
    """{relative path: sha256} of the tree's own files (linked or heavy directories left out)."""
    files = {}
    for dirpath, dirs, names in os.walk(tree):
        dirs[:] = sorted(d for d in dirs if not (dirpath == tree and d in LINKED))
        for n in names:
            full = os.path.join(dirpath, n)
            if os.path.islink(full):
                continue
            with open(full, "rb") as f:
                files[os.path.relpath(full, tree)] = hashlib.sha256(
                    f.read()
                ).hexdigest()
    return files


def argv_of(step, engine_exe, node_host):
    launch = [engine_exe] + (["--node"] if node_host else [])
    if step.tool == "hereby":
        return launch + [
            os.path.join("node_modules", "hereby", "bin", "hereby.js"),
            *step.args,
        ]
    base = [os.path.join("node_modules", "eslint", "bin", "eslint.js")]
    flags = step.args if step.plain else [*lc.LINT_FLAGS, *step.args]
    return launch + base + flags


def run_case(case, engine, exe, ts_dir, work, timeout):
    """Run the steps of `case` on one engine; return a result dict."""
    tree = os.path.join(work, case.name, engine, "ts")
    shutil.rmtree(os.path.dirname(tree), ignore_errors=True)
    os.makedirs(os.path.dirname(tree))
    make_tree(ts_dir, tree, case)
    before = snapshot(tree)
    env = {k: v for k, v in os.environ.items() if k not in DROP_ENV}
    steps = []
    for step in case.steps:
        code, out, err = oracle_procs.run_group(
            argv_of(step, exe, engine == "jsse"), tree, env, timeout
        )
        steps.append(
            {
                "exit": code,
                "stdout": normalise(out.decode("utf-8", "replace"), tree, exe),
                "stderr": normalise(err.decode("utf-8", "replace"), tree, exe),
            }
        )
    after = snapshot(tree)
    changed = {}
    for rel in sorted(set(before) | set(after)):
        if before.get(rel) == after.get(rel):
            continue
        if rel not in after:
            changed[rel] = None
            continue
        with open(os.path.join(tree, rel), "rb") as f:
            body = f.read().decode("utf-8", "replace")
        if os.path.basename(rel) == ".eslintcache":
            body = normalise_cache(body)
        changed[rel] = normalise(body, tree, exe)
    return {"steps": steps, "changed": changed}


def check_must(case, result):
    """Problems with the reference (Node) result: a control that proves nothing is a failure of its own."""
    problems = []
    last = result["steps"][-1]
    text = {
        "stdout": "\n".join(s["stdout"] for s in result["steps"]),
        "stderr": "\n".join(s["stderr"] for s in result["steps"]),
        "exit": last["exit"],
    }
    for kind, pattern in case.must:
        if kind.startswith("file "):
            body = result["changed"].get(kind[5:])
            if body is None or not re.search(pattern, body):
                problems.append(f"expected {kind} to match /{pattern}/ on node")
        elif not re.search(pattern, text[kind], re.M):
            problems.append(f"expected {kind} to match /{pattern}/ on node")
    return problems


def diff_text(a, b, label, limit=40):
    if a == b:
        return []
    d = list(
        difflib.unified_diff(
            a.splitlines(), b.splitlines(), "node", "jsse", lineterm="", n=1
        )
    )
    if len(d) > limit:
        d = d[:limit] + [f"... {len(d) - limit} more diff lines"]
    return [f"{label}:"] + d


def compare(node, jsse):
    """Lines describing how two results differ (empty when they are identical)."""
    lines = []
    for i, (a, b) in enumerate(zip(node["steps"], jsse["steps"]), 1):
        if a["exit"] != b["exit"]:
            lines.append(f"step {i}: exit {a['exit']} on node, {b['exit']} on jsse")
        lines += diff_text(a["stdout"], b["stdout"], f"step {i} stdout")
        lines += diff_text(a["stderr"], b["stderr"], f"step {i} stderr")
    for rel in sorted(set(node["changed"]) | set(jsse["changed"])):
        if rel not in jsse["changed"]:
            lines.append(f"file {rel}: changed on node only")
        elif rel not in node["changed"]:
            lines.append(f"file {rel}: changed on jsse only")
        else:
            a, b = node["changed"][rel], jsse["changed"][rel]
            lines += diff_text(a or "", b or "", f"file {rel}")
    return lines


def load_known(path):
    known = {}
    if os.path.isfile(path):
        with open(path, encoding="utf-8") as f:
            for line in f:
                if line.strip() and not line.startswith("#"):
                    name, _, issue = line.rstrip("\n").partition("\t")
                    known[name] = issue
    return known


def verdict(case, node, jsse, known):
    """(status, lines): PASS, KNOWN, FAIL or STALE."""
    problems = check_must(case, node)
    if problems:
        return "FAIL", [
            "the Node run does not satisfy the case's own checks:"
        ] + problems
    diffs = compare(node, jsse)
    listed = case.name in known
    if diffs and listed:
        return "KNOWN", [f"differs ({known[case.name]})"]
    if diffs:
        return "FAIL", diffs
    if listed:
        return "STALE", [
            f"listed in the known failures ({known[case.name]}) but matches Node: remove it"
        ]
    return "PASS", []


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("cases", nargs="*")
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--timeout", type=int, default=7200)
    ap.add_argument("--jsse")
    ap.add_argument("--known", default=KNOWN)
    ap.add_argument("--work")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--require", action="store_true")
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument("--no-jsse", action="store_true")
    o = ap.parse_args()
    table = lc.cases()
    chosen = lc.select(o.cases, table)
    if o.list:
        for c in table:
            print(f"{c.name:<24}{'default' if c.default else 'named':<9}{c.notes}")
        return 0

    def skip(msg):
        print(
            ("ERROR: " if o.require else "SKIP: ") + msg,
            file=sys.stderr if o.require else sys.stdout,
        )
        return 1 if o.require else 0

    if not os.environ.get("TS_SRC_DIR"):
        return skip(
            "TS_SRC_DIR is not set; point it at a TypeScript v6.0.3 checkout (npm ci done)"
        )
    ts_dir = os.path.realpath(os.environ["TS_SRC_DIR"])
    problem = check_checkout(ts_dir)
    if problem:
        return skip(problem)
    node = shutil.which("node")
    if not node:
        return skip("node is not on PATH")
    jsse = os.path.abspath(
        o.jsse
        or os.environ.get("JSSE")
        or os.path.join(ROOT, "target", "release", "jsse")
    )
    if not o.no_jsse and not os.access(jsse, os.X_OK):
        print(f"jsse binary not found or not executable: {jsse}", file=sys.stderr)
        return 1
    jobs = max(1, min(o.jobs, 4))
    work = (
        os.path.abspath(o.work) if o.work else tempfile.mkdtemp(prefix="jsse-ts-lint-")
    )
    os.makedirs(work, exist_ok=True)
    oracle_procs.install_cleanup()
    known = load_known(o.known)
    engines = [("node", node)] + ([] if o.no_jsse else [("jsse", jsse)])
    print(
        f"lint oracle: {len(chosen)} case(s) from {ts_dir} (node {node}, jsse {jsse}, work {work})",
        flush=True,
    )
    tasks = sorted(
        ((c, e, x) for c in chosen for e, x in engines),
        key=lambda t: (not t[0].full, t[0].name),
    )

    def go(task):
        c, e, x = task
        try:
            r = run_case(c, e, x, ts_dir, work, o.timeout)
        except Exception as exc:
            r = {"error": f"{type(exc).__name__}: {exc}"}
        if o.verbose:
            print(f"  done {c.name} on {e}", flush=True)
        return (c.name, e), r

    with ThreadPoolExecutor(jobs) as pool:
        results = dict(pool.map(go, tasks))
    bad = 0
    for c in chosen:
        errors = [
            f"{e} run crashed: {results[(c.name, e)]['error']}"
            for e, _ in engines
            if "error" in results[(c.name, e)]
        ]
        if errors:
            status, lines = "FAIL", errors
        elif o.no_jsse:
            problems = check_must(c, results[(c.name, "node")])
            status, lines = ("FAIL", problems) if problems else ("PASS", [])
        else:
            status, lines = verdict(
                c, results[(c.name, "node")], results[(c.name, "jsse")], known
            )
        print(f"{status:<6}{c.name}")
        for ln in lines:
            print("      " + ln)
        bad += status in ("FAIL", "STALE")
    if bad:
        print(f"\nlint oracle: FAIL ({bad} of {len(chosen)}; work dir kept: {work})")
        return 1
    if not o.work:
        shutil.rmtree(work, ignore_errors=True)
    print(f"\nlint oracle: PASS ({len(chosen)} case(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())
