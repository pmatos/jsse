# Fuzz the parser with ClusterFuzzLite under ASan

Status: accepted (#1172, part of #1168); partly supersedes the "no sanitizer"
and "replay-only PR gate" decisions in ADR 0004.

## Context

ADR 0004 gated PRs on a corpus replay (`-runs=0`) with no sanitizer, and ran
new-mutation fuzzing nightly for five minutes per target. Nothing persisted the
inputs a nightly run discovered, so every run started from the committed seeds,
and nothing exercised a PR's change with mutation fuzzing.

## Decision

1. **`parse` runs in ClusterFuzzLite** (`.clusterfuzzlite/`, workflows
   `clusterfuzzlite-{pr,batch,build}.yml`), built with AddressSanitizer
   (the only sanitizer ClusterFuzzLite offers for Rust) and `--debug-assertions`.
2. **PRs run code-change fuzzing as a blocking check, capped at 120 s of
   fuzzing.** The ASan build is extra wall time and is bounded by the job's
   `timeout-minutes`. The check is not a required status check yet; making it
   one is a repository-settings decision.
3. **A daily batch run (30 min) stores the merged corpus** as a GitHub Actions
   artifact. The next batch run and PR runs download it. A `storage-repo` was
   not used because it needs a second repository and a personal access token.
   Artifacts expire with the repository's retention setting, after which
   fuzzing restarts from `fuzz/corpus/parse/`.
4. **A push-to-`main` build is uploaded** so ClusterFuzzLite can tell a crash a
   PR introduced from one already on `main`, and not report the latter. This is
   the main defence against the blocking gate failing on latent bugs, the
   reason ADR 0004 kept the PR gate replay-only.
5. **`differential` stays outside ClusterFuzzLite.** It needs `node` and a
   release `jsse`, neither available in the builder container, so it keeps its
   nightly job in `fuzz.yml`, which now contains only that job.
6. **The builder image installs current nightly.** The
   `base-builder-rust` image pins a nightly that predates edition 2024 and
   `let_chains`, so `.clusterfuzzlite/Dockerfile` installs the latest nightly.

## Consequences

- A PR can fail on a latent parser bug unrelated to its diff. The triage
  procedure is in `.claude/skills/fuzzing/SKILL.md`: file the bug, re-run.
- Nightly toolchain drift in the builder image can break the fuzz build
  independently of the repository.
- Follow-ups: corpus pruning and coverage reports (ClusterFuzzLite `prune` and
  `coverage` modes), a `storage-repo`, and further targets from #1168.
