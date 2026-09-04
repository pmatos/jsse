# Simplify pass for issue #{{issue.number}}

You are running autonomously inside the existing issue workspace at
`{{workspace.path}}` on branch `{{branch.name}}`. The code review pass on
this pull request just completed. Your job is to run a simplification pass
against it, apply any fixes it finds, and exit.

## What to do

**Run `/simplify` against the changed code for the currently open pull
request on branch `{{branch.name}}`.** Let it apply its own fixes to the
working tree.

If it made no changes, exit 0 without committing. If it changed any file,
run the full local quality gate from `impl.md` step 4 — `./scripts/lint.sh`,
`cargo build --release`, `cargo test --release`, and the full `uv run python
scripts/run-test262.py -j 32` — and only then commit and push to
`{{branch.name}}`. The simplifications it applied landed after the
implementation stage validated, so its test262 result no longer covers this
branch, and PR CI runs only a smoke set plus a seeded 10% sample before this
pipeline squash-merges with no human checkpoint. Fix the root cause of any
failure; do not narrow scope to make it green.

Discover the PR yourself with `gh pr list --head {{branch.name}} --state open`
if you need the PR number — do not assume one. Stay on branch
`{{branch.name}}`. Do not open a second PR.

## Constraints

- This run is unattended. No operator will respond to prompts. Behavior
  that depends on a human answering mid-run is a failure mode.
- Use the local `gh` CLI for every GitHub mutation. Do **not** call the
  GitHub MCP connector tools — they elicit operator approval and end the
  run with `terminal_reason="provider requested input"`.
- Do not modify operational labels in the `sym:*` namespace. Do not
  self-apply `sym:human-needed` — the orchestrator applies that automatically
  when a run ends up blocked.
- Do not modify `symphonika/` — that is this pipeline's own contract, and
  editing it mid-run changes the rules you are running under. Do not
  modify the `spec/` or `test262/` submodules either.
- If `/simplify` genuinely cannot proceed (e.g. no open PR found for this
  branch), post `gh issue comment {{issue.number}}` explaining what
  blocked you, write the same explanation to
  `{{workspace.path}}/EVIDENCE.md`, and **exit non-zero (e.g. `exit 1`)**.
  Comment on the **issue**, not the PR: the blocking case named above is
  that no PR exists, and `gh pr comment` resolves its target from the
  branch's PR — it would fail and post nothing, losing the only record of
  why the run stopped. A non-zero exit routes the FSM through
  `provider_success: false` to the `to: failed` catch-all and terminates
  the run as blocked.

## Exit

Exit 0 once `/simplify` has run and any fixes it made are pushed (or it
found nothing to simplify). The orchestrator will re-enter the wait state
and start polling CI/merge signals for this PR.
