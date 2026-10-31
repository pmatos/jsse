import importlib.util
import unittest
from pathlib import Path

ORACLE = Path(__file__).resolve().parent / "run-hereby-stacks-oracle.py"

spec = importlib.util.spec_from_file_location("run_hereby_stacks_oracle", ORACLE)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)

NODE_ONLY = (
    "Error: x\n"
    "    at TracingChannel.tracePromise (node:diagnostics_channel:547:22)\n"
    "    at loadHerebyfile (file:///ts/loadHerebyfile.js:35:67)\n"
    "    at ModuleJob.run (node:internal/modules/esm/module_job:569:25)\n"
    "    at async main (file:///ts/index.js:13:9)\n"
)


class NormaliseTest(unittest.TestCase):
    def test_diagnostics_channel_and_internal_frames_are_dropped(self):
        base = oracle.load_base()
        text = base.normalise(NODE_ONLY, "/work", "/ts")
        self.assertNotIn("diagnostics_channel", text)
        self.assertNotIn("node:internal", text)

    def test_user_and_async_frames_are_kept(self):
        base = oracle.load_base()
        text = base.normalise(NODE_ONLY, "/work", "/ts")
        self.assertIn("at loadHerebyfile (file://<TS>/loadHerebyfile.js:35:67)", text)
        self.assertIn("at async main", text)

    def test_a_message_that_names_the_module_is_kept(self):
        base = oracle.load_base()
        text = base.normalise(
            "SyntaxError: node:diagnostics_channel is a module\n", "/work", "/ts"
        )
        self.assertIn("is a module", text)


if __name__ == "__main__":
    unittest.main()
