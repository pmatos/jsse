#!/usr/bin/env python3
"""Drive `tsc --watch` through scripted edit sessions on Node and on jsse --node and diff them (issue #1248).

    TSC_DIR=/path/to/typescript uv run python scripts/run-tsc-watch-oracle.py [options] [case ...]

Each case in scripts/tsc-fixtures/watch-cases.txt is `name | project | tsc arguments | steps`
(` ;; ` separates steps; leading NAME=value tokens set the environment). The harness starts
`lib/_tsc.js <arguments>` in a fresh copy of scripts/tsc-fixtures/projects/<project>, waits for the
first "Watching for file changes." line, then runs the steps:

    @wait                   wait for a compile cycle, then for quiet output (tsc may run a second one), and record the
                            new stdout, stderr and every file created, changed or deleted since the last record
    @idle SECONDS           sleep, then record (nothing is expected to happen)
    @exits                  (first step) tsc is expected to stop by itself, e.g. when its configuration file goes away
    @replace PATH OLD NEW   @write PATH TEXT (\\n, \\r and \\uXXXX escapes)   @append PATH TEXT
    @remove PATH..          @mkdir PATH..   @rename OLD NEW   @touch PATH..

Edits between two @wait steps land together, so a burst is a debounce test. The session ends with SIGINT; the exit
status joins the record. Stdout, stderr, emitted files and the status must be identical on both
engines after the normalisation of the project oracle (work-tree path, TypeScript path, clock stamps).
Options: --jobs N (default 4), --timeout SECONDS per wait (default 300), --settle SECONDS of quiet output after a cycle (default 1.0), --ci,
--list, --require, --pin / --no-node (digests in scripts/tsc-fixtures/watch-expected.txt), --fixtures DIR.
"""

import argparse
import contextlib
import difflib
import hashlib
import importlib.util
import os
import re
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import oracle_procs  # noqa: E402

spec = importlib.util.spec_from_file_location(
    "tsc_projects", os.path.join(HERE, "run-tsc-projects-oracle.py")
)
proj = importlib.util.module_from_spec(spec)
spec.loader.exec_module(proj)

FIX = proj.FIX
READY = re.compile(rb"Watching for file changes\.")
CLOCK = re.compile(r"(?<!\d)\d{1,2}:\d{2}:\d{2}\s?[AP]M\b")
START = re.compile(
    rb"(?:Starting compilation in watch mode|File change detected\. Starting incremental compilation)\.\.\."
)
ELAPSED = re.compile(r"Elapsed:: [0-9.e+-]+ms")
EDITS = ("@replace", "@write", "@append", "@remove", "@mkdir", "@rename", "@touch")
STEPS = EDITS + ("@wait", "@idle")
LEAKING_ENV = ("TSC_WATCHFILE", "TSC_WATCHDIRECTORY", "TSC_NONPOLLING_WATCHER")
LOCALE = {
    "LC_ALL": "en_US.UTF-8",
    "LANG": "en_US.UTF-8",
    "LANGUAGE": "en_US",
    "TZ": "UTC",
}


class Log:
    """Incremental view of the growing stdout file: counts of started and finished cycles."""

    def __init__(self, path):
        self.path, self.pos, self.rest = path, 0, b""
        self.ready = self.started = 0

    def poll(self):
        """Read what was appended since the last call; returns the number of new bytes."""
        with open(self.path, "rb") as f:
            f.seek(self.pos)
            new = f.read()
        self.pos += len(new)
        data = self.rest + new
        cut = data.rfind(b"\n") + 1
        self.rest, whole = data[cut:], data[:cut]
        self.ready += len(READY.findall(whole))
        self.started += len(START.findall(whole))
        return len(new)


def edit(tree, step):
    op, args = step[0], step[1:]

    def path(p):
        return os.path.join(tree, p)

    if op in ("@replace", "@remove"):
        proj.scenario_step(tree, step)
    elif op in ("@write", "@append"):
        with open(
            path(args[0]), "w" if op == "@write" else "a", encoding="utf-8", newline=""
        ) as f:
            f.write(proj.decode_escapes(args[1]))
    elif op == "@mkdir":
        for p in args:
            os.makedirs(path(p))
    elif op == "@rename":
        os.rename(path(args[0]), path(args[1]))
    else:
        for p in args:
            os.utime(path(p), None)


def load_cases(fix):
    cases = []
    with open(os.path.join(fix, "watch-cases.txt"), encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line and not line.startswith("#"):
                name, project, args, steps = (p.strip() for p in line.split("|", 3))
                cases.append(
                    (
                        name,
                        project,
                        shlex.split(args),
                        [shlex.split(s) for s in steps.split(" ;; ")],
                    )
                )
    return cases


def read(path):
    with open(path, "rb") as f:
        return f.read()


def run_case(engine, exe, tsc_dir, work, case, timeout, settle, fix):
    name, project, args, steps = case
    expect_exit = steps[0] == ["@exits"]
    steps = steps[1:] if expect_exit else steps
    tree = os.path.join(work, engine, name)
    shutil.copytree(os.path.join(fix, "projects", project), tree)
    proj.expand_escaped(tree)
    logs = os.path.join(work, engine + "-logs")
    os.makedirs(logs, exist_ok=True)
    out_path, err_path = (os.path.join(logs, f"{name}.{s}") for s in ("out", "err"))
    extra, _, _, args = proj.split_invocation(list(args))
    env = {
        k: v
        for k, v in os.environ.items()
        if k not in proj.DROP_ENV and k not in LEAKING_ENV
    }
    env.update(LOCALE)
    env.update(extra)
    cmd = [exe] + (["--node"] if engine == "jsse" else [])
    cmd += [os.path.join(tsc_dir, "lib", "_tsc.js")] + args
    for step in steps:
        if step[0] not in STEPS:
            raise SystemExit(f"{name}: unknown watch step {step[0]}")

    moving = any(a in ("--diagnostics", "--extendedDiagnostics") for a in args)

    def norm(b):
        t = b.decode("utf-8", "surrogateescape")
        t = t.replace(tree, "<DIR>").replace(tsc_dir, "<TSC>")
        t = re.sub(re.escape(os.path.join(work, engine)) + r"(?!\w)", "<WORK>", t)
        t = CLOCK.sub("<TIME>", t.replace(work, "<ROOT>"))
        t = ELAPSED.sub("Elapsed:: <MS>", t)
        return proj.normalise_stats(t) if moving else t

    log = Log(out_path)
    record, seen = [], {"out": 0, "err": 0, "ready": 0, "files": proj.snapshot(tree)}
    pending = []

    def snapshot_record(header):
        out, err = read(out_path), read(err_path)
        files = proj.snapshot(tree)
        old = seen["files"]
        changed = {p: c for p, c in files.items() if old.get(p) != c}
        text = f"=== {header}\n"
        text += "--- stdout\n" + norm(out[seen["out"] :])
        text += "--- stderr\n" + norm(err[seen["err"] :])
        for p in sorted(changed):
            text += f"--- file {p}\n" + proj.normalise_file(p, norm(changed[p]))
        for p in sorted(set(old) - set(files)):
            text += f"--- file {p}\n<deleted>\n"
        record.append(text)
        seen.update(
            out=len(out), err=len(err), files=files, ready=len(READY.findall(out))
        )

    stack = contextlib.ExitStack()
    with open(out_path, "wb") as fo, open(err_path, "wb") as fe:
        p = stack.enter_context(
            oracle_procs.group(cmd, cwd=tree, env=env, stdout=fo, stderr=fe)
        )

    def wait_cycles(base):
        """Block until a cycle beyond the `base` finished, no cycle is running and the output was quiet for
        `settle` seconds (a change can start a second cycle after the first)."""
        deadline = time.monotonic() + timeout
        moved = time.monotonic()
        while True:
            now = time.monotonic()
            if log.poll():
                moved = now
            if log.ready > base and log.started <= log.ready and now - moved >= settle:
                return None
            if p.poll() is not None:
                return f"exited {p.returncode} early"
            if now > deadline:
                return "timeout"
            time.sleep(0.05)

    start, problem = time.monotonic(), None
    try:
        problem = wait_cycles(0)
        if problem is None:
            snapshot_record("initial compile")
            for step in steps:
                if step[0] in EDITS:
                    edit(tree, step)
                    pending.append(" ".join(step))
                elif step[0] == "@wait":
                    problem = wait_cycles(seen["ready"])
                    if problem:
                        break
                    snapshot_record(f"after {'; '.join(pending) or 'nothing'}")
                    pending.clear()
                else:
                    time.sleep(float(step[1]))
                    snapshot_record(f"idle after {'; '.join(pending) or 'nothing'}")
                    pending.clear()
    finally:
        try:
            if p.poll() is None:
                os.killpg(p.pid, signal.SIGINT)
                try:
                    p.wait(timeout=30)
                except subprocess.TimeoutExpired:
                    os.killpg(p.pid, signal.SIGKILL)
                    p.wait()
                    problem = problem or "ignored SIGINT"
        finally:
            stack.close()
    status = problem or str(p.returncode)
    elapsed = time.monotonic() - start
    snapshot_record(f"end (exit {status})")
    if expect_exit:
        stopped = (problem or "").startswith("exited")
        problem = None if stopped else problem or "did not stop by itself"
    return {
        "exit": status,
        "stdout": "".join(record),
        "secs": elapsed,
        "problem": problem,
    }


def digest(res):
    return hashlib.sha256(
        (res["exit"] + "\0" + res["stdout"]).encode("utf-8", "surrogateescape")
    ).hexdigest()


def pins(path):
    try:
        with open(path, encoding="utf-8") as f:
            return dict(row.split() for row in f if row.strip())
    except FileNotFoundError:
        return {}


def main():
    oracle_procs.install_cleanup()
    ap = argparse.ArgumentParser()
    ap.add_argument("cases", nargs="*")
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--timeout", type=int, default=300)
    ap.add_argument("--settle", type=float, default=1.0)
    for flag in ("--ci", "--list", "--require", "--pin", "--no-node"):
        ap.add_argument(flag, action="store_true")
    ap.add_argument("--fixtures", default=FIX)
    ap.add_argument(
        "--dump", help="write each engine's record of every session to this directory"
    )
    o = ap.parse_args()
    fix = os.path.abspath(o.fixtures)
    cases = load_cases(fix)
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
            "TSC_DIR is not set; point it at an installed typescript 6.0.3 package to run the tsc watch oracle"
        )
    tsc_dir = os.path.realpath(os.environ["TSC_DIR"])
    manifest = os.path.join(tsc_dir, "package.json")
    if not os.path.isfile(
        os.path.join(tsc_dir, "lib", "_tsc.js")
    ) or not os.path.isfile(manifest):
        return skip(f"{tsc_dir} is not a typescript package (no lib/_tsc.js)")
    version = re.search(r'"version"\s*:\s*"([^"]+)"', read(manifest).decode())
    if not version or version.group(1) != "6.0.3":
        return skip(
            f"the tsc watch oracle targets typescript 6.0.3, found {version and version.group(1)}"
        )
    node = shutil.which("node")
    if not node and not o.no_node:
        return skip(
            "node not found; the oracle needs it (or --no-node with the pinned digests)"
        )
    if o.pin and (o.no_node or not node):
        sys.exit("--pin needs Node: it pins Node's results")
    jsse = os.path.abspath(
        os.environ.get("JSSE", os.path.join(proj.ROOT, "target", "release", "jsse"))
    )
    if not os.access(jsse, os.X_OK):
        print("Building jsse (release)...", flush=True)
        subprocess.run(["cargo", "build", "--release"], cwd=proj.ROOT, check=True)
    pinfile = os.path.join(fix, "watch-expected.txt")
    pinned = pins(pinfile) if o.no_node else {}
    print(
        f"typescript 6.0.3 from {tsc_dir} (jsse: {jsse}, {len(cases)} watch sessions, {o.jobs} jobs)",
        flush=True,
    )
    base = (
        "/tmp"
        if os.path.realpath("/tmp") == "/tmp" and os.access("/tmp", os.W_OK)
        else None
    )
    work = os.path.realpath(tempfile.mkdtemp(prefix="tsc-oracle-", dir=base))

    def one(case):
        results = {}
        jobs = [("jsse", jsse)] + ([] if o.no_node else [("node", node)])
        for engine, exe in jobs:
            results[engine] = run_case(
                engine, exe, tsc_dir, work, case, o.timeout, o.settle, fix
            )
            if o.dump:
                os.makedirs(o.dump, exist_ok=True)
                with open(
                    os.path.join(o.dump, f"{case[0]}.{engine}.txt"),
                    "w",
                    encoding="utf-8",
                    errors="surrogateescape",
                ) as f:
                    f.write(results[engine]["stdout"])
        return case[0], results

    failed, passed, fresh = [], 0, {}
    with ThreadPoolExecutor(max_workers=o.jobs) as pool:
        for name, res in pool.map(one, cases):
            j = res["jsse"]
            secs = f"jsse {j['secs']:.1f}s" + (
                f", node {res['node']['secs']:.1f}s" if "node" in res else ""
            )
            if "node" in res:
                n = res["node"]
                if not n["problem"]:
                    fresh[name] = digest(n)
                same = n["exit"] == j["exit"] and n["stdout"] == j["stdout"]
                ok = same and not n["problem"]
            else:
                ok = pinned.get(name) == digest(j) and not j["problem"]
            print(f"{'PASS' if ok else 'FAIL'} {name} ({secs})", flush=True)
            if ok:
                passed += 1
                continue
            failed.append(name)
            if "node" in res:
                d = list(
                    difflib.unified_diff(
                        n["stdout"].splitlines(),
                        j["stdout"].splitlines(),
                        "node",
                        "jsse",
                        lineterm="",
                        n=2,
                    )
                )
                limit = 400 if o.ci else 60
                print("\n".join("    " + r for r in d[:limit]))
                if n["exit"] != j["exit"]:
                    print(f"    exit: node {n['exit']} jsse {j['exit']}")
            else:
                print(
                    f"    digest {digest(j)} is not the pinned {pinned.get(name)}\n{j['stdout']}"
                )
    shutil.rmtree(work, ignore_errors=True)
    if o.pin and failed:
        print("not pinning: a selected session failed or Node did not run it cleanly")
    elif o.pin:
        merged = pins(pinfile)
        merged.update(fresh)
        with open(pinfile, "w", encoding="utf-8") as f:
            f.writelines(f"{k} {v}\n" for k, v in sorted(merged.items()))
    print(f"tsc watch oracle: {passed}/{len(cases)} passed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
