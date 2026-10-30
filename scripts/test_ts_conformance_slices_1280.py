import importlib.util
import sys
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS_DIR))
import ts_compiler_slices as compiler_slices  # noqa: E402
import ts_conformance_slices_1280 as conf  # noqa: E402

spec = importlib.util.spec_from_file_location(
    "run_ts_conformance_1280_oracle", SCRIPTS_DIR / "run-ts-conformance-1280-oracle.py"
)
wrapper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wrapper)

FILES = (
    "directives/a.ts",
    "directives/b.tsx",
    "pedantic/p.ts",
    "references/r.ts",
    "scanner/ecmascript3/s3.ts",
    "scanner/ecmascript5/s5.ts",
    "parser/ecmascript6/e6.ts",
    "parser/ecmascript6/e6.ts|@target: es5",
    "parser/ecmascript5/top.ts",
    "parser/ecmascript5/Accessors/acc.ts",
    "parser/ecmascript5/ErrorRecovery/er.ts",
    "parser/ecmascript5/IndexSignatures/ix.ts",
    "parser/ecmascript5/Statements/st/deep.ts",
    "parser/ecmascript5/Types/ty.ts",
    "parser/ecmascript2018/e18.ts",
    "parser/ecmascript5X/near.ts",
    "types/t.ts",
    "classes/c.ts",
)


def entries(*files):
    out = []
    for f in files:
        path, _, cfg = f.partition("|")
        full = f"tests/cases/conformance/{path}"
        suite = f"conformance tests conformance tests for {full}"
        if cfg:
            suite += f" ({cfg})"
        for it in compiler_slices.IT_TITLES:
            out.append((f"{suite} {it} for {full}", f"{it} for {full}"))
    return out


SUITES = conf.suites_from_titles(entries(*FILES))


class Conformance1280SlicesTest(unittest.TestCase):
    def files(self, spec):
        return [s.rel for s in conf.select(spec, SUITES)]

    def test_issue_row_names_the_five_directories_only(self):
        dirs = {s.dir for s in conf.select("issue1280", SUITES)}
        self.assertEqual(
            dirs, {"directives", "pedantic", "references", "scanner", "parser"}
        )

    def test_only_term_selects_a_directory_without_its_subdirectories(self):
        self.assertEqual(
            self.files("only:parser/ecmascript5"), ["parser/ecmascript5/top.ts"]
        )
        self.assertEqual(self.files("only:parser"), [])

    def test_dir_term_includes_subdirectories_but_not_name_prefixes(self):
        got = self.files("dir:parser/ecmascript5")
        self.assertIn("parser/ecmascript5/Statements/st/deep.ts", got)
        self.assertNotIn("parser/ecmascript5X/near.ts", got)

    def test_chunks_partition_the_issue_row(self):
        seen = {}
        for name in conf.CHUNKS:
            for s in conf.select(name, SUITES):
                self.assertNotIn(
                    s.key, seen, f"{s.key} in {seen.get(s.key)} and {name}"
                )
                seen[s.key] = name
        # parser/ecmascript5X is a different directory, found by no chunk
        whole = {s.key for s in conf.select("issue1280", SUITES)}
        self.assertEqual(set(seen), whole - {s.key for s in SUITES if "5X" in s.key})

    def test_compiler_and_conformance_terms_still_work(self):
        self.assertEqual(self.files("files:e6.ts"), ["parser/ecmascript6/e6.ts"] * 2)
        self.assertEqual(self.files("dir:pedantic+files:p.ts"), ["pedantic/p.ts"])

    def test_unknown_term_is_rejected(self):
        with self.assertRaises(ValueError):
            conf.select("nonsense:1", SUITES)

    def test_pin_of_oracle_json_counts_the_directories(self):
        pin = wrapper.base.pinned_expected
        wrapper.base.conf = conf
        self.assertEqual(pin("dir:scanner", conf.SLICES), 264)
        self.assertEqual(pin("issue1280", conf.SLICES), 5430 + 264 + 36 + 12 + 90)
        self.assertIsNone(pin("cf2", conf.SLICES))
        self.assertIsNone(pin("only:parser/ecmascript5", conf.SLICES))

    def test_known_failures_file_is_a_comment_header_plus_tab_separated_entries(self):
        known = Path(wrapper.KNOWN)
        self.assertEqual(known.name, "conformance-1280-known-failures.txt")
        for line in known.read_text(encoding="utf-8").splitlines():
            if line and not line.startswith("#"):
                self.assertEqual(len(line.split("\t")), 2, line)


if __name__ == "__main__":
    unittest.main()
