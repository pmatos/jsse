# Plan: Adopt ClusterFuzzLite (ASan, persistent corpus, capped PR fuzzing) for #1172

## Problem restated
Fuzzing today is `.github/workflows/fuzz.yml`: a PR job that only replays the seed corpus (`-runs=0`, `--sanitizer none`) and a nightly `fuzz-deep` job that runs `parse_roundtrip` and `differential` for 5 minutes each with no sanitizer. Nothing persists the discovered corpus. #1168 decided to move to ClusterFuzzLite (CFL): ASan builds, a corpus stored across runs, a blocking PR job capped at ~2 minutes of fuzzing, and a longer scheduled batch run. `differential` needs `node` and a release `jsse` binary, so it cannot run inside CFL's container and stays in the existing Fuzz workflow's nightly job. The `parse_roundtrip` -> `parse` rename (#1171, still open) must land first so the CFL config uses final names.

## Spec basis
N/A: no JavaScript behavior change. This is CI/tooling/docs under `.github/`, a new `.clusterfuzzlite/`, and `docs/`.

## Assumptions
- **#1171 (rename) is not on this branch; PR #1179 (OPEN, "refactor(fuzz): rename parse_roundtrip target to parse") implements it.** Before anything else check `gh pr view 1179 --json state,mergedAt`; if merged, rebase onto `origin/main`; if open, do not duplicate it (see below).
- (`fuzz/fuzz_targets/parse_roundtrip.rs` still exists; #1171 is OPEN). Implementation stage first runs `git fetch origin main` and rebases if the rename has landed. If #1179 is still open at implementation time, either rebase this branch onto #1179's head branch or, failing that, include the minimal rename as its own first commit (target file, `[[bin]]` name/path in `fuzz/Cargo.toml`, `git mv fuzz/corpus/parse_roundtrip fuzz/corpus/parse`, the `fuzz_parse_bytes` doc comment in `src/lib.rs:67`, and the doc/workflow references) and say so in the PR body and a comment on #1171, so the commit is easy to drop if #1171 lands first. The rest of this plan assumes the name `parse`.
- **Corpus storage = GitHub Actions artifacts (CFL default), not a `storage-repo`.** A storage repo needs a `PERSONAL_ACCESS_TOKEN` secret and a second repo, neither of which this unattended run can create. Artifact storage is zero-config; CFL's batch run uploads the merged corpus artifact and PR/batch runs download the latest one from the same repo. Trade-off: artifact retention (repo default 90 days) and no browsable coverage report. Revisit with `storage-repo` as a follow-up.
- **Sanitizer = `address` only.** CFL's Rust support only offers ASan (CFL docs); matches #1168.
- **PR cap**: `fuzz-seconds: 120`. That bounds the fuzzing phase. Docker image build plus the ASan cargo build of the icu-heavy dependency tree is extra wall time and is not under the cap; `timeout-minutes: 45` on the job keeps it bounded.
- **Batch cadence**: daily, 03:30 UTC, `fuzz-seconds: 1800`, plus `workflow_dispatch`. (`fuzz.yml` differential stays at 03:00 UTC; staggering avoids runner contention only.)
- **No SARIF** (`output-sarif: false`): SARIF upload needs `security-events: write` and code-scanning setup; the crash is reported in the job log and via artifact upload instead.
- **Continuous build workflow IS in scope.** CFL docs: continuous builds (push to main, `upload-build: true`) are what PR fuzzing uses to decide whether a crash found in a PR is novel; non-novel crashes are not reported. This is the main defence against the blocking gate going red on latent bugs (the risk ADR 0004 raised), so it is added as `clusterfuzzlite-build.yml`.
- **Corpus artifact storage must be verified from source, not assumed.** Read the filestore in google/oss-fuzz (`infra/cifuzz/filestore/github_actions/`) / clusterfuzzlite issues to confirm the `v1` actions still upload/download the previous run's corpus via GitHub artifacts (GitHub retired the v3 artifact protocol). If broken, `storage-repo` + a `PERSONAL_ACCESS_TOKEN` secret is the only route: that needs the owner, so record it as a precondition via `gh issue comment 1172` and ship the artifact-based config anyway.
- **No prune/coverage CFL jobs in this PR**: not in the acceptance criteria; listed as follow-ups.
- **Base image `gcr.io/oss-fuzz-base/base-builder-rust:v1`** (verified to exist with `docker manifest inspect`); CFL's own examples use the `v1` tag matching `actions/*@v1`.
- **Action pinning**: repo pins third-party actions by commit SHA with a `# vX.Y.Z` comment (see `fuzz.yml`). Resolve `google/clusterfuzzlite/actions/build_fuzzers@v1` and `run_fuzzers@v1` to SHAs (`gh api repos/google/clusterfuzzlite/git/ref/tags/v1`, deref if annotated) and pin the same way.

## Key Files
| File | Role | Lines of Interest |
|------|------|-------------------|
| `.github/workflows/fuzz.yml` | trim to nightly `differential` only | whole file (header comment 1-11, `fuzz-smoke` 29-66, `fuzz-deep` 68-105) |
| `.clusterfuzzlite/Dockerfile` [new] | builder image | - |
| `.clusterfuzzlite/build.sh` [new] | builds `parse` with ASan, packages seed corpus into `$OUT` | - |
| `.clusterfuzzlite/project.yaml` [new] | `language: rust` | - |
| `.github/workflows/clusterfuzzlite-pr.yml` [new] | blocking code-change fuzzing, 120 s | - |
| `.github/workflows/clusterfuzzlite-batch.yml` [new] | scheduled batch fuzzing, stores corpus | - |
| `.github/workflows/clusterfuzzlite-build.yml` [new] | push-to-main build with `upload-build: true` so PR fuzzing can drop non-novel crashes | - |
| `fuzz/Cargo.toml` | `[[bin]]` list; only `parse` goes to CFL | 14-26 |
| `fuzz/corpus/parse/` (37 seeds, 156K) | CFL seed corpus source | - |
| `src/lib.rs` | `fuzz_parse_bytes` (line 67), `ENGINE_STACK_SIZE` 128 MiB (line 28), `run_on_engine_stack` (line 41) - read-only reference | - |
| `README.md` | "## Fuzzing" section | 173-194 |
| `.claude/skills/fuzzing/SKILL.md` | run/triage workflow; stale CI statements | lines on `fuzz-smoke`, `fuzz-deep`, corpus hygiene |
| `docs/adr/2026-10-23-HHMM-clusterfuzzlite-fuzzing.md` [new] | records the decisions (use the real UTC time at authoring) | convention in `docs/adr/README.md` |
| `docs/adr/0004-fuzz-lib-target-and-subprocess-differential.md` | add "partly superseded" pointer; it states ASan and PR mutation fuzzing are out of scope (lines ~116-141) | 108-142 |

Branch protection: ruleset 13071909 requires only `build` and `Validate PR title`. `Fuzz / fuzz-smoke` is NOT a required check, so removing it needs no ruleset change. The new CFL check is "blocking" in that it fails the PR check; making it a *required* check is a repo-settings action for the owner (note in PR body, do not attempt via API).

## Steps

### 1. Sync with the rename (#1171)
- Fetch/rebase as described under Assumptions. Verify `grep -rn parse_roundtrip . --exclude-dir=target --exclude-dir=test262 --exclude-dir=spec --exclude-dir=.git` is empty before continuing (excluding `.ultraplan/`, `PLAN.md`).

### 2. Add `.clusterfuzzlite/project.yaml`, `Dockerfile`, `build.sh`
- `project.yaml`: `language: rust`.
- `Dockerfile`:
  ```
  FROM gcr.io/oss-fuzz-base/base-builder-rust:v1
  COPY . $SRC/jsse
  COPY .clusterfuzzlite/build.sh $SRC/
  WORKDIR $SRC/jsse
  ```
  The build context is the repo root; checkout without submodules keeps `test262/` and `spec/` empty, so the copy is small. Add a root `.dockerignore` [new] only if needed (`target/`, `.git`); check that none exists first (`test -e .dockerignore`).
- `build.sh` (executable, `set -eu`): `cd "$SRC/jsse"`; `cargo fuzz build -O --debug-assertions parse` (build only CFL targets - never `differential`); copy `fuzz/target/x86_64-unknown-linux-gnu/release/parse` to `$OUT/`; `zip -jq "$OUT/parse_seed_corpus.zip" fuzz/corpus/parse/*`. Keep the target list as one shell variable so adding targets (#1168 follow-ups) is a one-line change. `--debug-assertions` also selects the debug-sized recursion/parse-depth limits (`cfg!(debug_assertions)`), which matters under ASan (frames are larger).
- **Toolchain check first** (cheap, avoids a gate that fails on every PR): `docker run --rm gcr.io/oss-fuzz-base/base-builder-rust:v1 bash -c 'rustc --version; cargo fuzz --version'` (first pull is large; run with a generous timeout, 10+ min, a 100 s attempt during planning timed out on the pull). The repo needs edition 2024 + nightly `let_chains` (local rustc 1.98.1). If the image's nightly is too old, drop the `:v1` tag (untagged = latest) or add `RUN rustup update nightly && rustup default nightly` to the Dockerfile.
- Reuses: existing `[[bin]] parse` in `fuzz/Cargo.toml`; no `jsse` feature changes (`default-features = false` already drops jemalloc).
- Cap parallelism: export `CARGO_BUILD_JOBS` is not needed on hosted runners; for any local validation use `CARGO_BUILD_JOBS=4`.

### 3. Add `.github/workflows/clusterfuzzlite-pr.yml`
- Follow CFL's documented step list: its examples have no `actions/checkout` step (`build_fuzzers` fetches the PR/ref itself via `github-token`). Do not add `actions/checkout` unless the first run shows the Dockerfile `COPY . $SRC/jsse` sees an empty context; then add it with `persist-credentials: false` and `submodules: false`.
- `name: ClusterFuzzLite PR`; `on: pull_request: branches: [main]`; `permissions: contents: read, actions: read` (needed to download the stored corpus artifact; verify against CFL behavior in the first run and widen only if the log shows a 403).
- `concurrency: group: ${{ github.workflow }}-${{ github.ref }}`, `cancel-in-progress: true`.
- Job `pr` on `ubuntu-latest`, `timeout-minutes: 45`, steps: `build_fuzzers` (`language: rust`, `sanitizer: address`, `github-token: ${{ secrets.GITHUB_TOKEN }}`), then `run_fuzzers` (`mode: code-change`, `sanitizer: address`, `fuzz-seconds: 120`, `output-sarif: false`, `github-token`).
- Add `if: failure()` `actions/upload-artifact` of CFL's crash output directory (CFL writes crashes under the workspace `artifacts/` / `out/` dir - confirm the exact path in the first run logs; use `if-no-files-found: ignore`) so a testcase is downloadable without scraping logs.
- Reuses the `actions/upload-artifact` pin line from `.github/workflows/fuzz.yml` (line 63).

### 4. Add `.github/workflows/clusterfuzzlite-batch.yml`
- `on: schedule: cron "30 3 * * *"` and `workflow_dispatch`; same permissions; `concurrency` without cancelling a running batch (`cancel-in-progress: false`) so a corpus upload is never aborted mid-way.
- Job `batch`, `timeout-minutes: 90`: same build step, `run_fuzzers` with `mode: batch`, `fuzz-seconds: 1800`, `sanitizer: address`. Batch mode uploads the merged corpus artifact; the next batch and PR runs download it (acceptance: "next run starts from it").
- Same `if: failure()` crash upload.

### 4b. Add `.github/workflows/clusterfuzzlite-build.yml`
- `on: push: branches: [main]`; `permissions: contents: read`; single `build` job: `build_fuzzers` with `language: rust`, `sanitizer: address`, `upload-build: true`. Pin by SHA like the others. Mind artifact quota (CFL warns large builds can exceed free quotas): check the artifact size in the first run and, if large, set a short retention via the action's inputs or drop it in favour of the documented trade-off.

### 5. Trim `.github/workflows/fuzz.yml`
- Delete `fuzz-smoke` and the `parse` steps from the deep job; drop the `pull_request` trigger; rename job to `differential`; keep `workflow_dispatch` + `schedule 0 3 * * *`; keep nightly toolchain, cargo-fuzz install, `cargo build --release`, and `cargo +nightly fuzz run differential --sanitizer none -- -max_total_time=300 -timeout=30 fuzz/corpus/differential`, crash artifact upload, `timeout-minutes: 60`.
- Rewrite the header comment (lines 1-11): it currently justifies replay-only PR gating; replace with a two-line pointer: parse fuzzing moved to the ClusterFuzzLite workflows; this one hosts `differential` because it needs node and `target/release/jsse`. Keep comment density low per user style.
- `concurrency` group can stay.

### 6. Documentation
- `README.md` "## Fuzzing" (173-194): replace the CI paragraph with a table "target | where | sanitizer | cadence": `parse` | ClusterFuzzLite PR (120 s, blocking) + daily batch (30 min) | ASan | per PR / daily; `differential` | `.github/workflows/fuzz.yml` | none | nightly. Explain corpus persistence (batch artifact), that discovered corpus is not committed, and the triage pointer.
- `.claude/skills/fuzzing/SKILL.md`: update the description/intro targets; replace the "exactly what CI's `fuzz-smoke` job runs" sentence and the "nightly `fuzz-deep` gives each target 5 minutes" sentence; add a "Triaging a CFL finding" subsection: open the failed `ClusterFuzzLite PR`/`batch` run, download the crash artifact (or copy the testcase from the log), reproduce locally with `cargo +nightly fuzz run parse <file>` (ASan is cargo-fuzz's default sanitizer; use `--sanitizer none` only for speed), `cargo +nightly fuzz tmin`, then the existing rule: file an issue, don't fix inside the tooling PR. Also add how to fetch the batch corpus artifact (`gh run download`) to seed a local session, and note a PR failure from a latent bug unrelated to the diff is expected possible (the ADR's original flakiness worry) - file it, then rerun.
- New ADR `docs/adr/2026-10-23-HHMM-clusterfuzzlite-fuzzing.md`: decisions (CFL over hand-rolled workflow; ASan; artifact-stored corpus; blocking 120 s PR gate accepting latent-bug flake risk; `differential` stays outside CFL because it needs node + release binary; only `parse` in CFL). Consequences/follow-ups.
- `docs/adr/0004-...md`: add a short "Update" note at the ASan bullet (~line 139) and near the fuzz-smoke discussion (~line 114) pointing at the new ADR; do not rewrite history.
- `CONTEXT.md`: no change (no new domain vocabulary; "Divergence Tier" untouched).

## TDD slices
CI/config work has no unit tests; slices are verify-then-proceed steps, each one commit:
1. **Rename sync** (step 1). Check: `cargo fuzz build parse --sanitizer none` and `cargo +nightly fuzz run parse --sanitizer none -- -runs=0 fuzz/corpus/parse` pass (the seed-replay that `fuzz-smoke` used to do).
2. **CFL build config** (step 2). Check: `bash -n` + `shellcheck .clusterfuzzlite/build.sh`; then a local `docker build -f .clusterfuzzlite/Dockerfile .` and `docker run -v "$PWD/out:/out" -e SANITIZER=address -e FUZZING_LANGUAGE=rust -e FUZZING_ENGINE=libfuzzer -e ARCHITECTURE=x86_64 <image> compile` (the image's `compile` entrypoint sets `$OUT` and the sanitizer flags; do not invoke `build.sh` bare), checking `$OUT/parse` and `parse_seed_corpus.zip` appear. Use `CARGO_BUILD_JOBS=4`; if the box's memory budget is tight or the build takes >30 min, skip and rely on the PR run (note this in the PR body).
3. **Local ASan sanity run**: run the built `$OUT/parse` with the seed corpus (`-runs=0`) then ~3 min of mutation (`-max_total_time=180`). Goal: find out *before* merge whether the gate would be red on arrival (ASan stack-overflow in deep-nesting seeds on the 128 MiB engine stack, rss_limit, latent parser panics). Any real crash: minimize, `gh issue create` per `SKILL.md` rules (no engine fix in this PR), and state it in the PR body. A false positive from ASan frame inflation (stack overflow at parse depth limit) is NOT worked around by editing engine code here; file it.
4. **PR + batch workflows** (steps 3-4). Check: `actionlint .github/workflows/clusterfuzzlite-*.yml .github/workflows/fuzz.yml`; `uvx zizmor@1.26.1 .github/workflows/` (same version as `workflow-lint.yml`). The PR's own CFL check run is the acceptance test for "PR runs fuzz with ASan, finishes within the cap": confirm the log shows ASan instrumentation, a 120 s fuzz phase, and a green result.
5. **Trim `fuzz.yml`** (step 5). Check: actionlint/zizmor; PR no longer has a `Fuzz` check; `differential` job content unchanged (diff review).
6. **Docs + ADR** (step 6). Check: `grep -rn "fuzz-smoke\|fuzz-deep\|parse_roundtrip" . --exclude-dir=target --exclude-dir=.git --exclude-dir=test262 --exclude-dir=spec` returns nothing stale; the repo's pre-commit hooks (typos, end-of-file) pass on the staged files (`./scripts/ensure-hooks.sh` first).

## Test surface
- No test262 involvement; none of `test262/test/...` is relevant. No `test262-extra/` or `tests/` additions (no engine behavior change).
- Gates: `actionlint`, `zizmor` (as in `workflow-lint.yml`), `shellcheck`, pre-commit hooks, the PR's CFL check run, `cargo +nightly fuzz run parse -- -runs=0` replay, and the repo's `build` CI job (should be unaffected).
- Post-merge verification (schedule and `workflow_dispatch` only run from the default branch, so it cannot be exercised before merge; the orchestrator squash-merges unattended, so nobody will do this by hand). The implementation stage opens a follow-up issue via `gh issue create` (label `agent-ready`) titled e.g. "ci(fuzz): verify ClusterFuzzLite batch corpus round-trip", with exact steps: `gh workflow run clusterfuzzlite-batch.yml` twice (wait for the first to finish), then `gh run view <id> --log | grep -i corpus` on the second to confirm it loaded the first run's corpus, and `gh run list`/`gh api repos/pmatos/jsse/actions/artifacts` to confirm the corpus artifact; then check the next PR's CFL job downloads it. Link it from the PR body. This validates acceptance criterion 2.
- `differential` still nightly: verify by diffing the job against the old `fuzz-deep` differential steps and (post-merge) the next scheduled `Fuzz` run.

## Regression risk
- `test262-pass.txt`: untouched; no engine code changes, no `--update-baseline`.
- Shared machinery: none in the interpreter. Touchpoints are CI only. `src/lib.rs` is read-only except the doc comment from the rename (if the fallback rename is needed).
- Gate-red-on-arrival: a blocking mutation-fuzzing PR job can fail PRs for latent bugs unrelated to the diff (the original reason ADR 0004 made the PR gate replay-only; #597 was exactly this and is now closed). Mitigations: step-3 pre-merge local run, the continuous-build workflow (non-novel crashes are not reported), code-change mode starts from the stored corpus plus seeds, docs say how to triage and rerun; not a required check until the owner opts in.
- Hosted-runner cost/time: ASan cargo build of the icu/jiff-heavy tree may take 10-20 min per PR run. Mitigation: `timeout-minutes`, concurrency cancel; follow-up could cache the build (CFL supports cached builds via `upload-build` on main pushes - out of scope).
- Corpus artifact expiry (90 days) silently resets the corpus to the committed seeds; acceptable and documented.
- CFL action is `v1` mutable tag; pinned by SHA, bumped by Dependabot's github-actions ecosystem (already configured in `.github/dependabot.yml`). The Docker base image `:v1` is not Dependabot-tracked; accepted.
- Nightly Rust drift in the builder image (it ships latest nightly): could break the build on a nightly regression independent of the repo; rerun/hotfix as needed.
- Fork PRs get a read-only token: corpus download may fail there; external PRs are not a triage surface for this repo, so ignored.

## Out of scope
- Execute target, step budget, `arbitrary` AST generator, typed-array/Proxy/GC focused targets (other children of #1168).
- CFL `prune` and `coverage` modes, `storage-repo` with a PAT, SARIF upload, build caching beyond the continuous-build artifact, dictionaries (`-dict`), making the CFL check a *required* status check (repo-settings, owner action).
- Persisting the `differential` corpus; ASan for `differential`; any engine fix for crashes the fuzzer finds (file separately per `SKILL.md`).
- Wiring `fuzz/` into `scripts/lint.sh`; any refactor of `fuzz/src/lib.rs` or the `differential` target.
