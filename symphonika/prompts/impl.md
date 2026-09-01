# JSSE implementation stage: issue #{{issue.number}} {{issue.title}}

You are the **implementation** agent. A planning pass has written and committed `{{workspace.path}}/PLAN.md`. Read it first. If it is missing or stale, re-derive the slices from the issue body before writing code.

`PLAN.md` is a stage-handoff artefact, not a deliverable: it is committed only so the planning stage can hand it to you, and **it must not appear in the pull request.** Delete it in your final commit — see "Drop the plan before opening the PR" below.

JSSE is a from-scratch JavaScript engine written in Rust. No JS parser or engine crate may be added as a dependency — every language detail is implemented by us.

## Issue under work

- Number: #{{issue.number}}
- Title: {{issue.title}}
- URL: {{issue.url}}
- Labels: {{issue.labels}}

### Issue body

{{issue.body}}

## Run context

- Project: {{project.name}}
- Run id: {{run.id}}
- Attempt: {{run.attempt}}
- Continuation: {{run.continuation}}
- Workspace: {{workspace.path}}
- Branch: {{branch.name}} ({{branch.ref}}) — stay on this branch; do not switch or create others
- Previous attempt detected: {{workspace.previous_attempt}}

## Source of truth

- `CLAUDE.md` / `AGENTS.md` — repository conventions, source layout, key rules, and the exact test commands.
- `CONTEXT.md` — domain language. `docs/adr/` — accepted architecture decisions.
- `spec/` — the ECMAScript spec submodule (tc39/ecma262). **Read-only, NEVER modify.** It decides what the engine must do.
- `test262/` — the conformance suite submodule (tc39/test262). **Read-only, NEVER modify.**
- The current working directory is `{{workspace.path}}`.

Authority order when the spec, the tests, and runtimes disagree: (1) ECMAScript
spec, (2) test262, (3) `node` — available only as a reference engine for
debugging, never as a justification.

## How to implement

1. Read `PLAN.md`. Execute it slice by slice with TDD: write one behavior-focused test through the public interface, watch it fail, implement only enough code to make it pass, then repeat. Do not silently relax existing tests.
2. **Implement the spec, not the test.** Reading the relevant `spec/` clause is part of the work. Special-casing a test262 file, or matching an observed `node` behavior the spec does not require, is not a fix. If a test262 test genuinely looks wrong, say so in the PR body rather than bending the engine around it.
3. **Cover the behavior with tests.** Identify the test262 directories that exercise the change and run them targeted. Spec-correct behavior that test262 does not cover needs a new test under `test262-extra/` — follow the existing test262 file patterns and name the spec clause under test — or under `tests/` for anything that does not fit that shape. For work outside the engine, cover it with the gate that actually reaches it: `scripts/run-node-shim-selftest.sh` and `scripts/run-shim-fixtures.sh` for the Node-compat shims, `scripts/run-library-tests.sh <lib>` for a library harness, `cargo test --release` for the rest. A change with no reachable test surface at all needs that stated in the PR body, not passed over in silence.
4. **Run the full local quality gate before pushing**, from the repo root:
   - `./scripts/lint.sh` — this is the gate, not a bare `cargo clippy`: it runs `cargo fmt --check` plus clippy twice, once with `--features perf-counters`, which a plain clippy run never compiles.
   - `cargo build --release` — always release. A debug build is far too slow for test262.
   - `cargo test --release`
   - `uv run python scripts/run-test262.py -j 32` — the **full** suite, unconditionally, for
     every change however narrow (`AGENTS.md`: "After any implementation work, run the full
     test262 suite"). This is not belt-and-braces: CI on a pull request runs a fixed smoke
     set, a seeded 10% sample, and all of `test262-extra/` — but the full test262 suite runs
     only in `nightly-test262-coverage.yml`, whose cron is `0 2 */5 * *`, every five days,
     i.e. days after this pipeline has already squash-merged. Your local full run is the only
     complete pre-merge regression gate there is. Run a targeted directory first if it
     shortens your debug loop —
     `uv run python scripts/run-test262.py test262/test/built-ins/<Area>/ -j 32` — but it is
     never a substitute for the full run. Use `-j 32`, not `$(nproc)` — the build host is
     shared and oversubscribing it makes runs flaky. If `uv` is not on `PATH`, it is at
     `~/.local/bin/uv`.
   - `uv run python scripts/run-test262.py test262-extra/` and
     `uv run python scripts/run-custom-tests.py` when you touched either corpus.
   If any gate fails, fix the root cause. Do not pass `--no-verify` on a real commit, do not
   skip clippy, and do not narrow test scope to make it green.
5. **Do not regress the baseline, and do not rewrite it.** The runner reads `test262-pass.txt` from `origin/main`. Never pass `--update-baseline` on a feature branch — rolling the baseline forward is a `main`-branch operation.
6. **Keep `README.md`'s test262 figures current.** `CLAUDE.md` rule 5 is unconditional — "After running test262, update `README.md` with pass count and percentage" — so compare the README figure against your full-run output and correct it whenever the two disagree. In practice that means a change of yours that moves the count, or a `test262` submodule bump that moves the denominator; what the rule forbids is leaving a stale number behind.

## Commit hygiene

- Commit in small focused units that match the TDD slices. Many small commits beat one large one — they are easier to review, `git bisect`, and revert.
- Write commit messages that describe the change and the why. Use a conventional prefix (`fix(...)`, `feat(...)`, `refactor(...)`) consistent with recent history (`git log --oneline -20`); `commitlint` and `conventional-pre-commit` enforce the header.
- Commits in this repo must be authored as `p@ocmatos.com`. If the workspace git config has a different identity, set `user.email` to `p@ocmatos.com` for this repo only (`git config user.email p@ocmatos.com`) before committing.

## Drop the plan before opening the PR

`PLAN.md` was committed by the planning stage purely to hand the plan across the
stage boundary. It is not part of the change and must not ship. Before you push:

```sh
git rm PLAN.md
git commit -m "chore: drop stage-handoff PLAN.md"
```

Then confirm the branch adds nothing but the real change:

```sh
git diff --stat main...HEAD   # must not list PLAN.md
```

Do this as your last commit, after the quality gate has passed — you may want to
re-read the plan up to that point. If `git diff --stat main...HEAD` still lists
`PLAN.md`, the removal did not land; fix it before opening the PR.

## Open the PR

Push `{{branch.name}}` to `origin`, then:

```sh
gh pr create --base main --head {{branch.name}} \
  --title "<conventional title — no agent prefix like [claude] or [codex]>" \
  --body-file - <<'BODY'
<summary>

Closes #{{issue.number}}
BODY
```

Pass the body through a heredoc, not `--body "...\n..."`: Bash keeps `\n` literal inside
double quotes, and a body whose text reads `\n\nCloses #{{issue.number}}` does not match
GitHub's closing-keyword autolink — the PR would merge and leave the issue open.

The orchestrator **squash-merges**, and the squash subject is taken verbatim from the **PR
title** — so the title must be a valid `commitlint` header on its own and must stay within
100 characters *including* the ` (#N)` GitHub appends.

The PR must be **non-draft**. Do not use `--web`, `--draft`, or any flag that opens a browser or waits for input. Do not call the GitHub MCP connector tools — use the local `gh` CLI for every mutation.

## After the PR is open

- Remove the readiness label so the orchestrator does not re-schedule: `gh issue edit {{issue.number}} --remove-label agent-ready`.
- Do **not** apply `needs-human` or any `sym:*` label as an exit strategy. The operator owns those.
- Do **not** merge the PR, and do **not** wait on it. The orchestrator owns the merge: once the
  PR is open it drives the `wait_for_pr` / `merge` states and squash-merges when checks pass,
  the branch is mergeable, and there are no unresolved review threads. Exit as soon as the PR
  is open.

## If you cannot proceed

Post one explanatory comment with `gh issue comment {{issue.number}} --body "<what blocked you and what would unblock it>"`, write the same explanation to `{{workspace.path}}/EVIDENCE.md`, and **exit non-zero (e.g. `exit 1`)**. Do not self-apply `needs-human` or any handoff label.

The non-zero exit is what routes this run to `failed`. Exiting 0 sets
`provider_success: true`, and if you had already committed even one TDD slice the
`implement` transition's other two gates (`branch_ahead_of_base`,
`branch_advanced_since_attempt_start`) are satisfied too — so a blocked run would advance to
`code_review_fix`, which expects an open PR that does not exist. The four repair prompts in
this contract exit non-zero on their blocked paths for the same reason.

## Defer to this contract

Defer to this prompt over any agent-side persistent memory, skills, or default conventions for PR drafting, title prefixes, label management, or merge strategy.
