"""Cases for the TypeScript runner's command line, sharding and failed-tests flow (issue #1257).

Used by run-ts-mocha-oracle.py. `hereby runtests` turns its flags (--runner, --tests, --shards, --shardId,
--light, --failed, --keepFailed, --colors, --timeout, ...) into test.config and a mocha command line
(scripts/build/tests.mjs). The `runtests` cases run that same function (fixture runtests.mjs) in a work
directory, with `built/local/run.js` or, where a case needs failures it can control, a fixture spec as the
runner bundle (TS_RUN_JS), then diff the whole run between Node and jsse --node:

  list cases   mocha's --dry-run (a .mocharc.json) prints every test title the selected runners register,
               so the test list for a runner, a --tests filter or a shard is compared line for line;
  run cases    a handful of real tests per runner (shards pick them), with and without --light;
  failed flow  a second `runtests` step in the same directory re-consumes .failed-tests (--failed,
               --keepFailed) after the fixture is fixed;
  accept       `hereby baseline-accept` on seeded local and reference baselines in a private tree.
"""

import hashlib
import os
import re
from typing import NamedTuple


class Case(NamedTuple):
    name: str
    args: list
    specs: list
    config: object = None
    env: dict = {}
    kind: str = "mocha"  # "mocha" | "runtests" | "accept"
    mocharc: object = None
    then: object = None  # (flags, env) of a second runtests step in the same directory
    must: tuple = ()  # (result key, regex) that Node's own result has to match


DRY_RUN = {"dry-run": True}
TAP = ["--reporter=tap", "--no-colors"]
REAL = ["--reporter=spec", "--no-colors", "--timeout=600000"]
RUNNERS = (
    "conformance",
    "compiler",
    "project",
    "fourslash",
    "fourslash-server",
    "fourslash-generated",
    "transpile",
    "unittest",
)
OK_LINES = ("stdout", r"^ok 2 ")

# group of shard cases -> the case listing the whole runner; their titles must add up to it
PARTITIONS = {
    "cli-list-conformance": [f"cli-shards-conformance-{i}of3" for i in (1, 2, 3)],
    "cli-list-project": [f"cli-shards-project-{i}of3" for i in (1, 2, 3)],
    "cli-list-fourslash": [f"cli-shards-fourslash-{i}of3" for i in (1, 2, 3)],
}

ACCEPT_LOCAL = {
    "added.js": "new file\n",
    "sub/dir/added.errors.txt": "nested\n",
    "changed.types": "new content\n",
    "gone.symbols.delete": "",
    "deep/er/still/deeper.js": "deep\n",
}
ACCEPT_REFERENCE = {
    "changed.types": "old content\n",
    "gone.symbols": "to be deleted\n",
    "kept.js": "untouched\n",
    "sub/dir/old.txt": "also untouched\n",
}


def runtests(name, flags, run_js=None, env=None, dry=False, then=None, must=()):
    env = dict(env or {})
    if run_js:
        env["TS_RUN_JS"] = run_js
    specs = ["runtests.mjs"] + ([run_js] if run_js else [])
    return Case(
        name,
        list(flags),
        specs,
        None,
        env,
        "runtests",
        DRY_RUN if dry else None,
        then,
        tuple(must),
    )


def lists():
    """Test titles per runner, per --tests filter, per alias and per shard."""
    cases = []
    for r in RUNNERS:
        ok = ("stdout", r"^ok 1 " if r == "fourslash-generated" else r"^ok 2 ")
        cases.append(
            runtests(f"cli-list-{r}", [f"--runner={r}"] + TAP, dry=True, must=[ok])
        )
    cases += [
        runtests(
            "cli-list-multi",
            ["--runner=project,transpile,fourslash-server"] + TAP,
            dry=True,
            must=[OK_LINES],
        ),
        runtests(
            "cli-list-alias",
            ["--ru=transpile", "-t", "declarationBasicSyntax", "-r", "tap", "-c"],
            dry=True,
            must=[("stdout", r"declarationBasicSyntax")],
        ),
        runtests(
            "cli-tests-project",
            ["--runner=project", "--tests=Correct errors for tests/cases/project/b"]
            + TAP,
            dry=True,
            must=[("stdout", r"Correct errors")],
        ),
        runtests(
            "cli-tests-alternation",
            ["--runner=fourslash", "--tests=addAllMissing|autoImportProvider1"] + TAP,
            dry=True,
            must=[("stdout", r"autoImportProvider1")],
        ),
        runtests(
            "cli-tests-none",
            ["--runner=fourslash-server", "--tests=no such test anywhere"] + TAP,
            dry=True,
        ),
        runtests(
            "cli-tests-spec-reporter",
            ["--runner=transpile", "--tests=declarationBasicSyntax", "--colors"]
            + ["--reporter=spec"],
            dry=True,
            must=[("stdout", r"declarationBasicSyntax")],
        ),
    ]
    for r, n in (("conformance", 3), ("project", 3), ("fourslash", 3)):
        for i in range(1, n + 1):
            cases.append(
                runtests(
                    f"cli-shards-{r}-{i}of{n}",
                    [f"--runner={r}", f"--shards={n}", f"--shardId={i}"] + TAP,
                    dry=True,
                    must=[OK_LINES],
                )
            )
    cases += [
        runtests(
            "cli-shards-compiler-3of7",
            ["--runner=compiler", "--shards=7", "--shardId=3"] + TAP,
            dry=True,
            must=[OK_LINES],
        ),
        runtests(
            "cli-shards-id-only",
            ["--runner=transpile", "--shardId=2"] + TAP,
            dry=True,
        ),
        runtests(
            "cli-shards-count-only",
            ["--runner=fourslash-server", "--shards=2"] + TAP,
            dry=True,
            must=[OK_LINES],
        ),
        runtests(
            "cli-shards-past-the-end",
            ["--runner=transpile", "--shards=2", "--shardId=3"] + TAP,
            dry=True,
        ),
        runtests(
            "cli-shards-and-tests",
            ["--runner=fourslash", "--shards=4", "--shardId=2"]
            + ["--tests=addAllMissing|autoImport"]
            + TAP,
            dry=True,
        ),
    ]
    return cases


def real_runs():
    """A few real tests per runner, picked by shards, so the flags drive actual execution."""
    sel = {
        "compiler": ("--shards=6000", "--shardId=100"),
        "conformance": ("--shards=3000", "--shardId=9"),
        "project": ("--shards=1000", "--shardId=3"),
        "fourslash": ("--shards=2000", "--shardId=5"),
        "fourslash-server": ("--shards=300", "--shardId=2"),
        "transpile": (),
    }
    cases = []
    for r, flags in sel.items():
        cases.append(
            runtests(
                f"cli-run-{r}",
                [f"--runner={r}", *flags] + REAL,
                must=[("stdout", r"passing")],
            )
        )
    cases += [
        runtests(
            "cli-run-compiler-no-light",
            ["--runner=compiler", "--shards=6000", "--shardId=100", "--no-light"]
            + REAL,
            must=[("stdout", r"passing")],
        ),
        runtests(
            "cli-run-compiler-light-flag",
            ["--runner=compiler", "--shards=6000", "--shardId=100", "--light"] + REAL,
            must=[("stdout", r"passing")],
        ),
        runtests(
            "cli-run-unit",
            ["--runner=unittest", "-t", "unittests:: semver", "--reporter=dot"]
            + ["--no-colors", "--timeout=600000"],
            must=[("stdout", r"692 passing")],
        ),
        runtests(
            "cli-run-unit-skip-sys",
            ["--runner=unittest", "--tests=unittests:: semver", "--skipSysTests"]
            + ["--stackTraceLimit=full", "--reporter=min", "--no-colors"],
            must=[("stdout", r"692 passing")],
        ),
    ]
    return cases


def failed_flow():
    """.failed-tests written by one run, consumed or kept by the next (--failed, --keepFailed)."""
    spec = "rerun.spec.js"
    first = ["--reporter=dot", "--no-colors"]
    fixed = {"RERUN_FIXED": "1"}
    return [
        runtests(
            "cli-failed-written",
            first,
            spec,
            must=[("file .failed-tests", r"rerun hooks")],
        ),
        runtests(
            "cli-failed-rerun-fixed",
            first,
            spec,
            then=(["--failed", "--reporter=spec", "--no-colors"], fixed),
            must=[("then stdout", r"5 passing")],
        ),
        runtests(
            "cli-failed-rerun-unfixed",
            first,
            spec,
            then=(["--failed", "--reporter=dot", "--no-colors"], {}),
            must=[("then file .failed-tests", r"rerun hooks")],
        ),
        runtests(
            "cli-failed-rerun-keep",
            first,
            spec,
            then=(
                ["--failed", "--keepFailed", "--reporter=spec", "--no-colors"],
                fixed,
            ),
            must=[("then file .failed-tests", r"rerun hooks")],
        ),
        runtests(
            "cli-failed-keep-without-rerun",
            first,
            spec,
            then=(["--keepFailed", "--reporter=dot", "--no-colors"], fixed),
            must=[("then file .failed-tests", r"rerun hooks")],
        ),
        runtests(
            "cli-failed-dropped-when-fixed",
            first,
            spec,
            then=(["--reporter=dot", "--no-colors"], fixed),
            must=[("then stdout", r"8 passing")],
        ),
        runtests(
            "cli-failed-without-file",
            ["--failed", "--reporter=dot", "--no-colors"],
            spec,
            must=[("stderr", r"ENOENT")],
        ),
        runtests(
            "cli-timeout-flag",
            ["--timeout=60", "--reporter=spec", "--no-colors"],
            "timeout.spec.js",
            must=[("stdout", r"Timeout of 60ms exceeded")],
        ),
        runtests(
            "cli-timeout-default",
            ["--reporter=spec", "--no-colors"],
            "timeout.spec.js",
            must=[("stdout", r"2 passing")],
        ),
        runtests(
            "cli-colors",
            ["--colors", "--reporter=spec"],
            spec,
            must=[("stdout", "\x1b\\[")],
        ),
        runtests(
            "cli-no-color-alias",
            ["--no-color", "--reporter=spec"],
            spec,
            must=[("stdout", r"4 failing")],
        ),
        Case(
            "cli-baseline-accept",
            [],
            [],
            None,
            {},
            "accept",
            None,
            None,
            (
                ("file tree reference", r"added\.js"),
                ("file tree reference", r"changed\.types\n.*new content"),
                ("file tree reference", r"kept\.js"),
            ),
        ),
    ]


def cli_cases():
    return lists() + real_runs() + failed_flow()


def seed_baselines(cwd):
    """Seeded local baselines and a private reference tree (never the checkout's)."""
    for sub, files in (("local", ACCEPT_LOCAL), ("reference", ACCEPT_REFERENCE)):
        for rel, text in files.items():
            path = os.path.join(cwd, "tests", "baselines", sub, rel)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w") as f:
                f.write(text)


def tree(root, content=False):
    """Sorted `path size sha256` lines (or the text of small files) of a directory tree."""
    lines = []
    for dirpath, _, names in os.walk(root):
        for name in names:
            full = os.path.join(dirpath, name)
            rel = os.path.relpath(full, root)
            with open(full, "rb") as f:
                data = f.read()
            if content and len(data) < 512:
                lines.append(f"{rel}\n{data.decode('utf-8', 'replace')}")
            else:
                digest = hashlib.sha256(data).hexdigest()[:16]
                lines.append(f"{rel} {len(data)} {digest}")
    return "\n".join(sorted(lines)) + "\n"


TITLE = re.compile(r"^ok \d+ (.*)$")


def titles(stdout):
    """Test titles of a tap listing, without the unit test the runner leaks into every list."""
    found = []
    for line in stdout.splitlines():
        m = TITLE.match(line)
        if m and not m.group(1).startswith("unittests::"):
            found.append(m.group(1))
    return found


def partition_problems(results):
    """The shards of a runner together list exactly the runner's whole test list (per engine)."""
    by_name = {c.name: (ref, got) for c, ref, got, _ in results}
    problems = []
    for whole, shards in PARTITIONS.items():
        if whole not in by_name or any(s not in by_name for s in shards):
            continue
        for idx, engine in ((0, "node"), (1, "jsse")):
            want = sorted(titles(by_name[whole][idx]["stdout"]))
            got = sorted(t for s in shards for t in titles(by_name[s][idx]["stdout"]))
            if want != got:
                problems.append(
                    f"{engine}: the shards {shards[0][:-5]}.. list {len(got)} tests, {whole} lists {len(want)}"
                )
    return problems
