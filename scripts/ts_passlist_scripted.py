"""The scripted oracles as pass-list runners of scripts/run-ts-pass-list.py (issue #1296).

`tsc-projects` (scripts/run-tsc-projects-oracle.py: `tsc` compiles, diagnostics, informational flags, emit matrix and
`--build` scenarios), `tsc-watch` (run-tsc-watch-oracle.py) and `hereby` (run-hereby-oracle.py) are not mocha runners.
Each case is a test whose id is `runner :: case`; it passes when the oracle prints `ok`/`PASS` for it, i.e. when
jsse and Node agree on exit status, output and every file written (the oracles' own comparison, run without
`--no-node`: the pinned digests depend on where the typescript package sits relative to the work directory, Node
and jsse do not). The watch sessions are timing sensitive, so they settle for 3 s instead of 1 s (a loaded host made
the default flaky). The tsc ones need TSC_DIR (an installed typescript 6.0.3 package), the hereby one TS_SRC_DIR.
"""

import collections
import json
import os
import re
import shutil

import oracle_procs

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
SCRIPTED = {
    "tsc-projects": ("run-tsc-projects-oracle.py", [], "TSC_DIR"),
    "tsc-watch": ("run-tsc-watch-oracle.py", ["--settle", "3"], "TSC_DIR"),
    "hereby": ("run-hereby-oracle.py", [], None),
}
RESULT = re.compile(r"^(ok  |PASS|FAIL) (\S+)")


class Case:
    """One oracle case, shaped like ts_passlist.Test (id, full, parent, runner, bucket)."""

    __slots__ = ("bucket", "full", "id", "parent", "runner")

    def __init__(self, runner, name):
        self.runner, self.full, self.parent, self.bucket = runner, name, name, "all"
        self.id = f"{runner} :: {name}"

    def __repr__(self):
        return f"Case({self.id!r})"


def unavailable(runner, environ=None):
    """Why `runner` cannot run here, or None."""
    environ = os.environ if environ is None else environ
    env = SCRIPTED[runner][2]
    if env is None:
        return None
    path = environ.get(env)
    if not path:
        return f"{env} is not set (an installed typescript 6.0.3 package)"
    try:
        with open(os.path.join(path, "package.json"), encoding="utf-8") as f:
            version = json.load(f).get("version")
    except (OSError, ValueError):
        version = None
    if version != "6.0.3" or not os.path.isfile(os.path.join(path, "lib", "_tsc.js")):
        return f"{path} is not a typescript 6.0.3 package"
    return None


def command(runner, python, extra):
    return [python, os.path.join(SCRIPTS, SCRIPTED[runner][0]), *extra]


def list_cases(runner, python, env):
    code, out, err = oracle_procs.run_group(
        command(runner, python, ["--list"]), SCRIPTS, env, 300
    )
    if code != "0":
        raise RuntimeError(
            f"{runner}: --list exited {code}: {err.decode('utf-8', 'replace')[-500:]}"
        )
    return [Case(runner, n) for n in out.decode().split() if n]


def parse_results(text):
    """{case: passed} from an oracle's output."""
    out = {}
    for line in text.splitlines():
        m = RESULT.match(line)
        if m:
            out[m.group(2)] = m.group(1) != "FAIL"
    return out


def run_cases(runner, cases, python, jsse, jobs, timeout, env):
    """(Counter {(case, kind): n}, error or None) of one oracle run over `cases`."""
    extra = [*SCRIPTED[runner][1], "--jobs", str(jobs)]
    if runner == "hereby":
        extra += ["--jsse", jsse]
    names = [c.full for c in cases]
    code, out, err = oracle_procs.run_group(
        command(runner, python, [*extra, *names]),
        SCRIPTS,
        {**env, "JSSE": jsse},
        timeout,
    )
    text = out.decode("utf-8", "replace")
    results = parse_results(text)
    outcomes = collections.Counter(
        {(n, "passing" if ok else "failing"): 1 for n, ok in results.items()}
    )
    if not results:
        tail = (text + err.decode("utf-8", "replace"))[-1500:]
        return outcomes, f"{runner}: exit {code}, no results; output tail:\n{tail}"
    missing = [n for n in names if n not in results]
    if missing:
        return (
            outcomes,
            f"{runner}: exit {code}, {len(missing)} of {len(names)} cases printed no result "
            f"(timeout or killed?), first: {missing[0]}",
        )
    return outcomes, None


def python_exe():
    return shutil.which("python3") or "python3"
