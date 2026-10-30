import importlib.util
import sys
import types
import unittest
from unittest import mock
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_conformance_slices_1274 as conf  # noqa: E402

wrapper_spec = importlib.util.spec_from_file_location(
    "run_ts_conformance_types_oracle",
    SCRIPTS_DIR / "run-ts-conformance-types-oracle.py",
)
wrapper = importlib.util.module_from_spec(wrapper_spec)
wrapper_spec.loader.exec_module(wrapper)

ITS = (
    "Correct errors",
    "Correct module resolution tracing",
    "Correct sourcemap content",
    "Correct JS output",
    "Correct Sourcemap output",
    "Correct type/symbol baselines",
)


def entries(*paths):
    out = []
    for p in paths:
        path, _, cfg = p.partition("|")
        suite = (
            f"conformance tests conformance tests for tests/cases/conformance/{path}"
        )
        if cfg:
            suite += f" ({cfg})"
        for it in ITS:
            title = f"{it} for tests/cases/conformance/{path}"
            out.append((f"{suite} {it} for tests/cases/conformance/{path}", title))
    return out


SUITES = conf.suites_from_titles(
    entries(
        "classes/a.ts",
        "types/union/u.ts",
        "types/union/v.tsx",
        "types/union/w.ts|@target: es5",
        "types/union/w.ts|@target: es2015",
        "types/union/sub/x.ts",
        "types/union/with space.ts",
        "types/unionish/y.ts",
        "types/tuple/t.ts",
        "types/any/z.ts",
    )
)
BUCKETS = {"types": {"passed": 54}, "classes": {"passed": 6}}


class ConformanceTypesSlicesTest(unittest.TestCase):
    def files(self, spec):
        return [s.file.split("conformance/", 1)[1] for s in conf.select(spec, SUITES)]

    def test_suites_parse_file_configuration_and_base_name(self):
        cfg = [s for s in SUITES if s.config]
        self.assertEqual([s.config for s in cfg], ["@target: es5", "@target: es2015"])
        self.assertEqual({s.base for s in cfg}, {"w.ts"})
        self.assertEqual(len(SUITES), 10)
        self.assertTrue(all(len(s.tests) == 6 for s in SUITES))

    def test_dir_term_takes_whole_path_components_only(self):
        self.assertEqual(
            self.files("union"),
            [
                "types/union/u.ts",
                "types/union/v.tsx",
                "types/union/w.ts",
                "types/union/w.ts",
                "types/union/sub/x.ts",
                "types/union/with space.ts",
            ],
        )
        self.assertNotIn("types/unionish/y.ts", self.files("dir:types/union"))

    def test_types_row_is_the_default_slice_and_excludes_other_directories(self):
        self.assertEqual(conf.SLICES["first200"], conf.SLICES["types"])
        self.assertEqual(len(self.files("types")), 9)
        self.assertNotIn("classes/a.ts", self.files("types"))

    def test_subdirectory_rows_cover_the_types_directory_exactly(self):
        self.assertEqual(len(set(conf.TYPES_DIRS)), len(conf.TYPES_DIRS))
        self.assertEqual(set(conf.SLICES), {"types", "first200"} | set(conf.TYPES_DIRS))

    def test_compiler_terms_still_apply_and_union_with_dir(self):
        self.assertEqual(self.files("files:t.ts"), ["types/tuple/t.ts"])
        self.assertEqual(
            self.files("tuple+files:z.ts"), ["types/tuple/t.ts", "types/any/z.ts"]
        )
        self.assertEqual(self.files("match:unionish"), ["types/unionish/y.ts"])

    def test_grep_selects_exactly_the_suites_of_a_batch(self):
        grep = conf.grep_for([s for s in SUITES if s.config])
        self.assertTrue(grep.startswith("^(?:conformance tests conformance tests for"))
        self.assertIn(r"\(@target: es5\)", grep)
        self.assertTrue(grep.endswith(") Correct "))

    def test_pinned_count_uses_whole_top_level_directories_only(self):
        pin = lambda spec: conf.pinned_directory(spec, conf.SLICES, BUCKETS)  # noqa: E731
        self.assertEqual(pin("types"), 54)
        self.assertEqual(pin("first200"), 54)
        self.assertIsNone(pin("union"))
        self.assertIsNone(pin("dir:classes+files:a.ts"))
        self.assertEqual(pin("dir:classes+dir:types"), 60)
        self.assertIsNone(pin("dir:nonexistent"))

    def test_wrapper_replaces_the_runner_settings_of_the_compiler_oracle(self):
        oracle = wrapper.configure(wrapper.load_oracle())
        self.assertEqual(oracle.CONFIG["runners"], ["conformance"])
        self.assertTrue(oracle.SUITE_GREP.startswith("^conformance tests"))
        self.assertIs(oracle.slices, conf)
        self.assertTrue(Path(wrapper.KNOWN).exists())

    def test_wrapper_raises_the_oracle_timeouts_unless_given(self):
        seen = []
        fake = types.SimpleNamespace(main=lambda: seen.append(list(sys.argv)) or 0)
        with (
            mock.patch.object(wrapper, "load_oracle", lambda: fake),
            mock.patch.object(wrapper, "configure", lambda o: o),
            mock.patch.object(sys, "argv", ["x"]),
        ):
            wrapper.main(["--jobs", "2"])
            wrapper.main(["--test-timeout", "5"])
            wrapper.main(["--test-timeout=7", "--known=x.txt"])
        first, second, third = seen
        self.assertEqual(first[first.index("--test-timeout") + 1], "1800000")
        self.assertEqual(first[first.index("--timeout") + 1], "7200")
        self.assertEqual(second[second.index("--test-timeout") + 1], "5")
        self.assertNotIn("--test-timeout", third)
        self.assertNotIn("--known", third)
        self.assertIn("--test-timeout=7", third)

    def test_known_failures_file_is_a_comment_header_plus_tab_separated_entries(self):
        for line in Path(wrapper.KNOWN).read_text(encoding="utf-8").splitlines():
            if line and not line.startswith("#"):
                self.assertEqual(len(line.split("\t")), 2, line)


if __name__ == "__main__":
    unittest.main()
