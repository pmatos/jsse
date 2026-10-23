# Plan: rename `parse_roundtrip` fuzz target to `parse` (#1171)

## Problem restated
The `parse_roundtrip` cargo-fuzz target only calls `jsse::fuzz_parse_bytes`, which parses and discards the result (no print-and-reparse; ADR 0004 says so). The name overstates it. Rename the target to `parse` everywhere: the `[[bin]]` entry, the target source file, the seed corpus directory, the Fuzz workflow, README, ADR 0004, the fuzzing skill, and the doc comment on `fuzz_parse_bytes`. Pure rename, no behaviour change. Parent: #1168 (ClusterFuzzLite config is written against the final names afterwards).

## Spec basis
N/A: no JavaScript behavior change. Fuzz-tooling/CI/doc rename only; nothing under `src/` changes except one doc comment.

## Assumptions
- Complexity: Small (mechanical rename, ~8 text files + 38 `git mv`). No subagents or adversarial review needed.
- ADR 0004 line ~133 ("A true AST-print-and-reparse roundtrip") describes the *concept* the old name implied, not the target name; it stays (it now explains why the target is named `parse`). Only the literal target-name mentions are changed.
- `fuzz.yml` line 2 refers to `docs/adr/0003-...` but the file is `0004-...`. Fixing that stale path is a one-token drive-by on a line already being edited (the same comment line 1 is changed); do it since it is in the same comment block and zero-risk. If the implementer prefers strictness, leaving it is also acceptable.
- Keep `fuzz_parse_bytes` (library fn name) unchanged: the issue only asks for the doc comment to change.
- Corpus is moved with `git mv` so history follows; no new seeds added.

## Key Files
| File | Role | Lines |
|------|------|-------|
| `fuzz/Cargo.toml` | `[[bin]] name`/`path` | 18-19 |
| `fuzz/fuzz_targets/parse_roundtrip.rs` | target source -> `parse.rs` | whole file |
| `fuzz/corpus/parse_roundtrip/` (37 `.js` files) | seed corpus -> `fuzz/corpus/parse/` | dir |
| `.github/workflows/fuzz.yml` | build/replay (fuzz-smoke), deep run, header comment | 1-2, 55-59, 94-95 |
| `src/lib.rs` | doc comment of `fuzz_parse_bytes` | 61-62 |
| `README.md` | fuzz section | 175, 183 |
| `docs/adr/0004-fuzz-lib-target-and-subprocess-differential.md` | target-name mentions | 35, 49, 116, 127, 131 (131 keep wording per Assumptions, 133 body unchanged except name) |
| `.claude/skills/fuzzing/SKILL.md` | skill docs | 8, 14, 36, 46, 49, 61, 79, 101 |

`fuzz/Cargo.lock` has no reference (verified by grep). `fuzz/.gitignore` ignores only `target/artifacts/coverage`. No `.clusterfuzzlite/`, scripts, or pre-commit config mention the target.

## Steps (ordered)

### 1. Move source and corpus
- `git mv fuzz/fuzz_targets/parse_roundtrip.rs fuzz/fuzz_targets/parse.rs`
- `git mv fuzz/corpus/parse_roundtrip fuzz/corpus/parse` (moves all 37 tracked seeds; confirm `git ls-files fuzz/corpus | cut -d/ -f3 | sort | uniq -c` shows `parse 37`, `differential 7`).
- If an untracked local `fuzz/corpus/parse_roundtrip` remains with stray files, it is not part of the repo; ignore.

### 2. Point build config at the new names
- `fuzz/Cargo.toml`: `name = "parse"`, `path = "fuzz_targets/parse.rs"`.
- `.github/workflows/fuzz.yml`: replace every `parse_roundtrip` with `parse` (header comment line 1, step name "Build parse fuzz target", `cargo +nightly fuzz build parse --sanitizer none`, replay `... run parse --sanitizer none -- -runs=0 fuzz/corpus/parse`, deep-run step name and command with `fuzz/corpus/parse`). Optionally correct `0003` -> `0004` on line 2.

### 3. Docs and doc comment
- `src/lib.rs:61-62`: "Backs the `parse` fuzz target (`fuzz/fuzz_targets/parse.rs`)". Re-wrap if rustfmt/line width requires (comment only).
- `README.md` lines 175 and 183: `parse`.
- ADR 0004: replace the five target-name mentions with `` `parse` ``; reflow lines minimally. Leave the "true AST-print-and-reparse roundtrip" bullet's heading; in its body change `` `parse_roundtrip` is parse-must-not-panic only`` to `` `parse` is parse-must-not-panic only`` (this is why the name is accurate).
- `.claude/skills/fuzzing/SKILL.md`: all eight mentions, including the replay command `fuzz/corpus/parse`.

### 4. Verify (no TDD cycle: pure rename, the existing replay gate is the test)
1. `grep -rIn parse_roundtrip --exclude-dir=.git --exclude-dir=target --exclude-dir=test262 --exclude-dir=spec .` returns nothing; also check `git ls-files | grep parse_roundtrip` is empty.
2. `cargo +nightly fuzz build parse --sanitizer none` then `cargo +nightly fuzz run parse --sanitizer none -- -runs=0 fuzz/corpus/parse` (exactly the CI commands; cargo-fuzz 0.13.2 and nightly are installed locally). Expect all 37 seeds executed, exit 0. Cap parallelism: `CARGO_BUILD_JOBS=4`; run with a generous explicit timeout (fat-LTO is not used by the fuzz profile, but the first build is slow).
3. `cargo +nightly fuzz build differential --sanitizer none` is unnecessary unless step 2 shows a Cargo.toml issue; at minimum `cargo +nightly fuzz list` must show `differential` and `parse`.
4. `cargo fmt --check` / `./scripts/lint.sh` (comment edit in `src/lib.rs`); `cargo check` is enough for the lib.
5. Run `./scripts/ensure-hooks.sh` before committing.
6. `git rm PLAN.md` in the implementation commit (stage handoff artefact); do not include `.ultraplan/`.

## Testing / test surface
- test262 / `test262-extra/`: not applicable, no engine or JS-visible change; `test262-pass.txt` is untouched (do not run `--update-baseline`).
- Gates covering this change: the Fuzz workflow `fuzz-smoke` PR job (build + `-runs=0` replay of `fuzz/corpus/parse`) — reproduced locally in Step 4.2 — plus the repo grep in 4.1. `cargo test --release` is not needed beyond `cargo check` since only a doc comment changed in `src/`.
- PR title: `refactor(fuzz): rename parse_roundtrip target to parse` (Conventional Commits; squash-merged). PR body links #1171 and references parent #1168. Note: the PR must touch something so the Fuzz workflow (pull_request trigger, no path filter) runs; it will.

## Regression risk
- Engine/baseline: none. Only a doc comment in `src/lib.rs` changes; no tree-walker, MOP, GC, bytecode, or ObjectKind code is touched.
- Risk is a missed reference leaving the workflow pointing at a nonexistent target/corpus dir: `cargo fuzz run parse ... fuzz/corpus/parse` would fail (or silently run on an empty corpus if the directory is missing—libFuzzer errors on a nonexistent corpus path, so CI is red). Mitigated by the grep and local replay; confirm the replay log reports 37 inputs, not 0.
- `Swatinem/rust-cache` keys are unaffected. Dependabot/other workflows do not reference the target.
- Outside-repo: any external ClusterFuzzLite/OSS-Fuzz setup does not exist yet (#1168 follow-up), so nothing external to update.

## Out of scope
- Renaming `fuzz_parse_bytes` or `differential`.
- Implementing a real parse/print/reparse roundtrip target.
- Adding seeds, changing fuzz budgets, or restructuring the Fuzz workflow.
- The ClusterFuzzLite config and wider fuzz changes in #1168's other sub-issues.
- Rewording ADR 0004 beyond the name substitutions.
