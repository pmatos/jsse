"""Cases for the TypeScript runner's parallel host, `hereby runtests-parallel` (issue #1258).

Used by run-ts-mocha-oracle.py. `runConsoleTests(runJs, reporter, /*runInParallel*/ true)` (fixture runtests.mjs
with TS_PARALLEL=1) writes test.config with `workerCount` and `taskConfigsFolder` and runs `process.execPath
built/local/run.js`; run.js then becomes the host (src/testRunner/parallel/host.ts), which forks
`process.argv[1]` once per worker with an IPC channel, hands out tasks by message and replays the collected
results into mocha's Base reporter. The cases diff the whole run between Node and `jsse --node`;
`serial_problems` checks that a parallel run reports what the serial run of the same sample reports; the
`par-crash-*` cases run the host over a wrapper (par-crash.js) that makes worker 0 fail.

A sample needs at least as many files as workers (the host sends `undefined` to a worker it has no task for and
then waits forever, on Node too), and the serial run counts one unit test the runner leaks into every list.
"""

import re

from ts_runner_cli_cases import runtests

SAMPLE = {
    "compiler": ["--runner=compiler", "--shards=1500", "--shardId=40"],
    "conformance": ["--runner=conformance", "--shards=900", "--shardId=11"],
    "project": ["--runner=project", "--shards=8", "--shardId=3"],
    "fourslash": ["--runner=fourslash", "--shards=900", "--shardId=7"],
}
PASSING = ("stdout", r"\d+ passing")
PAR = {"TS_PARALLEL": "1"}
# each worker parses lib.d.ts for its first test: tens of seconds on jsse
SLOW = ["--timeout=600000"]
COUNT = re.compile(r"^\s*(\d+) (passing|failing)", re.M)


def par(name, flags, run_js=None, env=None, must=(PASSING,)):
    return runtests(name, flags, run_js, {**PAR, **(env or {})}, False, None, must)


def parallel_cases():
    cases = []
    for r, sel in SAMPLE.items():
        if r != "project":
            serial = sel + ["--reporter=dot", "--no-colors"] + SLOW
            cases.append(runtests(f"par-serial-{r}", serial, must=[PASSING]))
        cases.append(par(f"par-run-{r}", sel + ["--workers=3", "--no-colors"] + SLOW))
    cases.append(
        par("par-workers-8", SAMPLE["compiler"] + ["-w", "8", "--no-colors"] + SLOW)
    )
    return cases + crashes()


def crashes():
    """A worker that exits or is killed ends the run with exit status 2 and a report; an unhandled rejection too
    (the worker reports it over IPC); an uncaught exception is one failed test, so the status is 1."""
    sel = SAMPLE["compiler"] + ["--workers=3", "--no-colors"] + SLOW
    gone = ("stderr", r"Test worker process exited with nonzero exit code")
    cases = []
    for mode, expect, status in (
        ("exit", gone, 2),
        ("kill", gone, 2),
        ("throw", ("stdout", r"1 failing[\s\S]*worker boom"), 1),
        ("reject", ("stderr", r"unexpected error[\s\S]*worker reject"), 2),
    ):
        cases.append(
            par(
                f"par-crash-{mode}",
                sel,
                "par-crash.js",
                {"CRASH_MODE": mode},
                must=[expect, ("exit", f"^{status}$")],
            )
        )
    return cases


def counts(stdout):
    return {kind: int(number) for number, kind in COUNT.findall(stdout)}


def serial_problems(results):
    """A parallel run passes and fails what the serial run of its sample does: the serial count is the
    parallel one plus the same leaked unit test on both engines, and neither fails anything."""
    by_name = {c.name: (ref, got) for c, ref, got, _ in results}
    problems = []
    for r in SAMPLE:
        if r == "project":
            continue
        s, p = by_name.get(f"par-serial-{r}"), by_name.get(f"par-run-{r}")
        if not s or not p:
            continue
        deltas = []
        for idx, engine in ((0, "node"), (1, "jsse")):
            serial, par_run = counts(s[idx]["stdout"]), counts(p[idx]["stdout"])
            if (
                serial.get("failing")
                or par_run.get("failing")
                or "passing" not in par_run
            ):
                problems.append(
                    f"{engine}: {r} has failures or no result: {serial} {par_run}"
                )
            deltas.append(serial.get("passing", 0) - par_run.get("passing", 0))
        if deltas != [1, 1]:
            problems.append(
                f"{r}: serial minus parallel passing is {deltas} (node, jsse)"
            )
    return problems
