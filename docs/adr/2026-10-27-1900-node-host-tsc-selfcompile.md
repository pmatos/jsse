# Node host mode: tsc self-compile and large-program stress

Status: accepted, epic #1214, issue #1249 (builds on `2026-10-27-0900-node-host-tsc-projects.md`
and the `hereby local` measurement of #1253).

## Decision

1. **A separate script, no pins.** `scripts/run-tsc-selfcompile-oracle.py` runs the same
   `lib/_tsc.js` of typescript 6.0.3 on Node and on `jsse --node` over TypeScript's own sources
   and diffs the two runs live. Nothing is pinned: the oracle is Node, and the inputs (a
   checkout) are too large to digest-pin usefully. It gates on `TS_SRC_DIR` (a v6.0.3 checkout
   with `npm ci`, the same input as the hereby and ts-suite oracles) and takes the compiler from
   `TSC_DIR`, else from `$TS_SRC_DIR/built/local`. It skips without them.
2. **Cases.** Each case copies `src/` (minus `testRunner` and `harness`) into a fresh tree per
   engine, links `node_modules` for `@types/node`, and never writes to the checkout.
   - `compiler-dts`: `-p src/compiler`, the real project config (composite,
     `isolatedDeclarations`): `.d.ts`, `.d.ts.map` and the `.tsbuildinfo`, 157 files.
   - `compiler-js`: the same project with `--composite false --emitDeclarationOnly false
     --declaration false ...` so the checker, parser and emitter are compiled to JS plus
     source maps, 156 files.
   - `src-js`: one program of all ten source projects (452 files with lib and `@types/node`, 347k
     lines, no project references, so no build order), JS emit, 562 files. This is the "tsc over its own
     source" case; the project set is `tsconfig.selfcompile.json`, written into the copy.
   - `generated-large`: 60 generated modules (`gen_program`: unions, mapped, conditional and
     template-literal types, generic classes, a chain of imports) with JS, `.d.ts` and maps,
     240 files. It needs no checkout (only a compiler), runs in about 15 s on jsse and is the
     `--smoke` case that `tests/node_tsc_selfcompile.rs` runs.
3. **What is compared.** Exit status, stdout and stderr, and the sha256 of every file under
   `built/` after replacing the tree's own path (all spellings, including `~`) by `<TREE>`;
   files only on one side are differences. A run must exit 0 on Node (the sources are clean),
   so a case cannot pass by both engines failing. Source maps and `.tsbuildinfo` hold only
   relative paths, and both trees sit at the same depth, so no other normalisation exists.
4. **Measurement.** Each run reports wall time and peak RSS (`wait4` `ru_maxrss` of the
   process, including its worker threads) in a table; `--report FILE` writes them as JSON with
   the binary, the load average and the CPU count, because the numbers are only comparable at
   equal load. Cases run one at a time by default (`--jobs 1`); Node always runs first.
5. **Budget guard.** `scripts/tsc-fixtures/selfcompile-budget.json` has, per
   case, `jsse_max_seconds`, `jsse_max_rss_mb`, `max_time_ratio` and `max_rss_ratio` (a ratio
   to the Node run of the same invocation, so it travels between hosts better than seconds).
   `null` means no limit; `--max-seconds` and `--max-rss-mb` override for one invocation.
   The guard is unit-tested (`scripts/test_run_tsc_selfcompile_oracle.py`: limits exactly at
   and just over the line, ratios, overrides, and the shipped values). **Ruling (2026-10-28,
   the maintainer):** every case has `max_rss_ratio = 6` (jsse peak RSS at most 6x Node's for
   the same case) and **no wall-time cap**; wall time is recorded in the table, `--report` and
   the nightly summary but never enforced, because it moves by an order of magnitude with host
   load (see Result). The worst ratio after #1407 is 5.6x (`src-js`), so the cap leaves about
   7% headroom on `src-js` and 5% on `compiler-js` (5.7x); a regression past it, or a further reduction under #1401, is a deliberate
   edit of this file. Byte-identity and the exit status are enforced regardless.
6. **CI, both non-required.** `.github/workflows/tsc-selfcompile-smoke.yml` runs `--smoke
   --require` (the generated program, a `typescript@6.0.3` download, about 15 s of tsc on
   jsse) on pushes and PRs that touch `src/`, `Cargo.*`, the oracle or the budget; it is a
   separate workflow so a registry hiccup can never fail the required `build` check.
   `.github/workflows/nightly-tsc-selfcompile.yml` runs all four cases daily (cron, like
   `nightly-gc-stress.yml`) against a `v6.0.3` checkout with `npm ci`, writes the table to
   the step summary and uploads the JSON report. `tests/node_tsc_selfcompile.rs` still runs
   `--smoke` when `TSC_DIR` is set and passes with a notice otherwise.

## Result

Node v26.8.2, TypeScript 6.0.3, 24-core host **at load average 65 to 70** (other agents were
building and testing), non-LTO release binary (`CARGO_PROFILE_RELEASE_LTO=off`, 16 codegen
units). All four cases are byte-identical to Node with no engine change.

| case | files | Node s | jsse s | x | Node MiB | jsse MiB | x |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| compiler-dts | 157 | 11.0 | 734 | 67 | 645 | 3690 | 5.7 |
| compiler-js | 156 | 13.3 | 1470 | 111 | 740 | 4742 | 6.4 |
| src-js | 562 | 16.2 | 1609 | 99 | 911 | 6030 | 6.6 |
| generated-large | 240 | 0.8 | 11.1 | 15 | 154 | 384 | 2.5 |

Shipped config (fat LTO, same host and load, a second run of two cases): `compiler-js` Node 11.7 s,
jsse 639 s (54x), RSS 709 vs 4739 MiB (6.7x); `generated-large` Node 0.8 s, jsse 10.3 s (14x), RSS 159 vs 387 MiB.

The wall times are inflated by the load (the LTO run of `compiler-js` took 639 s against 1470 s
without LTO) and must not be used as budgets, which is why none is set. The RSS figures are
the stable ones. A whole-`src` compile peaked at about 6 GiB on jsse against 0.9 GiB on Node,
far outside the 512 MB limit of the test262 runner (#1401 tracks reducing it; #1297 asks for
the same for the full suite). After #1407 (tag-byte index for spilled property maps) the same
byte-identical runs peak at 2986 / 4044 / 5065 MiB, ratios 4.6 / 5.7 / 5.6 for `compiler-dts`
/ `compiler-js` / `src-js`, which the 6x cap is set against.

## Not covered

`tsc -b` over `src/` (every project in dependency order, ~1840 s, #1253) and the d.ts bundles
(#1378). The issue's "other large corpora" are the generated program here and the three
TypeScript source corpora; a node_modules-sized `@types` program would add input size without
new engine paths.
