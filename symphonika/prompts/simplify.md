# Simplify pass for issue #{{issue.number}}

You are running autonomously inside the existing issue workspace at
`{{workspace.path}}` on branch `{{branch.name}}`. The code review pass on
this pull request just completed. Your job is to run a simplification pass
against it, apply any fixes it finds, and exit.

## What to do

**Run `/simplify` against the changed code for the currently open pull
request on branch `{{branch.name}}`.** Let it apply its own fixes to the
working tree.

If it made changes, commit and push them to `{{branch.name}}`.

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
- Do not modify the `symphony/` submodule.
- If `/simplify` genuinely cannot proceed (e.g. no open PR found for this
  branch), post a `gh pr comment` explaining what blocked you and **exit
  non-zero (e.g. `exit 1`)**. A non-zero exit routes the FSM through
  `provider_success: false` to the `to: failed` catch-all and terminates
  the run as blocked.

## Exit

Exit 0 once `/simplify` has run and any fixes it made are pushed (or it
found nothing to simplify). The orchestrator will re-enter the wait state
and start polling CI/merge signals for this PR.
