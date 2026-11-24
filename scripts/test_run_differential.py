import contextlib
import importlib.util
import io
import os
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
RUNNER = REPO_ROOT / "scripts" / "run-differential.py"


def _load_runner():
    spec = importlib.util.spec_from_file_location("run_differential", RUNNER)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


rd = _load_runner()
Result = rd.Result


class ClassifyTests(unittest.TestCase):
    def test_ok_normalizes_line_endings_and_trailing_space(self):
        self.assertEqual(
            rd.classify("node", 0, "a  \r\nb\n\n", ""), Result("ok", "a\nb")
        )

    def test_uncaught_error_class_is_found_after_node_source_context(self):
        stderr = "/tmp/x.js:3\nfoo: bar()\n   ^\n\nTypeError: bar is not a function\n    at ..."
        self.assertEqual(rd.classify("node", 1, "", stderr).outcome, "throw:TypeError")

    def test_jsse_panic_exit_and_signals_are_crashes(self):
        self.assertEqual(rd.classify("jsse", 101, "", "panicked").outcome, "crash")
        self.assertEqual(rd.classify("node", -11, "", "").outcome, "crash")
        self.assertEqual(rd.classify("node", 101, "", "").outcome, "throw:?")

    def test_allocation_failure_is_oom_not_crash(self):
        self.assertEqual(
            rd.classify("jsse", -6, "", "memory allocation of 8 bytes failed").outcome,
            "oom",
        )

    def test_timeout(self):
        self.assertEqual(rd.classify("jsse", None, "partial\n", "").outcome, "timeout")

    def test_boa_uncaught_report_moves_from_stdout_to_outcome(self):
        out = "printed\nUncaught Error: RangeError: bad (t.js:1:7)\n    at <main> (t.js:1:7)\n"
        self.assertEqual(
            rd.classify("boa", 1, out, ""), Result("throw:RangeError", "printed")
        )
        self.assertEqual(
            rd.classify("boa", 1, "Uncaught Error: 1\n", "").outcome, "throw:?"
        )

    def test_boa_parse_error_comes_from_the_stderr_report(self):
        stderr = "Error: \n   0: \x1b[91mSyntaxError: 'let' is disallowed\x1b[0m\n"
        self.assertEqual(rd.classify("boa", 1, "", stderr).outcome, "throw:SyntaxError")


class JudgeTests(unittest.TestCase):
    def setUp(self):
        self.ok = Result("ok", "1\n2")
        self.other = Result("ok", "1\n3")

    def test_match(self):
        self.assertEqual(
            rd.judge("f.js", self.ok, {"node": self.ok}, {}).status, "match"
        )

    def test_divergence_reports_first_differing_line(self):
        report = rd.judge("f.js", self.ok, {"node": self.other}, {})
        self.assertEqual(report.status, "diverge")
        self.assertIn("stdout line 2: jsse='2' node='3'", report.detail)
        self.assertEqual(len(report.fingerprint), 16)

    def test_outcome_difference_is_reported(self):
        thrown = Result("throw:TypeError", "1")
        report = rd.judge("f.js", thrown, {"node": self.ok}, {})
        self.assertIn("outcome: jsse=throw:TypeError node=ok", report.detail)

    def test_approval_is_pinned_to_fingerprint(self):
        fp = rd.fingerprint(self.ok, self.other)
        approved = {"f.js": {"file": "f.js", "fingerprint": fp, "reason": "known"}}
        self.assertEqual(
            rd.judge("f.js", self.ok, {"node": self.other}, approved).status, "approved"
        )
        moved = Result("ok", "1\n4")
        self.assertEqual(
            rd.judge("f.js", self.ok, {"node": moved}, approved).status, "changed"
        )

    def test_boa_second_opinion_is_noted(self):
        report = rd.judge("f.js", self.ok, {"node": self.other, "boa": self.ok}, {})
        self.assertIn("boa agrees with jsse", report.detail)


class LoadApprovedTests(unittest.TestCase):
    def test_entries_require_file_fingerprint_and_reason(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "approved.toml"
            path.write_text(
                '[[approved]]\nfile = "a.js"\nfingerprint = "x"\n', encoding="utf-8"
            )
            with self.assertRaises(ValueError):
                rd.load_approved(path)

    def test_checked_in_list_parses(self):
        rd.load_approved(rd.DEFAULT_APPROVED)


@unittest.skipUnless(
    os.name == "posix", "uses executable shell scripts as fake engines"
)
class EndToEndTests(unittest.TestCase):
    """Drives main() with fake engines so the test needs neither jsse nor node."""

    def make_engine(self, root: Path, name: str, body: str) -> str:
        path = root / name
        path.write_text(
            f"#!{sys.executable}\n" + textwrap.dedent(body), encoding="utf-8"
        )
        path.chmod(0o755)
        return str(path)

    def run_main(self, jsse_out: str, approved_text: str = "", observe: bool = False):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            corpus = root / "corpus"
            corpus.mkdir()
            (corpus / "t.js").write_text("show(1)\n", encoding="utf-8")
            approved = root / "approved.toml"
            approved.write_text(approved_text, encoding="utf-8")
            jsse = self.make_engine(root, "jsse", f"print({jsse_out!r})\n")
            # The fake node is invoked as `node --expose-gc RUNNER FILE`.
            node = self.make_engine(root, "node", "print('1')\n")
            argv = [
                str(corpus),
                "--jsse",
                jsse,
                "--node",
                node,
                "--no-boa",
                "--approved",
                str(approved),
                "-j",
                "1",
            ]
            if observe:
                argv.append("--observe")
            json_out = root / "out.json"
            argv += ["--json", str(json_out)]
            env_summary = os.environ.pop("GITHUB_STEP_SUMMARY", None)
            try:
                with contextlib.redirect_stdout(io.StringIO()):
                    code = rd.main(argv)
            finally:
                if env_summary is not None:
                    os.environ["GITHUB_STEP_SUMMARY"] = env_summary
            return code, json_out.read_text(encoding="utf-8")

    def test_match_exits_zero(self):
        code, _ = self.run_main("1")
        self.assertEqual(code, 0)

    def test_divergence_exits_one_unless_observe(self):
        self.assertEqual(self.run_main("2")[0], 1)
        self.assertEqual(self.run_main("2", observe=True)[0], 0)


if __name__ == "__main__":
    unittest.main()
