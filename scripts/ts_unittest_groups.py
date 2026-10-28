"""Groups of TypeScript unit tests run by run-ts-unittests-config-sys-oracle.py (issue #1260).

Each group is a mocha `-g` regular expression over the full test titles of the `unittest` runner
(src/testRunner/unittests): `unittests:: config:: ...` (unittests/config), `unittests:: sys:: ...`
(unittests/sys: symlinkWatching, which drives the real ts.sys and the virtual file system) and
`unittests:: debugDeprecation`. unittests/helpers/*.ts registers no test of its own: those files are the
builders (virtualFileSystemWithWatch, tsc, tscWatch, tsserver, solutionBuilder, ...) the tsc, tsbuild, tscWatch
and tsserver suites use, so they are exercised by (and counted with) those groups.
"""

GROUPS = {
    "config-commandLineParsing": r"^unittests:: config:: commandLineParsing",
    "config-configurationExtension": r"^unittests:: config:: configurationExtension",
    "config-convertCompilerOptionsFromJson": r"^unittests:: config:: convertCompilerOptionsFromJson",
    "config-convertTypeAcquisitionFromJson": r"^unittests:: config:: convertTypeAcquisitionFromJson",
    "config-initTSConfig": r"^unittests:: config:: initTSConfig",
    "config-matchFiles": r"^unittests:: config:: matchFiles",
    "config-showConfig": r"^unittests:: config:: showConfig",
    "config-tsconfigParsing": r"^unittests:: config:: tsconfigParsing",
    "sys-symlinkWatching": r"^unittests:: sys:: symlinkWatching",
    "debugDeprecation": r"^unittests:: debugDeprecation",
}
