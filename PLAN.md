# Plan: issue #348 — Align PR-title gate with commitlint rules

## 1. Problem restated

`.github/workflows/lint-pr-title.yml` (via `amannn/action-semantic-pull-request@v6.1.1`
with zero config inputs) only checks that a PR title starts with a valid
Conventional Commits `type:` prefix. `commitlint.config.cjs` — which
`.github/workflows/commitlint.yml` runs against every push to `main` — extends
`@commitlint/config-conventional`, which additionally enforces `subject-case`
(no sentence/start/pascal/upper case), `subject-full-stop` (no trailing `.`),
and `header-max-length` (100 chars). Since this repo squash-merges PRs using
the PR title as the commit subject, a title like `fix: Correct typo` passes
the PR gate, merges, and only then fails `commitlint.yml` on `main` — a
required check turns red *after* merge, on a commit nobody can amend by
re-running CI.

The issue's "why deferred" reasoning is now stale: it assumed
`lint-pr-title.yml` and `commitlint.config.cjs` didn't exist on `main` yet.
Commits `a8dd684` ("ci: enforce Conventional Commits (PR titles +
commitlint)") and `3384938` ("ci: harden new workflows against zizmor
findings") landed both on `main` before this branch was cut, so the
inconsistency is live now, not merely prospective.

## 2. Spec basis

N/A: no JavaScript behavior change — this is a GitHub Actions CI workflow
fix under `.github/`; nothing in `spec/` governs repository tooling.

## 3. Files to touch

- `.github/workflows/lint-pr-title.yml` — replace the `amannn` action with a
  base-ref checkout + commitlint CLI run against the PR title (and, per §4
  slice 3, the PR number suffix GitHub's squash-merge appends).
- `.github/zizmor.yml` — update/add policy comments and ignore entries for
  the new workflow shape (`dangerous-triggers`, `adhoc-packages`).

No other file references `lint-pr-title.yml` or `amannn` (checked via
`grep -rn "amannn\|lint-pr-title\|action-semantic-pull-request"` across
`*.md`/`*.yml`/`*.yaml`, excluding the `spec/`/`test262/` submodules) — no
`docs/` or `README.md` updates are needed.

**A prior attempt already left an uncommitted draft of this change in this
worktree** (`git status` shows both files modified, nothing committed). It
implements exactly the issue's "suggested direction" and I've verified it
mechanically (see §5). It is correct except for one gap (§4 slice 3, the
squash-suffix length check), which is small enough to patch in rather than
redo from scratch. The implementation stage should adopt it as the starting
point rather than re-deriving the approach. Final target contents for both
files, in full, follow so the plan is self-contained even if the worktree is
reset before the implementation stage runs:

### `.github/workflows/lint-pr-title.yml` (target)

```yaml
# Enforces that PR titles follow the same Conventional Commits rules as pushes.
# https://www.conventionalcommits.org/  |  https://commitlint.js.org/
name: Lint PR title

on:
  pull_request_target:
    types:
      - opened
      - edited
      - reopened
      - synchronize

permissions:
  contents: read

jobs:
  validate:
    name: Validate PR title
    runs-on: ubuntu-latest
    steps:
      # pull_request_target checks out the trusted base branch by default. Never
      # override this with the pull request's head ref: commitlint loads and
      # executes commitlint.config.cjs from the checkout.
      - uses: actions/checkout@v7.0.0
        with:
          persist-credentials: false
      - uses: actions/setup-node@v7.0.0
        with:
          node-version: lts/*
      - name: Install commitlint
        run: npm install --no-save @commitlint/cli@21 @commitlint/config-conventional@21
      - name: Lint PR title
        env:
          PR_TITLE: ${{ github.event.pull_request.title }}
          PR_NUMBER: ${{ github.event.pull_request.number }}
        # GitHub's squash-merge appends " (#<number>)" to the PR title to form
        # the commit subject; lint the string that will actually land on main,
        # not the bare title, or a too-long title still slips past this gate.
        run: printf '%s (#%s)\n' "$PR_TITLE" "$PR_NUMBER" | npx --no-install commitlint --config commitlint.config.cjs --verbose
```

(Diff from the current worktree draft: add `PR_NUMBER` to `env:` and change
the `printf` format/args on the last line. Everything else — the base-ref-only
checkout, `persist-credentials: false`, `contents: read`, passing the title
via `env:` instead of `run:` interpolation — is unchanged from the draft and
is correct as-is.)

### `.github/zizmor.yml` (target)

Keep the draft's rewritten `dangerous-triggers` and `adhoc-packages` comments
verbatim (they already accurately describe the base-ref-only checkout, the
`env:`/stdin-only handling of untrusted input, and the ad-hoc npm install
rationale) and keep `lint-pr-title.yml` listed under both ignore blocks. No
further changes needed there for the `PR_NUMBER` addition — it's still passed
via `env:`, so the existing "never interpolated into shell code" rationale
still holds.

## 4. TDD slices

This is a CI workflow, not engine code, so "red-green" here means
lint-then-verify against `actionlint`/`zizmor`/a local `commitlint` dry-run
rather than `cargo test`. Each slice is a reviewable, independently-verifiable
step.

1. **Red:** Run `printf 'fix: Correct typo\n' | npx commitlint --config commitlint.config.cjs` in a scratch dir with `@commitlint/cli@21` + `@commitlint/config-conventional@21` installed — confirm it exits 1 with `subject-case`. This is the exact example from the issue and from PR #337's review comment; it's the case the current `amannn`-based gate wrongly accepts. (Already reproduced during planning — see §5.)
2. **Green:** Replace the `amannn` step in `lint-pr-title.yml` with the base-ref checkout + `npm install` + commitlint-via-stdin steps shown in §3, reading the title from `PR_TITLE` (`env:`, never `run:` interpolation). Verify locally: `actionlint .github/workflows/lint-pr-title.yml` and `uvx zizmor@1.26.1 .github/workflows/lint-pr-title.yml --no-online-audits` both report no findings (both already pass clean against the current worktree draft, confirmed during planning).
3. **Red→Green (squash-suffix length):** Reproduce first: a title of 98 chars (`fix(engine): ` + 85 `a`s) passes `commitlint` bare but the same string with GitHub's squash suffix appended (`... (#374)`, 105 chars) fails `header-max-length` (reproduced during planning, see §5). Fix: add `PR_NUMBER: ${{ github.event.pull_request.number }}` to the step's `env:` and change the `run:` line to `printf '%s (#%s)\n' "$PR_TITLE" "$PR_NUMBER" | npx --no-install commitlint --config commitlint.config.cjs --verbose`, so the linted string matches what will actually land in the squash commit. Verify: the 98-char-title / 105-char-header case now fails locally in the same scratch-dir setup as slice 1, and a title that stays under 100 chars including the `(#N)` suffix still passes.
4. **Update `.github/zizmor.yml`:** carry the draft's rewritten `dangerous-triggers` and `adhoc-packages` comments and ignore-list entries for `lint-pr-title.yml` (§3). Verify: `uvx zizmor@1.26.1 .github/workflows/` (whole directory, not just the one file) still reports no findings, so no other workflow's zizmor posture regressed.
5. **PR title and body:** open the PR with a title that itself passes the *old* gate pre-merge (type prefix only — the new gate can't validate itself, see §5) and would also pass the *new* gate post-merge: lowercase subject, no trailing period, under 100 chars including the `(#N)` GitHub will append (e.g. `ci: lint PR titles with commitlint`). State explicitly in the PR body that "Validate PR title" passing on this PR does **not** exercise the new commitlint-based check (see §5) — only the next PR opened after this merges will.

## 5. Test surface

This is CI tooling, not engine code — no `test262/` or `test262-extra/`
directory exercises it. The actual gates:

- `actionlint .github/workflows/lint-pr-title.yml` — YAML/expression/shell
  syntax check. Confirmed clean against the current worktree draft during
  planning (binary fetched from the pinned release used by
  `.github/workflows/workflow-lint.yml`, v1.7.7).
- `uvx zizmor@1.26.1 .github/workflows/` — the security audit
  `workflow-lint.yml` runs on every PR. Confirmed clean (0 findings, 2
  ignored, 1 suppressed) against the draft during planning.
- A local `commitlint` dry-run (scratch dir, `npm install --no-save
  @commitlint/cli@21 @commitlint/config-conventional@21`, then `printf
  '<title>\n' | npx --no-install commitlint --config commitlint.config.cjs
  --verbose`) — confirms the rule set actually rejects `fix: Correct typo`
  (exit 1, `subject-case`) and accepts `fix: correct typo` (exit 0), and
  separately confirms the squash-suffix gap in slice 3 (98-char bare title
  passes, same title + ` (#374)` suffix at 105 chars fails
  `header-max-length`). All three confirmed during planning; the
  implementation stage should re-run them against its actual final diff
  before opening the PR, since this is the only way to validate the change
  pre-merge.
- **What cannot be tested pre-merge, and must be said in the PR body:**
  `pull_request_target` runs the workflow file version from the *base*
  branch (`main`), not the PR's head. The implementing PR will still run
  the *old* `amannn`-based "Validate PR title" check against its own title,
  regardless of what the PR's diff changes the file to. The new
  commitlint-based gate only starts running on the *next* PR opened after
  this one merges. Do not let a green "Validate PR title" check on this PR
  be read as evidence the new gate works — that's exactly what the local
  `actionlint`/`zizmor`/`commitlint` dry-runs above are for.

## 6. Regression risk

- **Scope is fully contained to `.github/`.** No engine code
  (`src/lexer.rs`, `src/parser/`, `src/interpreter/`) is touched, so none of
  the tree-walker hot paths, the property MOP, GC rooting, the `ObjectKind`
  exhaustive matches, the bytecode fast path, or the Node-compat library
  harnesses are in play. `test262-pass.txt` cannot move.
- **Availability/flakiness risk, not correctness risk:** the new step adds an
  `npm install` of `@commitlint/cli@21`/`@commitlint/config-conventional@21`
  on every PR title change (same package/version already installed by
  `commitlint.yml` on every push to `main`, so no new supply-chain surface —
  `.github/zizmor.yml`'s existing `adhoc-packages` rationale for
  `commitlint.yml` extends verbatim to `lint-pr-title.yml`). If npm registry
  is briefly unavailable, PR-title validation fails closed (blocks merge)
  rather than silently passing — acceptable, matches `commitlint.yml`'s
  existing behavior on push.
- **Security-sensitive surface (the reason the issue was deferred in the
  first place):** `pull_request_target` grants a real `GITHUB_TOKEN` even on
  fork PRs. The draft's checkout leaves `ref:` unset (so GitHub checks out
  the trusted base branch, never the PR head — verified by reading the
  final YAML, no `ref:` key present), passes the title via `env:` (not
  `run:` interpolation — the classic template-injection vector zizmor's
  `template-injection` audit exists to catch), sets
  `persist-credentials: false`, and scopes `permissions:` down to
  `contents: read` (down from the current `pull-requests: read`, since the
  new step no longer calls the GitHub API at all). `zizmor` confirms no
  findings against this shape (§5).
- **Squash-suffix length check (slice 3) is the one place regression could
  hide:** if the implementation stage drops the `(#N)` suffix (e.g. "simplifies"
  the `printf` back to just `$PR_TITLE`), the exact bug the issue exists to
  close — a too-long title that becomes a too-long squash-commit header —
  re-opens silently, because both `actionlint` and `zizmor` are blind to it
  (it's a commitlint *rule* outcome, not a workflow syntax/security issue).
  Only the commitlint dry-run in slice 3 catches this; it must not be
  skipped.

## 7. Out of scope

- **`body-max-line-length` (also 100, error-level in
  `@commitlint/config-conventional`) can still fail `commitlint.yml` on
  `main` via the squash-merge body, independent of the title.** This PR only
  aligns the *title*/header check (what `lint-pr-title.yml` can see before
  merge); a PR body that produces an over-long commit body line is a
  pre-existing gap, not introduced or fixed here. Worth its own issue if it
  ever bites in practice.
- **The original reviewer's two mutually exclusive remedies** ("either add
  matching title rules here... or relax the commitlint config...") — this
  plan picks the issue's own "suggested direction" (reuse
  `commitlint.config.cjs` as the single source of truth for both gates) over
  either alternative (hand-porting individual rules into `amannn`'s config
  inputs, or weakening `commitlint.config.cjs`'s already-shipped
  `config-conventional` ruleset). Not revisited here.
- **No refactor of `commitlint.yml` or `commitlint.config.cjs` themselves.**
  Both already exist, already work correctly for pushes to `main`, and are
  out of scope — this issue only closes the gap on the PR-title side.
- **No change to `unpinned-uses`/`artipacked` zizmor policy blocks** — only
  the `dangerous-triggers` and `adhoc-packages` entries for
  `lint-pr-title.yml` are touched.
