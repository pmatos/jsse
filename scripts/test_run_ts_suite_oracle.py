import contextlib
import importlib.util
import io
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location(
    "ts_suite_oracle", SCRIPTS_DIR / "run-ts-suite-oracle.py"
)
oracle = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(oracle)

CASES = [
    (
        "  compiler tests compiler tests for tests/cases/compiler/Abc.ts",
        "Correct errors",
        True,
    ),
    (
        "  compiler tests compiler tests for tests/cases/compiler/Abc.ts",
        "Correct types",
        True,
    ),
    (
        "  compiler tests compiler tests for tests/cases/compiler/zed.ts",
        "Correct errors",
        False,
    ),
    (
        "  conformance tests conformance tests for tests/cases/conformance/es6/x.ts",
        "Correct errors",
        True,
    ),
    (
        "  fourslash tests tests/cases/fourslash/foo.ts",
        "fourslash test foo.ts runs correctly",
        True,
    ),
    (
        "  fourslash-server tests tests/cases/fourslash/server/foo.ts",
        "fourslash-server test foo.ts",
        True,
    ),
    ("  projects tests Compiling project for [a]", "Correct output", True),
    ("  transpile tests tests/cases/transpile/t.ts", "has expected js output", True),
    ("  unittests:: tsserver:: a", "works", True),
    ("  unittests:: tsserver:: a", "works", True),
    ("  unittests:: semver Version", "toString", True),
    ("comment parsing", "skips shebang", True),
    ("", "unittests:: tsbuildWatch:: watchMode:: x", True),
]


def xunit(cases):
    root = ["<testsuite>"]
    for cn, name, ok in cases:
        body = "" if ok else "<failure>boom</failure>"
        root.append(f'<testcase classname="{cn}" name="{name}">{body}</testcase>')
    return "\n".join(root + ["</testsuite>"])


class ClassifyTest(unittest.TestCase):
    def test_runners_and_suites(self):
        c = oracle.classify
        self.assertEqual(
            c("  compiler tests compiler tests for x", ""), ("compiler", None)
        )
        self.assertEqual(c("fourslash-server tests a", ""), ("fourslash-server", None))
        self.assertEqual(c("fourslash tests a", ""), ("fourslash", None))
        self.assertEqual(c("projects tests a", ""), ("project", None))
        self.assertEqual(c("unittests:: tsserver:: a b", ""), ("unittests", "tsserver"))
        self.assertEqual(
            c("unittests :: internalApi :: x", ""), ("unittests", "internalApi")
        )
        self.assertEqual(
            c("", "unittests:: tsbuildWatch:: x"), ("unittests", "tsbuildWatch")
        )
        self.assertEqual(c("comment parsing", "x"), ("unittests", "other"))

    def test_buckets(self):
        b = oracle.bucket
        self.assertEqual(
            b("compiler", "compiler tests for tests/cases/compiler/abc.ts"), "A"
        )
        self.assertEqual(b("conformance", "tests/cases/conformance/es6/x/y.ts"), "es6")
        self.assertEqual(
            b("fourslash-server", "tests/cases/fourslash/server/zed.ts"), "Z"
        )
        self.assertIsNone(b("project", "Compiling project for [a]"))


class SummariseTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)

    def write(self, name, cases):
        path = Path(self.tmp.name) / name
        path.write_text(xunit(cases))
        return str(path)

    def test_counts_digests_and_failures(self):
        runners, ids = oracle.summarise(self.write("a.xml", CASES))
        self.assertEqual(runners["compiler"]["passed"], 2)
        self.assertEqual(runners["compiler"]["failed"], 1)
        self.assertEqual(
            runners["compiler"]["buckets"],
            {"A": {"passed": 2, "failed": 0}, "Z": {"passed": 0, "failed": 1}},
        )
        self.assertEqual(runners["compiler"]["files"], 2)
        self.assertEqual(len(runners["compiler"]["failed_ids"]), 1)
        self.assertEqual(runners["unittests"]["suites"]["tsserver"]["passed"], 2)
        self.assertEqual(runners["unittests"]["suites"]["tsbuildWatch"]["passed"], 1)
        self.assertEqual(
            sorted(runners),
            [
                "compiler",
                "conformance",
                "fourslash",
                "fourslash-server",
                "project",
                "transpile",
                "unittests",
            ],
        )
        self.assertEqual(len(ids["unittests"][0]), 5)

    def test_digest_is_order_independent_and_count_sensitive(self):
        a = oracle.summarise(self.write("a.xml", CASES))[0]
        b = oracle.summarise(self.write("b.xml", list(reversed(CASES))))[0]
        self.assertEqual(a["unittests"]["digest"], b["unittests"]["digest"])
        fewer = oracle.summarise(self.write("c.xml", CASES[:-2]))[0]
        self.assertNotEqual(a["unittests"]["digest"], fewer["unittests"]["digest"])

    def test_check_reports_differences_only(self):
        runners, _ = oracle.summarise(self.write("a.xml", CASES))
        obs = {"totals": {"passed": 1}, "runners": runners}
        self.assertEqual(oracle.check(dict(obs), obs), [])
        other, _ = oracle.summarise(self.write("b.xml", CASES[:-2]))
        problems = oracle.check(obs, {"totals": {"passed": 1}, "runners": other})
        self.assertTrue(any("unittests/passed" in p for p in problems))
        self.assertTrue(any("unittests/digest" in p for p in problems))


class PinTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)

    def git(self, *args, cwd):
        return subprocess.run(
            ["git", "-c", "user.name=t", "-c", "user.email=t@t", *args],
            cwd=cwd,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()

    def upstream(self):
        up = self.root / "up"
        up.mkdir()
        self.git("init", "-q", cwd=up)
        (up / "package-lock.json").write_text("{}")
        self.git("add", ".", cwd=up)
        self.git("commit", "-q", "-m", "x", cwd=up)
        self.git("tag", "v1", cwd=up)
        return up, self.git("rev-parse", "HEAD", cwd=up)

    def test_fetch_verifies_the_commit(self):
        up, head = self.upstream()
        pin = {"repo": str(up), "tag": "v1", "commit": head}
        src = str(self.root / "cache" / "ts-src")
        oracle.fetch(src, pin)
        oracle.fetch(src, pin)
        with self.assertRaises(oracle.Fatal):
            oracle.fetch(src, dict(pin, commit="0" * 40))

    def test_fetch_rejects_modified_tree(self):
        up, head = self.upstream()
        pin = {"repo": str(up), "tag": "v1", "commit": head}
        src = self.root / "cache" / "ts-src"
        oracle.fetch(str(src), pin)
        (src / "package-lock.json").write_text("changed")
        with self.assertRaises(oracle.Fatal):
            oracle.fetch(str(src), pin)

    def test_install_rejects_a_changed_lockfile(self):
        src = self.root / "src"
        src.mkdir()
        (src / "package-lock.json").write_text("{}")
        with self.assertRaises(oracle.Fatal) as ctx:
            oracle.install(str(src), {"lockfile_sha256": "0" * 64, "mocha": "1"})
        self.assertIn("package-lock.json", str(ctx.exception))

    def test_cache_dir_must_be_outside_the_repository(self):
        with self.assertRaises(oracle.Fatal):
            oracle.cache_dir(str(oracle.ROOT) + "/ts-cache")
        self.assertEqual(oracle.cache_dir(str(self.root)), str(self.root))

    def test_yargs_patch_drops_only_the_module_type(self):
        p = self.root / "src" / "node_modules" / "mocha" / "node_modules" / "yargs"
        p.mkdir(parents=True)
        (p / "package.json").write_text(
            '{"name": "yargs", "type": "module", "main": "x"}'
        )
        oracle.patch_mocha_yargs(str(self.root / "src"))
        self.assertEqual(
            json.loads((p / "package.json").read_text()), {"name": "yargs", "main": "x"}
        )

    def test_committed_pin_and_manifest_agree(self):
        pin = oracle.load_pin()
        with open(oracle.MANIFEST, encoding="utf-8") as f:
            manifest = json.load(f)
        self.assertEqual(
            manifest["typescript"],
            {k: pin[k] for k in ("repo", "tag", "commit", "lockfile_sha256")},
        )
        self.assertEqual(manifest["mocha"], pin["mocha"])
        self.assertEqual(len(pin["commit"]), 40)
        total = sum(r["passed"] + r["failed"] for r in manifest["runners"].values())
        self.assertEqual(
            total, manifest["totals"]["passed"] + manifest["totals"]["failed"]
        )

    def test_unknown_stage_is_a_usage_error(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            oracle.main(["nope"])


if __name__ == "__main__":
    unittest.main()
