#!/usr/bin/env python3
"""Fetch, build and run TypeScript's own test suite on Node and record the oracle (issue #1250).

    uv run python scripts/run-ts-suite-oracle.py [options] [stage ...]

Stages (default: fetch install build run check): fetch (clone the pinned tag, verify the
commit), install (verify the lockfile sha256, npm ci), build (hereby local, hereby tests),
run (every runner under Node), record (write ts-suite/oracle.json from the latest run),
check (compare the latest run with ts-suite/oracle.json; exit 1 on any difference).

The pin is ts-suite/pin.json. Nothing is vendored: sources, node_modules, build output and
run logs live in the cache directory (--dir, else $TS_SUITE_DIR, else ~/.cache/jsse-ts-suite),
which must be outside the repository. The full Node pass list stays there too
(runs/<stamp>/node-pass-list.txt); the manifest pins its digests. See ts-suite/README.md.
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
import xml.etree.ElementTree as ET

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SUITE = os.path.join(ROOT, "ts-suite")
PIN = os.path.join(SUITE, "pin.json")
MANIFEST = os.path.join(SUITE, "oracle.json")
STAGES = ("fetch", "install", "build", "run", "record", "check")
RUNNER_PREFIXES = (
    ("conformance tests", "conformance"),
    ("compiler tests", "compiler"),
    ("projects tests", "project"),
    ("fourslash-server tests", "fourslash-server"),
    ("fourslash tests", "fourslash"),
    ("transpile tests", "transpile"),
)
CASE_PATH = re.compile(r"tests/cases/(\S+)")


class Fatal(Exception):
    pass


def sha256_file(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def sha256_lines(ids):
    esc = (
        i.replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r") for i in ids
    )
    return hashlib.sha256(("\n".join(sorted(esc)) + "\n").encode("utf-8")).hexdigest()


def load_pin(path=PIN):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def cache_dir(arg):
    d = os.path.abspath(
        os.path.expanduser(
            arg or os.environ.get("TS_SUITE_DIR") or "~/.cache/jsse-ts-suite"
        )
    )
    if os.path.commonpath([d, ROOT]) == ROOT:
        raise Fatal(
            f"cache directory {d} is inside the repository; TypeScript must not be vendored"
        )
    return d


def sh(cmd, cwd, log=None, env=None):
    print("+", " ".join(cmd), flush=True)
    out = open(log, "ab") if log else None
    try:
        r = subprocess.run(
            cmd, cwd=cwd, env=env, stdout=out, stderr=subprocess.STDOUT if out else None
        )
    finally:
        if out:
            out.close()
    return r.returncode


def must(cmd, cwd, **kw):
    if sh(cmd, cwd, **kw) != 0:
        raise Fatal(f"command failed: {' '.join(cmd)}")


def capture(cmd, cwd=None):
    return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True).stdout.strip()


def fetch(src, pin):
    if not os.path.isdir(os.path.join(src, ".git")):
        os.makedirs(os.path.dirname(src), exist_ok=True)
        must(
            [
                "git",
                "-c",
                "advice.detachedHead=false",
                "clone",
                "--quiet",
                "--depth",
                "1",
                "--branch",
                pin["tag"],
                pin["repo"],
                src,
            ],
            None,
        )
    head = capture(["git", "rev-parse", "HEAD"], src)
    if head != pin["commit"]:
        raise Fatal(f"{src} is at {head}, expected {pin['tag']} = {pin['commit']}")
    if capture(["git", "status", "--porcelain", "--untracked-files=no"], src):
        raise Fatal(f"{src} has modified tracked files")


def patch_mocha_yargs(src):
    """mocha 10's CLI requires yargs/yargs, an extensionless CommonJS file in a "type": "module"
    package; Node >= 22.12 loads it as ESM and fails, so `hereby run-eslint-rules-tests` and
    `hereby runtests` cannot start. Dropping the "type" key (cache copy only) restores CommonJS."""
    p = os.path.join(
        src, "node_modules", "mocha", "node_modules", "yargs", "package.json"
    )
    with open(p, encoding="utf-8") as f:
        data = json.load(f)
    if data.pop("type", None) is not None:
        with open(p, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2)


def install(src, pin):
    got = sha256_file(os.path.join(src, "package-lock.json"))
    if got != pin["lockfile_sha256"]:
        raise Fatal(
            f"package-lock.json sha256 {got} != pinned {pin['lockfile_sha256']}"
        )
    must(["npm", "ci", "--ignore-scripts", "--no-audit", "--no-fund"], src)
    patch_mocha_yargs(src)
    with open(
        os.path.join(src, "node_modules", "mocha", "package.json"), encoding="utf-8"
    ) as f:
        version = json.load(f)["version"]
    if version != pin["mocha"]:
        raise Fatal(f"mocha {version} installed, pinned {pin['mocha']}")


def hereby(src):
    return [os.path.join(src, "node_modules", ".bin", "hereby")]


def build(src, cache):
    log = os.path.join(cache, "build.log")
    for task in ("local", "tests"):
        must(hereby(src) + [task, "--no-colors"], src, log=log)


def run(src, cache, workers, timeout, offline):
    stamp = time.strftime("%Y%m%d-%H%M%S")
    rundir = os.path.join(cache, "runs", stamp)
    os.makedirs(rundir)
    env = dict(os.environ, CI="true")
    prefix = ["unshare", "-rn"] if offline else []
    results = os.path.join(src, "TEST-results.xml")
    jobs = (
        (
            "unit",
            ["runtests-parallel", "--light=false", "--no-lint", f"--workers={workers}"]
            + ([f"--timeout={timeout}"] if timeout else []),
        ),
        ("eslint-rules", ["run-eslint-rules-tests", "--reporter=xunit"]),
    )
    for name, args in jobs:
        if os.path.exists(results):
            os.remove(results)
        code = sh(
            prefix + hereby(src) + args + ["--no-colors"],
            src,
            log=os.path.join(rundir, f"{name}.log"),
            env=env,
        )
        with open(os.path.join(rundir, f"{name}.exit-code"), "w") as f:
            f.write(f"{code}\n")
        if os.path.exists(results):
            shutil.move(results, os.path.join(rundir, f"{name}.xml"))
    link = os.path.join(cache, "runs", "latest")
    if os.path.lexists(link):
        os.remove(link)
    os.symlink(stamp, link)
    print(f"run directory: {rundir}")
    return rundir


def classify(classname, name):
    text = " ".join((classname or name).split())
    for prefix, runner in RUNNER_PREFIXES:
        if text.startswith(prefix):
            return runner, None
    m = re.match(r"unittests\s*::\s*([^\s:]+)", text)
    return "unittests", m.group(1) if m else "other"


def bucket(runner, classname):
    m = CASE_PATH.search(classname or "")
    if not m:
        return None
    path = m.group(1)
    if runner == "conformance":
        parts = path.split("/")
        return parts[1] if len(parts) > 2 else None
    if runner in ("compiler", "fourslash", "fourslash-server"):
        return path.rsplit("/", 1)[-1][:1].upper()
    return None


def read_cases(xml_path):
    """Yield (runner, suite, bucket, path, id, passed) for every testcase."""
    for _, el in ET.iterparse(xml_path):
        if el.tag != "testcase":
            continue
        cn, name = el.get("classname") or "", el.get("name") or ""
        runner, suite = classify(cn, name)
        passed = el.find("failure") is None and el.find("error") is None
        path = CASE_PATH.search(cn)
        yield (
            runner,
            suite,
            bucket(runner, cn),
            path.group(1) if path else None,
            " ".join(cn.split()) + " :: " + name,
            passed,
        )
        el.clear()


def summarise(xml_path):
    """Per-runner counts and digests from one mocha xunit file."""
    runners, ids = {}, {}
    for runner, suite, bkt, path, ident, passed in read_cases(xml_path):
        r = runners.setdefault(
            runner,
            {"passed": 0, "failed": 0, "suites": {}, "buckets": {}, "files": set()},
        )
        r["passed" if passed else "failed"] += 1
        for table, key in (("suites", suite), ("buckets", bkt)):
            if key is not None:
                t = r[table].setdefault(key, {"passed": 0, "failed": 0})
                t["passed" if passed else "failed"] += 1
        if path and runner != "project":
            r["files"].add(path)
        ids.setdefault(runner, ([], []))[0 if passed else 1].append(ident)
    out = {}
    for runner, r in sorted(runners.items()):
        good, bad = ids[runner]
        entry = {
            "passed": r["passed"],
            "failed": r["failed"],
            "digest": sha256_lines(good),
        }
        if r["files"]:
            entry["files"] = len(r["files"])
        for table in ("suites", "buckets"):
            if r[table]:
                entry[table] = dict(sorted(r[table].items()))
        if bad:
            entry["failed_ids"] = sorted(bad)
        out[runner] = entry
    return out, ids


def discovered(log_path):
    with open(log_path, encoding="utf-8", errors="replace") as f:
        text = re.sub(r"\x1b\[[0-9;]*m", "", f.read())
    suites = re.search(r"Discovered (\d+) unittest suites", text)
    files = re.search(r"Discovered (\d+) test files", text)
    return int(suites.group(1)) if suites else None, int(
        files.group(1)
    ) if files else None


def observe(rundir):
    for name in ("unit.xml", "unit.log"):
        if not os.path.exists(os.path.join(rundir, name)):
            raise Fatal(
                f"{rundir} has no {name}; the run did not get as far as the test runner"
            )
    runners, ids = summarise(os.path.join(rundir, "unit.xml"))
    good = [i for g, _ in ids.values() for i in g]
    suites, files = discovered(os.path.join(rundir, "unit.log"))
    obs = {
        "unittest_suites_discovered": suites,
        "test_files_discovered": files,
        "totals": {
            "passed": len(good),
            "failed": sum(len(b) for _, b in ids.values()),
            "digest": sha256_lines(good),
        },
        "runners": runners,
    }
    eslint = os.path.join(rundir, "eslint-rules.xml")
    if os.path.exists(eslint):
        er, eids = summarise(eslint)
        entry = er["unittests"]
        entry.pop("suites", None)
        entry.pop("buckets", None)
        obs["eslint_rules"] = entry
    with open(os.path.join(rundir, "node-pass-list.txt"), "w", encoding="utf-8") as f:
        for ident in sorted(good):
            f.write(
                ident.replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r")
                + "\n"
            )
    return obs


def environment():
    return {
        "node": capture(["node", "--version"]),
        "npm": capture(["npm", "--version"]),
        "platform": f"{sys.platform}-{os.uname().machine}",
    }


def build_manifest(obs, pin):
    return {
        "schema": 1,
        "typescript": {k: pin[k] for k in ("repo", "tag", "commit", "lockfile_sha256")},
        "mocha": pin["mocha"],
        "oracle_engine": environment(),
        "command": "hereby runtests-parallel --light=false --no-lint; hereby run-eslint-rules-tests",
        "pass_list": "not committed; node-pass-list.txt in the cache run directory, one escaped id per line, sorted; sha256 is totals.digest",
        **obs,
    }


def diff(expected, got, path=""):
    """Differences between two manifest subtrees, as strings."""
    if isinstance(expected, dict) and isinstance(got, dict):
        out = []
        for k in sorted(set(expected) | set(got)):
            if k not in got:
                out.append(f"{path}/{k}: missing from run (expected {expected[k]!r})")
            elif k not in expected:
                out.append(f"{path}/{k}: not in manifest ({got[k]!r})")
            else:
                out += diff(expected[k], got[k], f"{path}/{k}")
        return out
    return [] if expected == got else [f"{path}: manifest {expected!r}, run {got!r}"]


def check(manifest, obs):
    keys = (
        "unittest_suites_discovered",
        "test_files_discovered",
        "totals",
        "runners",
        "eslint_rules",
    )
    return diff({k: manifest.get(k) for k in keys}, {k: obs.get(k) for k in keys})


def main(argv=None):
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("stages", nargs="*", metavar="stage", help=" ".join(STAGES))
    ap.add_argument(
        "--dir",
        help="cache directory (default $TS_SUITE_DIR, else ~/.cache/jsse-ts-suite)",
    )
    ap.add_argument("--workers", type=int, default=min(8, os.cpu_count() or 1))
    ap.add_argument(
        "--timeout", type=int, help="per-test timeout in ms (upstream default 40000)"
    )
    ap.add_argument(
        "--offline",
        action="store_true",
        help="run the suites in an empty network namespace (unshare -rn)",
    )
    ap.add_argument(
        "--run-dir",
        help="use this run directory for record/check instead of runs/latest",
    )
    args = ap.parse_args(argv)
    stages = args.stages or ["fetch", "install", "build", "run", "check"]
    if bad := [s for s in stages if s not in STAGES]:
        ap.error(f"unknown stage {bad[0]!r} (choose from {', '.join(STAGES)})")
    try:
        pin, cache = load_pin(), cache_dir(args.dir)
        src = os.path.join(cache, "ts-src")
        rundir = args.run_dir
        if "fetch" in stages:
            fetch(src, pin)
        if "install" in stages:
            install(src, pin)
        if "build" in stages:
            build(src, cache)
        if "run" in stages:
            rundir = run(src, cache, args.workers, args.timeout, args.offline)
        if "record" in stages or "check" in stages:
            rundir = rundir or os.path.join(cache, "runs", "latest")
            obs = observe(rundir)
            if "record" in stages:
                with open(MANIFEST, "w", encoding="utf-8") as f:
                    json.dump(build_manifest(obs, pin), f, indent=1, sort_keys=False)
                    f.write("\n")
                print(f"wrote {MANIFEST}")
            if "check" in stages:
                with open(MANIFEST, encoding="utf-8") as f:
                    problems = check(json.load(f), obs)
                for p in problems:
                    print("DIFF", p)
                print(
                    "oracle",
                    "MATCHES" if not problems else f"DIFFERS ({len(problems)})",
                    "- totals:",
                    obs["totals"]["passed"],
                    "passed,",
                    obs["totals"]["failed"],
                    "failed",
                )
                return 1 if problems else 0
    except Fatal as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
