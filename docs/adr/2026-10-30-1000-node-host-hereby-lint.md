# Node host mode: `hereby lint`

Status: accepted, epic #1214, issue #1295 (builds on `2026-10-28-1400-node-host-eslint-rules.md`, which made ESLint
10 and typescript-eslint load on jsse, and `2026-10-27-1100-node-host-hereby.md`).

## Context

`hereby lint` spawns `node node_modules/eslint/bin/eslint --cache --cache-location ./.eslintcache
--report-unused-disable-directives --max-warnings 0 .` with the TypeScript checkout's flat config: the
typescript-eslint recommended sets, `eslint-plugin-regexp`, the checkout's own rules (`scripts/eslint/rules`, some of
them type-aware) and `parserOptions.project` for `src/` (`src/tsconfig-eslint.json`: the whole compiler as one
program, so every rule that asks for types works over the checker) and `scripts/` (checkJs over the `.mjs` files). It is
the biggest Node/ESM dependency graph of the epic that runs on every file of the repository: 749 files, about 18,000
cache entries.

## Result

No engine or host change was needed. On the v6.0.3 tree `hereby lint` exits 0 on `jsse --node` as on Node with the
same output (`Linting: ...`, `Finished lint in <T>`), the `.eslintcache` written by the first run is identical and
the second run (cache hit) matches too. It takes about 20 minutes on jsse (release build without LTO) against 36 to 45 s
on Node, a factor of about 30 (the ESLint run of two files that need the src program takes 4 minutes on jsse and 5 s
on Node). The default global environment is untouched; the test262 pass list is
unaffected.

A lint that reports nothing says little about the diagnostics path, so the oracle adds control files
(`scripts/ts-lint-fixtures`): `lintControl.ts` in `src/compiler` (27 problems from 14 rules, among them the
type-aware `local/argument-trivia` and `@typescript-eslint/no-unnecessary-type-assertion`, three regexp rules with the
backtracking analysis, an unused `eslint-disable`), `lintControl.mjs` in `scripts` (11 errors) and `lintWarn.mjs` (one
warning that `--max-warnings 0` turns into exit status 1). They are linted with stylish, json, json-with-metadata and html
output, with `-o`, with `--fix` (the rewritten files are compared), with `--cache` twice, and `--print-config` for a
source, a script and a d.ts file (the whole resolved config, every plugin loaded). Every case matches.

The one difference found: the html formatter prints `Generated on <Date.prototype.toString()>`; V8 spells the zone
`(Central European Standard Time)` and jsse `(CET)`. ECMA-262 leaves the text after the offset to the implementation
(TimeZoneString), so this is not a bug and the oracle normalises the line. The same machinery has a neighbour in
`Intl.DateTimeFormat('en', {timeZoneName: 'long'})`, which V8 prints with a comma after the date; both are filed as #1480 (under #1418).

## Oracle

`scripts/run-ts-lint-oracle.py` (cases in `scripts/ts_lint_cases.py`, guard `tests/node_ts_lint.rs`, unit tests
`scripts/test_run_ts_lint_oracle.py`, how-to `docs/agents/library-tests-typescript-conformance-lint.md`). Cases run in
a private tree per engine and never write into `TS_SRC_DIR`. The light tree links `node_modules`; `full` copies the
checkout. `ts-suite/lint-known-failures.txt` is empty.
