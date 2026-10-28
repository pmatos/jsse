#!/usr/bin/env python3
"""Run mocha 10 and the TypeScript test runner's unit tests on Node and on jsse --node and diff them (issue #1255).

    TS_SRC_DIR=/path/to/TypeScript uv run python scripts/run-ts-mocha-oracle.py [options] [case-glob ...]

TS_SRC_DIR is a TypeScript v6.0.3 checkout with `npm ci` and `hereby local` done (node_modules/mocha and
built/local/run.js); without it the script prints SKIP and exits 0 unless --require is given. The cases start
mocha's CLI (node_modules/mocha/bin/_mocha, the way `hereby runtests` does) with every reporter on the fixtures in
scripts/ts-mocha-fixtures (passing, failing, timing-out, uncaught and late errors, hooks, exit hooks) and on
the TypeScript runner built/local/run.js, whose test.config selects one trivial unit-test suite. Stdout, stderr,
the exit code and the files the run leaves behind (.failed-tests, TEST-results.xml) must be identical after
normalising paths, durations and the stack frames jsse does not have (see normalise()). Never builds anything.
Exits 1 on any difference.

Each run gets its own working directory of symlinks into the checkout (built, node_modules, scripts, src, the test
inputs), so runs neither write into the checkout nor see each other's test.config.

Options: --jobs N (default 4), --timeout SECONDS (default 1800), --ci, --list, --require,
--jsse PATH (else $JSSE, else target/release/jsse).
"""

import argparse
import difflib
import fnmatch
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ts_runner_cli_cases as cli  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIX = os.path.join(ROOT, "scripts", "ts-mocha-fixtures")
DROP_ENV = (
    "NODE_OPTIONS",
    "FORCE_COLOR",
    "NO_COLOR",
    "COLUMNS",
    "NODE_DISABLE_COLORS",
    "CI",
)
LINKS = ("built", "node_modules", "scripts", "lib", "src", "package.json")
TEST_INPUTS = ("cases", "lib")
COMMON = ["--no-colors", "-t", "40000"]
UNIT_CONFIG = {"test": ["unittests:: semver"], "runners": ["unittest"], "light": False}
FAILED_TESTS = "scripts/failed-tests.cjs"

# mocha cases: name, mocha arguments, spec files (fixtures or "RUN" for built/local/run.js), test.config (None =
# none), env. The `runtests` and `accept` cases come from ts_runner_cli_cases.py (issue #1257).
CASES = []


def case(name, args, specs, config=None, env=None):
    CASES.append(cli.Case(name, args, specs, config, env or {}))


for reporter in (
    "spec",
    "dot",
    "tap",
    "min",
    "list",
    "nyan",
    "json",
    "xunit",
    "markdown",
    "landing",
    "progress",
):
    case(f"pass-{reporter}", ["-R", reporter], ["pass.spec.js"])
    case(f"fail-{reporter}", ["-R", reporter], ["fail.spec.js"])
case("async-spec", ["-R", "spec", "-t", "2000"], ["async.spec.js"])
case("async-dot", ["-R", "dot", "-t", "2000"], ["async.spec.js"])
case("late-spec", ["-R", "spec"], ["late.spec.js"])
case("exit-spec", ["-R", "spec"], ["exit.spec.js"])
case("exit-flag", ["-R", "spec", "--exit"], ["exit.spec.js"])
case("slow-spec", ["-R", "spec"], ["slow.spec.js"])
case("bail", ["-R", "spec", "--bail"], ["fail.spec.js"])
case("grep", ["-R", "spec", "-g", "generated"], ["pass.spec.js"])
case("grep-invert", ["-R", "dot", "-g", "sync", "-i"], ["pass.spec.js"])
case("fgrep", ["-R", "spec", "-f", "leaf"], ["pass.spec.js"])
case("all-specs", ["-R", "dot"], ["pass.spec.js", "fail.spec.js", "late.spec.js"])
case("no-specs", ["-R", "spec"], [])
case("missing-spec", ["-R", "spec"], ["absent.spec.js"])
case("bad-reporter", ["-R", "no-such-reporter"], ["pass.spec.js"])
case("failed-tests-pass", ["-R", FAILED_TESTS, "-O", "reporter=dot"], ["pass.spec.js"])
case("failed-tests-fail", ["-R", FAILED_TESTS, "-O", "reporter=dot"], ["fail.spec.js"])
case(
    "failed-tests-keep",
    ["-R", FAILED_TESTS, "-O", "reporter=dot,keepFailed=true"],
    ["pass.spec.js"],
)
case(
    "failed-tests-xunit", ["-R", FAILED_TESTS, "-O", "reporter=xunit"], ["fail.spec.js"]
)

TS_REPORTERS = ("dot", "spec", "min", "mocha-fivemat-progress-reporter")
for reporter in TS_REPORTERS:
    case(
        f"ts-semver-{reporter}",
        ["-R", FAILED_TESTS, "-O", f"reporter={reporter}", "-g", "unittests:: semver"],
        ["RUN"],
        UNIT_CONFIG,
    )
case(
    "ts-semver-plain-reporter",
    ["-R", "dot", "-g", "unittests:: semver"],
    ["RUN"],
    UNIT_CONFIG,
)
case("ts-no-match", ["-R", "spec", "-g", "no such test anywhere"], ["RUN"], UNIT_CONFIG)
case(
    "ts-semver-and-failures",
    ["-R", FAILED_TESTS, "-O", "reporter=spec", "-g", "unittests:: semver|^fail "],
    ["RUN", "fail.spec.js"],
    UNIT_CONFIG,
)
case(
    "ts-semver-fail-dot",
    ["-R", FAILED_TESTS, "-O", "reporter=dot", "-g", "unittests:: semver|^fail "],
    ["RUN", "fail.spec.js"],
    UNIT_CONFIG,
)
case(
    "ts-semver-xunit-ci",
    ["-R", FAILED_TESTS, "-O", "reporter=xunit", "-g", "unittests:: semver"],
    ["RUN"],
    UNIT_CONFIG,
    {"CI": "true"},
)
case(
    "ts-config-light",
    ["-R", "dot", "-g", "unittests:: semver"],
    ["RUN"],
    dict(
        UNIT_CONFIG,
        light=True,
        timeout=40000,
        skipSysTests=True,
        stackTraceLimit="full",
    ),
)
case(
    "ts-skip-unit-tests",
    ["-R", "spec", "-g", "unittests:: semver"],
    ["RUN"],
    {"test": ["unittests:: semver"], "runners": ["conformance"]},
)

CASES.extend(cli.cli_cases())

EXEC_LINE = re.compile(r"^> \S+ ", re.M)
HEREBY_TIME = re.compile(r"(?<= in )\d[\d.]*(?:ms|s|m)\b")
DURATION = re.compile(r"\(\d+(?:\.\d+)?(?:ms|s|m)\)|(?<=: )\d+(?:\.\d+)?ms$", re.M)
JSON_TIME = re.compile(r'("(?:duration|start|end)": )(?:"[^"]*"|\d+)')
XML_TIME = re.compile(r' (time|timestamp)="[^"]*"')
FRAME = re.compile(r"^\s+at ")
ASYNC_FRAME = re.compile(r"^\s+at async ")
BUILTIN_FRAME = re.compile(r"\(?node:[\w/]+:\d+:\d+\)?( \{|</failure></testcase>)?$")
ARROW = r"(?:[\w$]+\.(?:<anonymous>|&#x3C;anonymous&#x3E;)|Timeout\._onTimeout|Immediate\._onImmediate|exports\.[\w$]+)"
RECEIVER_FRAME = re.compile(r"^(\s+at )" + ARROW + r" \((.*:\d+:\d+)\)")
PID = re.compile(r"\(node:\d+\)")
SLOW_MARK = re.compile(r"^(\s*✔ .*?) \(<T>\)$", re.M)


def normalise(text, work, ts_dir):
    """Make two runs comparable: paths, durations, pids, and stack frames that differ between the engines.

    Dropped on both sides: frames inside Node's own modules (jsse's node-lib has other line numbers) and
    `at async` frames (issue #1368). `at Context.<anonymous> (file:1:2)` and `at Timeout._onTimeout (file:1:2)`
    are read as `at file:1:2`: V8 names the receiver of an arrow function and infers `exports.name` for a
    function assigned to it; jsse does neither. Mocha shows frames under the working directory relative to it.
    The spec reporter appends a duration to a passing test slower than half its threshold, so a loaded host
    adds `(<T>)` to a ✔ line on either side: dropped.
    """
    text = text.replace(work, "<WORK>").replace(ts_dir, "<TS>")
    text = DURATION.sub(lambda m: "(<T>)" if m.group().startswith("(") else "<T>", text)
    text = SLOW_MARK.sub(r"\1", text)
    text = JSON_TIME.sub(r"\1<T>", text)
    text = XML_TIME.sub(r' \1="<T>"', text)
    text = PID.sub("(node:<PID>)", text)
    kept = []
    for line in text.splitlines():
        if FRAME.match(line) and (
            BUILTIN_FRAME.search(line) or ASYNC_FRAME.match(line)
        ):
            builtin = BUILTIN_FRAME.search(line)
            if builtin and builtin.group(1):
                kept.append(builtin.group(1).strip())
            continue
        if FRAME.match(line):
            line = RECEIVER_FRAME.sub(r"\1\2", line).replace("<WORK>/", "")
        kept.append(line)
    return "\n".join(kept) + "\n"


def normalise_json(text, work, ts_dir):
    """The json reporter embeds stacks in strings and lists error keys in own-property order (V8 puts `stack`
    first, jsse after `message`): normalise the stacks line by line and sort the keys."""
    try:
        doc = json.loads(text)
    except ValueError:
        return text

    def walk(v):
        if isinstance(v, str):
            return normalise(v, work, ts_dir).rstrip("\n")
        if isinstance(v, list):
            return [walk(x) for x in v]
        if isinstance(v, dict):
            return {k: walk(x) for k, x in v.items()}
        return v

    return json.dumps(walk(doc), indent=1, sort_keys=True) + "\n"


def prepare(work, engine, c, ts_dir):
    cwd = os.path.join(work, engine, c.name)
    os.makedirs(os.path.join(cwd, "tests", "baselines", "local"))
    for link in LINKS:
        os.symlink(os.path.join(ts_dir, link), os.path.join(cwd, link))
    for link in TEST_INPUTS:
        os.symlink(
            os.path.join(ts_dir, "tests", link), os.path.join(cwd, "tests", link)
        )
    if c.kind == "accept":
        shutil.copy(os.path.join(ts_dir, "Herebyfile.mjs"), cwd)
        cli.seed_baselines(cwd)
    else:
        os.symlink(
            os.path.join(ts_dir, "tests", "baselines", "reference"),
            os.path.join(cwd, "tests", "baselines", "reference"),
        )
    for spec in c.specs:
        if spec != "RUN" and os.path.exists(os.path.join(FIX, spec)):
            shutil.copy(os.path.join(FIX, spec), cwd)
    if c.mocharc:
        with open(os.path.join(cwd, ".mocharc.json"), "w") as f:
            json.dump(c.mocharc, f)
    os.makedirs(os.path.join(cwd, "tmp"))
    return cwd


def collect_files(cwd, c):
    found = {}
    for name in (".failed-tests", "TEST-results.xml"):
        path = os.path.join(cwd, name)
        if os.path.isfile(path):
            with open(path, encoding="utf-8", errors="replace") as f:
                found[name] = f.read()
    if c.kind == "runtests":
        found["tree baselines"] = cli.tree(
            os.path.join(cwd, "tests", "baselines", "local")
        )
    if c.kind == "accept":
        found["tree reference"] = cli.tree(
            os.path.join(cwd, "tests", "baselines", "reference"), content=True
        )
    return found


def command(engine, exe, cwd, c, flags):
    node_flag = ["--node"] if engine == "jsse" else []
    if c.kind == "accept":
        return (
            [exe] + node_flag + ["node_modules/hereby/bin/hereby.js", "baseline-accept"]
        )
    if c.kind == "runtests":
        return [exe] + node_flag + ["runtests.mjs"] + flags
    mocha = os.path.join(cwd, "node_modules", "mocha", "bin", "_mocha")
    files = ["built/local/run.js" if s == "RUN" else s for s in c.specs]
    return [exe] + node_flag + [mocha] + COMMON + flags + files


def launch(cmd, cwd, env, timeout):
    start = time.monotonic()
    child = subprocess.Popen(
        cmd,
        cwd=cwd,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
    )
    try:
        out, err = child.communicate(timeout=timeout)
        code = str(child.returncode)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        out, err = child.communicate()
        code = "timeout"
    return code, out, err, time.monotonic() - start


def execute(engine, exe, cwd, c, flags, env_extra, timeout, ts_dir, work, prefix=""):
    env = {k: v for k, v in os.environ.items() if k not in DROP_ENV}
    env["NODE_ENV"] = "development"
    env["TMPDIR"] = os.path.join(cwd, "tmp")
    env.update(c.env)
    env.update(env_extra)
    if c.config is not None:
        with open(os.path.join(cwd, "test.config"), "w") as f:
            json.dump(c.config, f)
    code, out, err, secs = launch(
        command(engine, exe, cwd, c, flags), cwd, env, timeout
    )
    wd = os.path.join(work, engine)

    def norm(raw):
        text = raw.decode("utf-8", "replace")
        if "json" in flags:
            text = normalise_json(text, cwd, ts_dir)
        text = normalise(text, cwd, ts_dir).replace(wd, "<WORK>")
        if c.kind == "runtests":
            text = EXEC_LINE.sub("> <EXEC> ", text)
        return HEREBY_TIME.sub("<T>", text) if c.kind == "accept" else text

    result = {
        prefix + "exit": code,
        prefix + "stdout": norm(out),
        prefix + "stderr": norm(err),
    }
    for name, text in collect_files(cwd, c).items():
        result[prefix + "file " + name] = norm(text.encode())
    return result, secs


def run_case(c, engine, exe, ts_dir, work, timeout):
    cwd = prepare(work, engine, c, ts_dir)
    result, secs = execute(engine, exe, cwd, c, c.args, {}, timeout, ts_dir, work)
    if c.then:
        flags, env_extra = c.then
        more, more_secs = execute(
            engine, exe, cwd, c, flags, env_extra, timeout, ts_dir, work, "then "
        )
        result.update(more)
        secs += more_secs
    result["secs"] = secs
    return result


def diff_parts(a, b):
    keys = sorted((set(a) | set(b)) - {"secs"})
    return {
        key: (a.get(key, "<absent>\n"), b.get(key, "<absent>\n"))
        for key in keys
        if a.get(key) != b.get(key)
    }


def render(bad, limit):
    lines = []
    for key, (x, y) in bad.items():
        d = list(
            difflib.unified_diff(
                x.splitlines(), y.splitlines(), "node", "jsse", lineterm="", n=2
            )
        )
        lines.append(f"  {key} differs")
        lines += ["    " + row for row in d[:limit]]
        if len(d) > limit:
            lines.append(f"    ... {len(d) - limit} more diff lines")
    return "\n".join(lines)


def check_checkout(ts_dir):
    mocha = os.path.join(ts_dir, "node_modules", "mocha", "bin", "_mocha")
    run_js = os.path.join(ts_dir, "built", "local", "run.js")
    manifest = os.path.join(ts_dir, "package.json")
    if not os.path.isfile(mocha) or not os.path.isfile(manifest):
        return f"{ts_dir} has no node_modules/mocha (run npm ci in the TypeScript checkout)"
    if not os.path.isfile(run_js):
        return f"{ts_dir} has no built/local/run.js (run hereby tests in the TypeScript checkout)"
    try:
        with open(manifest, encoding="utf-8") as f:
            version = json.load(f).get("version", "unknown")
    except (OSError, ValueError, AttributeError):
        version = "unknown"
    if version != "6.0.3":
        return f"the mocha oracle targets TypeScript 6.0.3, found {version}"
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cases", nargs="*")
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--timeout", type=int, default=1800)
    ap.add_argument("--ci", action="store_true")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--require", action="store_true")
    ap.add_argument("--jsse")
    o = ap.parse_args()

    cases = CASES
    if o.cases:
        picked = [c for c in cases if any(fnmatch.fnmatch(c[0], p) for p in o.cases)]
        unknown = [
            p for p in o.cases if not any(fnmatch.fnmatch(c[0], p) for c in cases)
        ]
        if unknown:
            sys.exit(f"unknown case: {', '.join(unknown)}")
        cases = picked
    if o.list:
        print("\n".join(c[0] for c in cases))
        return 0

    def skip(msg):
        print(("ERROR: " if o.require else "SKIP: ") + msg)
        return 3 if o.require else 0

    if not os.environ.get("TS_SRC_DIR"):
        return skip(
            "TS_SRC_DIR is not set; point it at a built TypeScript v6.0.3 checkout to run the mocha oracle"
        )
    ts_dir = os.path.realpath(os.environ["TS_SRC_DIR"])
    problem = check_checkout(ts_dir)
    if problem:
        return skip(problem)
    node = shutil.which("node")
    if not node:
        return skip("node not found; the oracle needs it")
    given = o.jsse or os.environ.get("JSSE")
    jsse = os.path.abspath(given or os.path.join(ROOT, "target", "release", "jsse"))
    if not os.access(jsse, os.X_OK):
        if given:
            print(f"jsse binary not found or not executable: {jsse}", file=sys.stderr)
            return 1
        print("Building jsse (release)...", flush=True)
        subprocess.run(["cargo", "build", "--release"], cwd=ROOT, check=True)
    print(
        f"mocha from {ts_dir} (jsse: {jsse}, {len(cases)} cases, {o.jobs} jobs)",
        flush=True,
    )

    work = os.path.realpath(tempfile.mkdtemp(prefix="ts-mocha-oracle-"))

    def one(c):
        ref = run_case(c, "node", node, ts_dir, work, o.timeout)
        got = run_case(c, "jsse", jsse, ts_dir, work, o.timeout)
        bad = diff_parts(ref, got)
        for key, pattern in c.must:
            if not re.search(pattern, ref.get(key, ""), re.M):
                bad[f"expectation {key}"] = (
                    f"matches /{pattern}/\n",
                    "Node's own result does not\n",
                )
        if not o.ci:
            print(f"{'ok  ' if not bad else 'FAIL'} {c.name}", flush=True)
        return c, ref, got, bad

    try:
        with ThreadPoolExecutor(o.jobs) as pool:
            results = list(pool.map(one, cases))
        print(f"\n{'case':<34}{'exit':>5}{'node s':>9}{'jsse s':>9}  result")
        failed = 0
        for c, ref, got, bad in results:
            failed += bool(bad)
            what = "PASS" if not bad else "FAIL (" + ", ".join(bad) + ")"
            print(
                f"{c[0]:<34}{ref['exit']:>5}{ref['secs']:>9.1f}{got['secs']:>9.1f}  {what}"
            )
        for c, _, _, bad in results:
            if bad:
                print(f"\n{c[0]}:\n" + render(bad, 400 if o.ci else 40))
                if o.ci:
                    print(
                        f"::error title=mocha oracle::{c[0]} differs between node and jsse: {', '.join(bad)}"
                    )
        problems = cli.partition_problems(results)
        for problem in problems:
            print(f"\nshards: {problem}")
            if o.ci:
                print(f"::error title=mocha oracle::{problem}")
        print(f"\nmocha oracle: {len(results) - failed}/{len(results)} passed")
        return 1 if failed or problems else 0
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
