"""Tests for the mandreel per-phase driver generator (issue #526)."""

import contextlib
import importlib.util
import io
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "gen-mandreel-phases.py"
_spec = importlib.util.spec_from_file_location("gen_mandreel_phases", SCRIPT)
gen = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(gen)

TERMINATOR = "// End of mandreel.js file."
BENCH = f"function runMandreel() {{}}\n{TERMINATOR}\n"


class GenMandreelPhasesTest(unittest.TestCase):
    def _run(self, source_text, *extra):
        """Generates a driver from `source_text`, returning (exit code, driver)."""
        with tempfile.TemporaryDirectory() as d:
            src = Path(d) / "mandreel.js"
            out = Path(d) / "driver.js"
            src.write_text(source_text, encoding="utf-8")
            argv = sys.argv
            sys.argv = ["gen-mandreel-phases", str(src), "-o", str(out), *extra]
            try:
                with contextlib.redirect_stdout(io.StringIO()):
                    code = gen.main()
            finally:
                sys.argv = argv
            return code, (out.read_text(encoding="utf-8") if out.exists() else None)

    def test_rejects_source_without_terminator(self):
        code, text = self._run("var x = 1;\n")
        self.assertEqual(code, 1)
        self.assertIsNone(text)

    def test_emits_phase_markers_inside_a_function(self):
        code, text = self._run(BENCH)
        self.assertEqual(code, 0)
        self.assertIn("function __runMandreelPhased()", text)
        self.assertIn('__mark("run:heapcopy"', text)
        self.assertIn('__mark("TOTAL"', text)
        # The heap-copy loop must sit inside the function, not at top level:
        # a top-level `var` would turn its bindings into global-object lookups.
        self.assertLess(
            text.index("function __runMandreelPhased()"),
            text.index('__mark("run:heapcopy"'),
        )

    def test_prepends_the_clock_when_the_source_lacks_one(self):
        code, text = self._run(BENCH)
        self.assertEqual(code, 0)
        self.assertTrue(text.startswith("const __t0 = Date.now();"))

    def test_keeps_an_existing_clock_and_drops_a_previous_driver(self):
        code, text = self._run(
            f"const __t0 = Date.now();\n{BENCH}\nsetupMandreel(); // stale driver\n"
        )
        self.assertEqual(code, 0)
        self.assertEqual(text.count("const __t0 = Date.now();"), 1)
        self.assertNotIn("stale driver", text)

    def test_subphases_add_render_entry_point_markers(self):
        code, plain = self._run(BENCH)
        self.assertEqual(code, 0)
        self.assertNotIn("__mandreel_internal_preupdate", plain)
        code, sub = self._run(BENCH, "--subphases")
        self.assertEqual(code, 0)
        self.assertIn("__mandreel_internal_preupdate", sub)
        self.assertIn('__mark("render:__draw_total"', sub)


if __name__ == "__main__":
    unittest.main()
