# Plan: issue #347 — release.yml recovery path for post-tag publish/upload failures

## 1. Problem restated

`release.yml` runs `semantic-release`, which derives the next version, and
in its `prepare` phase runs (in plugin order) `@semantic-release/changelog`,
then `@semantic-release/exec`'s `prepareCmd` (`prepare.sh`, which bumps
`Cargo.toml`, builds the release binary, and stages
`release-upload/*.tar.gz` + `SHA256SUMS.txt`), then `@semantic-release/git`,
which commits `CHANGELOG.md`/`Cargo.toml`/`Cargo.lock` and creates+pushes the
`vX.Y.Z` tag on that commit. Only in the later `publish` phase does
`@semantic-release/github` create the GitHub Release. Per its source
(`lib/publish.js`), when assets are configured it **creates the release as a
draft, uploads assets to it, then PATCHes `draft: false`** — so a failure in
that phase can leave behind either no release at all, or a draft release
with some/all assets missing, or (rarer) a draft release with all assets
uploaded but never un-drafted.

`semantic-release` decides "is there anything new to release" purely from
git tags, not GitHub Release state, so once the tag is pushed, any later
run only ever considers commits *after* that tag. If the workflow is
rerun once new commits have landed on `main` in the meantime (the realistic
case — this is a weekly-cron/`workflow_dispatch` pipeline, so a rerun is
rarely on the exact same commit as the failed run), `semantic-release` will
cut the *next* version and simply orphan the broken tag forever; it will
never revisit it. A HEAD-only check ("is there a tag on the current
commit") therefore does not cover the case the issue describes — detection
has to look at the latest release tag regardless of where `HEAD` currently
is. The deferred PR #341 noted the old manual workflow had an explicit
same-tag retry (`gh release view "$TAG" && gh release upload ... --clobber`);
this issue is to give the semantic-release pipeline an equivalent. The
pipeline has since cut several real releases (v0.4.16 through v0.6.0),
satisfying the deferral's "prove itself on a real release" condition.

## 2. Spec basis

N/A: no JavaScript behavior change — this is a `.github/workflows/` CI
recovery mechanism; it touches no engine source, parser, or interpreter
code path.

## 3. Files to touch

- `.github/semantic-release/reconcile-release.sh` (new) — standalone,
  testable script containing the reconciliation logic. Accepts no
  arguments; discovers the latest `vX.Y.Z` tag itself.
- `.github/workflows/release.yml` — add a step running the new script
  **before** the existing `Run semantic-release` step (so a stale tag gets
  repaired before semantic-release potentially advances the latest tag to
  a new version), plus a one-line header comment noting the self-healing
  behavior.
- `scripts/test-reconcile-release.sh` (new) — self-test harness exercising
  the script's branches against a throwaway git repo with a stubbed `gh`
  and a stubbed `prepare.sh`, in the spirit of the existing self-verifying
  fixture scripts (`scripts/run-shim-fixtures.sh`).

No changes to `.github/semantic-release/prepare.sh` itself — it is reused
unmodified, invoked with the recovered version string from inside a
detached worktree checked out at the stale tag.

## 4. TDD slices

Each slice is exercised by `scripts/test-reconcile-release.sh`, which
creates a temp git repo (with a couple of tagged commits and, for the
HEAD-drift cases, extra untagged commits on top to model "the workflow was
rerun after more commits landed"), puts a stub `gh` script logging its
invocations and returning canned output selected by an env var ahead of the
real one on `PATH`, and points `PREPARE_SCRIPT` (an overridable env var read
by `reconcile-release.sh`, defaulting to the real
`.github/semantic-release/prepare.sh`) at a fast stub that just drops dummy
`release-upload/*.tar.gz` + `release-upload/SHA256SUMS.txt` files instead of
doing a real `cargo build --release`. Each test then runs
`reconcile-release.sh` and asserts on the stub `gh` call log.

1. **No release tags at all → no-op.** Test: fresh repo, no `v*` tags
   anywhere. Assert the script prints "nothing to reconcile" and the stub
   `gh` log is empty. Production code: the
   `git tag -l | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | sort -V | tail -n1`
   discovery and early `exit 0` when it's empty. Introduce `PREPARE_SCRIPT`
   here (unused by this branch, but defined) so later slices can rely on it.
2. **Latest tag has a fully-published release → no-op.** Test: tag
   `v1.2.3` (on an old commit, with newer untagged commits on top, to prove
   `HEAD` position doesn't matter); stub `gh release view` returns
   `isDraft: false` and both expected asset names. Assert the log shows the
   `release view` calls and no `create`/`upload`/`edit`/`prepare.sh`
   invocation.
3. **Latest tag has no GitHub Release yet → create path.** Test: stub
   `gh release view` exits non-zero ("release not found"). Assert:
   `git worktree add --detach <tmp> v1.2.3` happened, the stub
   `PREPARE_SCRIPT` was invoked with version `1.2.3`, then
   `gh release create v1.2.3 <worktree>/release-upload/*.tar.gz
   <worktree>/release-upload/SHA256SUMS.txt --title v1.2.3 --notes "..."`
   was called with `--notes` equal to `git log -1 --format=%b v1.2.3` in the
   worktree (the body semantic-release's `@semantic-release/git` committed,
   per `.releaserc.json`'s `message: "chore(release): ... \n\n${nextRelease.notes}"`).
4. **Latest tag has a draft release missing an asset → upload + un-draft
   path.** Test: stub `gh release view` succeeds, `isDraft: true`, returns
   only one of the two expected asset names. Assert `PREPARE_SCRIPT` ran,
   then `gh release upload v1.2.3 ... --clobber` and
   `gh release edit v1.2.3 --draft=false` were both called (not `create`).
5. **Latest tag has a published release missing an asset → upload path,
   no un-draft.** Test: stub `gh release view` succeeds, `isDraft: false`,
   missing one asset name. Assert `gh release upload ... --clobber` was
   called but `gh release edit --draft=false` was *not* (already published).
6. **Wire the workflow step.** Add the `Reconcile release assets` step to
   `release.yml` right after `setup-node` and before `Run semantic-release`,
   with `GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}` (the existing
   `contents: write` permission already covers `gh release
   view`/`create`/`upload`/`edit`). There is no way to execute the real
   Actions job locally; this slice is verified by
   `workflow-lint.yml`'s existing `actionlint`/`zizmor` gate (run locally,
   see slice list below) plus manual reading of the diff.

## 5. Test surface

Not applicable: `test262/` — this change has no JavaScript-observable
behavior, so no `test262/test/...` directory exercises it and nothing
belongs in `test262-extra/`.

Gates that actually cover this change:
- `scripts/test-reconcile-release.sh` (new; needs no network access or
  built binary since `prepare.sh` is stubbed via `PREPARE_SCRIPT`, and `gh`
  is stubbed) — covers the branches in slices 1-5.
- `actionlint` / `zizmor`, as invoked by `.github/workflows/workflow-lint.yml`
  — run locally the same way that workflow does (download the pinned
  `actionlint` release, `uvx zizmor@1.26.1 .github/workflows/`) to validate
  the new step's YAML/expression/shell syntax and that it doesn't introduce
  an injection or permissions finding.
- `shellcheck .github/semantic-release/reconcile-release.sh
  scripts/test-reconcile-release.sh` if available locally, matching the
  existing bash scripts' quality bar (`prepare.sh` is already
  shellcheck-clean).
- `cargo test --release` — unaffected by this change but run anyway per the
  project's standard gate, to confirm the PR touches nothing under `src/`.

## 6. Regression risk

None to `test262-pass.txt`: this change adds a CI script and a workflow
step, touching no file under `src/` — no tree-walker (`eval_expr`/
`exec_statement`), `property.rs` MOP, GC rooting/`gc_safepoint()`,
`ObjectKind` matches, bytecode fast path, or Node-compat library harness is
in the diff, so none of that shared machinery is at risk and the baseline
cannot move.

The operational risk is narrower and CI-only:
- A bug in tag discovery or the draft/asset check could cause a false
  no-op (a genuinely broken release stays broken) or an unwanted
  re-publish (recreating/re-uploading a tag that was actually fine). The
  strict `^v[0-9]+\.[0-9]+\.[0-9]+$` filter (no prerelease/build-metadata
  suffix) scopes this to release tags this pipeline itself produces, and
  `sort -V | tail -n1` always targets only the single latest one. The five
  TDD slices are the direct mitigation.
- The new step builds from a `git worktree add --detach <tmp> "$TAG"` and
  runs `prepare.sh` there unmodified — this is a **fresh, uncached**
  `cargo build --release --locked` (no attempt to share the primary
  checkout's `target/` via `CARGO_TARGET_DIR`, since `prepare.sh` uses
  paths relative to its own `target/release/jsse` and pointing
  `CARGO_TARGET_DIR` elsewhere would break that without a matching
  edit to `prepare.sh`, which is out of scope). This only runs on the
  rare reconciliation path (the common case exits after `gh release view`,
  before touching cargo at all) so the extra cold-build time is acceptable
  within the job's existing 45-minute timeout.
- The worktree is cleaned up via `trap ... EXIT` so a failed reconciliation
  doesn't leave a stray worktree registered against the checkout for the
  next step (`Run semantic-release`) to trip over.

## 7. Out of scope

- Rewriting `prepare.sh` or the asset-staging layout, or sharing the
  `target/` build cache between the primary checkout and the
  reconciliation worktree — reused/left unmodified.
- Handling multiple release tags pointing at different commits in ways
  other than "always reconcile only the single latest one" — older stale
  tags are assumed already resolved by a prior run, or are a case for
  manual intervention (the pre-existing `gh release create`/`upload
  --clobber` recovery path against an explicit tag still works for that).
- Retrying transient `semantic-release` failures that happen *before* the
  tag is pushed (e.g. `commit-analyzer`/`release-notes-generator` errors) —
  those are safe to simply rerun as-is, since no tag exists yet; out of
  scope per the issue, which is specifically about the post-tag window.
- A same-run self-heal (re-invoking the reconcile step immediately after
  `Run semantic-release` on a failure in that same job) — not needed to
  close the issue: since the step runs before `Run semantic-release` on
  *every* invocation of this workflow, a failure in this run's
  semantic-release step is caught and repaired by the pre-check on the
  very next run, which is the recovery path the issue asks for.
- Any change to `permissions:` in `release.yml` — the existing
  `contents: write` already covers `gh release create`/`upload`/`view`/`edit`.
- A general bats/shell-test framework for the repo — the one self-test
  script added here is scoped to this feature, not a new house convention.
