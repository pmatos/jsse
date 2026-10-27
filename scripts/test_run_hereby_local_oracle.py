import importlib.util
import json
import os
import tempfile
import unittest
from pathlib import Path

ORACLE = Path(__file__).resolve().parent / "run-hereby-local-oracle.py"

spec = importlib.util.spec_from_file_location("run_hereby_local_oracle", ORACLE)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)


class NormaliseLogTest(unittest.TestCase):
    def test_paths_durations_and_interpreter(self):
        text = "\n".join(
            [
                "Using /w/node/ts/Herebyfile.mjs to run local",
                "> /usr/bin/node scripts/processDiagnosticMessages.mjs src/compiler/diagnosticMessages.json",
                "Finished lib in 40ms",
                "Completed local in 1m 20s",
                "real\t0m20.339s",
            ]
        )
        self.assertEqual(
            oracle.normalise_log(text, "/w/node/ts", "/usr/bin/node"),
            [
                "Using <TREE>/Herebyfile.mjs to run local",
                "> <EXEC> scripts/processDiagnosticMessages.mjs src/compiler/diagnosticMessages.json",
                "Finished lib in <T>",
                "Completed local in <T>",
            ],
        )

    def test_home_is_printed_as_a_tilde(self):
        tree = os.path.expanduser("~/w/ts")
        text = "Using ~/w/ts/Herebyfile.mjs to run local"
        self.assertEqual(
            oracle.normalise_log(text, tree, "/x"),
            ["Using <TREE>/Herebyfile.mjs to run local"],
        )

    def test_the_interpreter_is_normalised_with_or_without_flags(self):
        a = oracle.normalise_log("> /usr/bin/node x.mjs a", "/t", "/usr/bin/node")
        b = oracle.normalise_log("> /opt/jsse x.mjs a", "/t", "/opt/jsse")
        self.assertEqual(a, b)


class DtsGapTest(unittest.TestCase):
    ERR = "ReferenceError: WebAssembly is not defined\nError in dts-services in 1m 52.8s\n"

    def test_only_the_dts_step_missing_webassembly_is_the_gap(self):
        self.assertTrue(oracle.is_dts_gap(1, self.ERR))
        self.assertFalse(oracle.is_dts_gap(0, self.ERR))
        self.assertFalse(oracle.is_dts_gap(1, "Error in build-tsc in 3s\n"))
        self.assertFalse(
            oracle.is_dts_gap(1, "ReferenceError: WebAssembly is not defined\n")
        )
        self.assertFalse(oracle.is_dts_gap(124, self.ERR))
        self.assertFalse(oracle.is_dts_gap(1, "Error in build-tsc in 3s\n" + self.ERR))


class LogDiffTest(unittest.TestCase):
    def test_task_order_does_not_matter(self):
        self.assertEqual(oracle.diff_logs(["a", "b", "c"], ["c", "a", "b"]), [])

    def test_a_missing_or_extra_line_is_reported(self):
        diff = oracle.diff_logs(["a", "b"], ["a", "b", "warning: x"])
        self.assertTrue(any(line == "+warning: x" for line in diff))


class SnapshotTest(unittest.TestCase):
    def test_tree_path_is_ignored_and_content_is_not(self):
        with tempfile.TemporaryDirectory() as tmp:
            trees = []
            for name, body in (("a", "src=/x/a/f.ts\n"), ("b", "src=/x/b/f.ts\n")):
                tree = os.path.join(tmp, name)
                os.makedirs(os.path.join(tree, "built", "local"))
                with open(os.path.join(tree, "built", "local", "f.js"), "w") as f:
                    f.write(body.replace("/x/" + name, tree))
                trees.append(oracle.snapshot(tree))
            self.assertEqual(oracle.compare_snapshots(*trees), [])
            tree = os.path.join(tmp, "c")
            os.makedirs(os.path.join(tree, "built", "local"))
            with open(os.path.join(tree, "built", "local", "f.js"), "w") as f:
                f.write("different\n")
            with open(os.path.join(tree, "built", "local", "g.js"), "w") as f:
                f.write("extra\n")
            lines = oracle.compare_snapshots(trees[0], oracle.snapshot(tree))
            self.assertEqual(len(lines), 2)
            self.assertTrue(lines[0].startswith("differs: built/local/f.js"))
            self.assertEqual(lines[1], "only on jsse: built/local/g.js")

    def test_generated_sources_are_part_of_the_snapshot(self):
        with tempfile.TemporaryDirectory() as tmp:
            os.makedirs(os.path.join(tmp, "src", "compiler"))
            rel = oracle.GENERATED[1]
            with open(os.path.join(tmp, rel), "w") as f:
                f.write("{}")
            self.assertIn(rel, oracle.snapshot(tmp))


class CheckoutTest(unittest.TestCase):
    def make(self, tmp, version):
        with open(os.path.join(tmp, "package.json"), "w") as f:
            json.dump({"name": "typescript", "version": version}, f)

    def test_wrong_version_and_missing_install_are_reported(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertIn("no package.json", oracle.check_checkout(tmp))
            self.make(tmp, "5.9.3")
            self.assertIn("not 6.0.3", oracle.check_checkout(tmp))
            self.make(tmp, "6.0.3")
            self.assertIn("run npm ci", oracle.check_checkout(tmp))


if __name__ == "__main__":
    unittest.main()
