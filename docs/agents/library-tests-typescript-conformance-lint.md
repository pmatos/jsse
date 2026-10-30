# Library Tests: TypeScript `hereby lint` oracle (#1295)

Part of the TypeScript-on-jsse epic (#1214); the other TypeScript oracles are listed in
[library-tests-typescript.md](library-tests-typescript.md). Design and measurements:
`docs/adr/2026-10-30-1000-node-host-hereby-lint.md`.

`TS_SRC_DIR=<TypeScript v6.0.3 checkout, npm ci and generated diagnostics> uv run python scripts/run-ts-lint-oracle.py [--jobs N (at most 4)] [--jsse BIN] [--work DIR] [--list] [--require] [case ...]`

`hereby lint` is `node node_modules/eslint/bin/eslint --cache --cache-location ./.eslintcache
--report-unused-disable-directives --max-warnings 0 .` (ESLint 10, typescript-eslint, the regexp plugin and the
custom rules of `scripts/eslint/rules`): type-aware rules over `src/` (one program of the whole compiler) and a second
program for `scripts/*.mjs`. The repository is clean on Node, so a clean run proves little; the oracle also lints
control files that break about thirty rules.

Each case runs its steps in a private tree per engine (the light tree links `node_modules`, `tests`, `built`, `lib`;
`full` copies the whole checkout; the checkout is never written) and compares Node and `jsse --node`: the exit
status, stdout and stderr of every step (tree path, durations, the interpreter and the html formatter's wall-clock
line normalised; in `.eslintcache` each `hashOfConfig`, which covers the tree path, is compared as a partition of the files) and every file the steps create, change or remove (`--fix` rewrites, `-o` reports, `.eslintcache`).
Node must satisfy each case's own checks (`must`: exit status, rule ids in the output, the fixed file), so a control
that reports nothing fails instead of comparing two empty outputs.

| Case | What it runs | jsse time |
| --- | --- | --- |
| `version`, `print-config-src/-scripts/-dts` | `--version`; the resolved flat config of a source, a script and a d.ts file (loads every plugin) | seconds |
| `ctl-scripts`, `ctl-scripts-fix`, `ctl-warning-only`, `ctl-scripts-formats` | `scripts/ts-lint-fixtures/lintControl.mjs` (11 errors), `--fix` then lint again, a warning-only file under `--max-warnings 0`, formatters stylish/json/json-with-metadata/html and `-o report.json` | 10 to 60 s |
| `ctl-src`, `ctl-src-json`, `ctl-src-fix` | `lintControl.ts` in `src/compiler` (27 problems: `argument-trivia`, `no-in-operator`, `no-unnecessary-type-assertion`, three regexp rules, naming-convention, an unused disable directive, ...), as `json` with fix ranges, and `--fix` | 4 to 5 min each |
| `ctl-both-cache` | both controls twice with `--cache` (the second run reads the file the first wrote) | 5 min |
| `full` (named only) | `hereby lint` on the whole checkout twice (the second run hits the cache) | about 45 min |

Without `TS_SRC_DIR` the script prints SKIP and exits 0 (exit 1 with `--require`). A case that differs on jsse must be
listed in `ts-suite/lint-known-failures.txt` (`case<TAB>issue`; empty: nothing differs); a listed case that matches
fails the run. Guards: `tests/node_ts_lint.rs` (the cheap cases and `ctl-src`), `scripts/test_run_ts_lint_oracle.py`
(normalisation, comparison, verdicts, case table).

## Results

`hereby lint` of the TypeScript v6.0.3 tree exits 0 with identical output on both engines: 20 minutes on jsse
(release build without LTO), 36 to 45 s on Node. All 13 cases match. The one difference seen is the html formatter's
`Generated on <Date.prototype.toString()>`, which names the time zone `(CET)` on jsse and `(Central European
Standard Time)` on Node; the text after the offset is implementation-defined (ECMA-262 TimeZoneString), so the
oracle normalises the line.
