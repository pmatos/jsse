# Resolve merge conflicts for issue #{{issue.number}}

You are running autonomously inside the existing issue workspace at
`{{workspace.path}}` on branch `{{branch.name}}`. The open pull request
from this branch has merge conflicts against `main` and cannot be merged.
Your job is to resolve those conflicts, push, and exit.

## What to do

**Use the pm-autofix-pr skill to fix the conflicts on the pull request for
branch `{{branch.name}}`** — its trigger set includes "fix the build" and
"iterate on pr", which covers conflict resolution.

Concretely:

1. `git fetch origin main`.
2. Rebase or merge `origin/main` into `{{branch.name}}`. Prefer rebase
   unless the branch history would be destructive to reviewers.
3. Resolve each conflict by preserving the intent of both changes. When
   resolution requires a judgment call, choose the most defensible option
   and document it in a `gh pr comment` after the push.
4. Run the local quality gate — a green gate is the proof that your
   resolution did not silently break behavior.
5. Force-push the rebased branch (`git push --force-with-lease`) only if
   you rebased. If you merged, push normally.

Discover the PR with `gh pr list --head {{branch.name}} --state open` if
you need the PR number. Stay on branch `{{branch.name}}`. Do not open a
second PR.

## Constraints

- Unattended run; no operator will respond mid-run.
- Use the local `gh` CLI for every GitHub mutation. Do **not** call the
  GitHub MCP connector tools.
- Do not modify operational labels in the `sym:*` namespace.
- Do not modify `symphonika/` — that is this pipeline's own contract, and
  editing it mid-run changes the rules you are running under. Do not
  modify the `spec/` or `test262/` submodules either.
- If conflicts are genuinely unresolvable without a product decision,
  post a `gh pr comment` describing what blocked you and **exit non-zero
  (e.g. `exit 1`)**. A non-zero exit routes the FSM through
  `provider_success: false` to the `to: failed` catch-all and terminates
  the run as `blocked`. Exiting 0 here would set `provider_success: true`,
  return the FSM to `wait_for_pr`, which would observe the same
  `mergeable: false` signal and route straight back into this state —
  an infinite loop. Do not self-apply `sym:human-needed`.

## Exit

Exit 0 once the rebase/merge is clean and pushed. The orchestrator will
re-check `mergeable` on the next tick and route accordingly.
