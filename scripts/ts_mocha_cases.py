"""The case table of the mocha oracle (scripts/run-ts-mocha-oracle.py): one entry per mocha invocation.

Each case is (name, mocha arguments, spec files, test.config or None, extra environment). A spec is a fixture in
scripts/ts-mocha-fixtures or "RUN" for the TypeScript runner built/local/run.js.
"""

from ts_runner_cli_cases import Case

UNIT_CONFIG = {"test": ["unittests:: semver"], "runners": ["unittest"], "light": False}
FAILED_TESTS = "scripts/failed-tests.cjs"

CASES = []


def case(name, args, specs, config=None, env=None):
    CASES.append(Case(name, args, specs, config, env or {}))


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
    "doc",
    "json-stream",
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

for reporter in ("spec", "dot", "tap", "json", "xunit"):
    case(f"timers-{reporter}", ["-R", reporter], ["timers.spec.js"])
    case(f"timeouts-{reporter}", ["-R", reporter], ["timeouts.spec.js"])
    case(f"diff-{reporter}", ["-R", reporter], ["diff.spec.js"])
case("timeouts-bail", ["-R", "spec", "--bail"], ["timeouts.spec.js"])
case("timeouts-retries", ["-R", "spec", "--retries", "1"], ["timeouts.spec.js"])
for limit, label in (("60", "60ms"), ("0", "off"), ("1s", "1s")):
    case(f"cli-timeout-{label}", ["-R", "spec", "-t", limit], ["cli-timeout.spec.js"])
case("cli-timeout-json", ["-R", "json", "-t", "60"], ["cli-timeout.spec.js"])
case("cli-slow-json", ["-R", "json", "--slow", "1000"], ["cli-timeout.spec.js"])
case("diff-no-diff", ["-R", "spec", "--no-diff"], ["diff.spec.js"])
case("diff-inline", ["-R", "spec", "--inline-diffs"], ["diff.spec.js"])
case("diff-colors", ["-R", "spec", "--colors"], ["diff.spec.js"])
case(
    "diff-colors-inline", ["-R", "spec", "--colors", "--inline-diffs"], ["diff.spec.js"]
)
case("diff-fail-fixture-colors", ["-R", "spec", "--colors"], ["fail.spec.js"])
case("fail-full-trace", ["-R", "spec", "--full-trace"], ["fail.spec.js"])
case(
    "fail-fivemat",
    ["-R", FAILED_TESTS, "-O", "reporter=mocha-fivemat-progress-reporter"],
    ["fail.spec.js"],
)
case("fail-tests-min", ["-R", FAILED_TESTS, "-O", "reporter=min"], ["fail.spec.js"])
for reporter in ("spec", "json", "dot"):
    case(
        f"retries-cli-{reporter}", ["-R", reporter, "--retries", "2"], ["retry.spec.js"]
    )
case("retries-none", ["-R", "spec"], ["retry.spec.js"])
case("only", ["-R", "spec"], ["only.spec.js"])
case("forbid-only", ["-R", "spec", "--forbid-only"], ["only.spec.js"])
case("forbid-pending", ["-R", "spec", "--forbid-pending"], ["pass.spec.js"])
case("check-leaks", ["-R", "spec", "--check-leaks"], ["leak.spec.js"])
case(
    "check-leaks-global",
    ["-R", "spec", "--check-leaks", "--global", "allowedLeak"],
    ["leak.spec.js"],
)
case("dry-run", ["-R", "spec", "--dry-run"], ["pass.spec.js"])
case("async-only", ["-R", "spec", "--async-only"], ["pass.spec.js"])
case("sort", ["-R", "dot", "--sort"], ["pass.spec.js", "fail.spec.js"])
case("delay", ["-R", "spec", "--delay"], ["delay.spec.js"])
case("tdd", ["-R", "spec", "-u", "tdd"], ["tdd.spec.js"])
case("grep-regex-flags", ["-R", "spec", "-g", "/GENERATED/i"], ["pass.spec.js"])
case("grep-alternation", ["-R", "dot", "-g", "sync|promise"], ["pass.spec.js"])
case("grep-no-match", ["-R", "spec", "-g", "nothing matches this"], ["pass.spec.js"])
case("fgrep-regex-chars", ["-R", "spec", "-f", "a.b"], ["pass.spec.js"])
case("fgrep-invert", ["-R", "dot", "-f", "generated", "-i"], ["pass.spec.js"])
case("grep-timeouts", ["-R", "spec", "-g", "hooks"], ["timeouts.spec.js"])
case("xunit-output", ["-R", "xunit", "-O", "output=report.xml"], ["fail.spec.js"])
case("json-output", ["-R", "json", "-O", "output=report.json"], ["fail.spec.js"])
case("list-reporters", ["--list-reporters"], [])
case("list-interfaces", ["--list-interfaces"], [])

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
