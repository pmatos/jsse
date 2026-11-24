---
name: verify
description: >-
  This skill should be used before opening or updating a pull request, and
  when the user asks to "verify", "check my change", "run the checks", "is
  this ready for a PR", or "run the differential check". Builds jsse, runs a
  test262 subset plus test262-extra, runs the jsse-vs-node differential
  check, and reports one result block to paste into the PR body.
version: 0.1.0
---

# Verify

Run this before every `gh pr create` and after every push that changes engine
code. It answers one question with evidence: does the change still build,
still pass the test262 baseline, and still behave like node on the
differential corpus? CI runs more (clippy, unit tests, GC stress, the 10%
test262 sample); this is the fast local pass that catches most of it first.

## Run it

```bash
./scripts/ensure-hooks.sh          # required before any commit, see AGENTS.md
./scripts/verify.sh [test262 paths for the area you touched]
```

- Always pass the test262 directories for the feature you changed, e.g.
  `./scripts/verify.sh test262/test/built-ins/Number/prototype/toFixed/`. The
  default 2% sample (fixed seed, about a minute) will not see a narrow regression.
- Add `--full` when the change touches the parser, interpreter core or GC, or
  before marking a PR ready: it adds clippy and the unit tests.
- The default build uses the quick no-LTO configuration from AGENTS.md
  "Building". Use `--shipped` only when you need the real release binary.
- The first build takes several minutes: run it with a Bash timeout of at
  least 15 minutes, or in the background, and never rebuild while a test262
  run from another step is still in flight.

## Read the result

`scripts/verify.sh` prints `### verify: PASS` or `### verify: FAIL` followed
by a table of steps, and writes the same Markdown to
`target/verify/report.md`. Per-step logs are in `target/verify/*.log`.

- **FAIL** means a blocking step failed: fmt, build, test262-extra (zero
  failures allowed), or a test262 baseline regression. Fix it before opening
  the PR. Do not edit `test262-pass.txt` to make a regression go away.
- **Differential divergences are observe-only for now.** They never fail the
  run, but you must look at each one:
  1. If your change caused it, treat it as a bug in your change.
  2. If it was already there (check the same file on `origin/main`), mention
     it in the PR body and leave it for a separate issue or small PR.
  3. Only if the difference is correct for jsse (node is wrong or is showing
     host behaviour), add an entry to `tests/differential/approved.toml` with
     the fingerprint the runner printed and a reason. Cite the spec section.
  The authority order is the spec, then test262, then node (AGENTS.md rule 6),
  so a divergence is a question to answer, not proof that jsse is wrong.

## Report it

Paste `target/verify/report.md` into the PR body under a `## Verification`
heading. If you skipped `--full` or ran without test262 paths, say so in one
line, so the reviewer knows what was not checked.

## Extending the differential corpus

When you fix a bug that test262 did not catch, add a small program to
`tests/differential/` that would have caught it. See
[docs/agents/differential-testing.md](../../../docs/agents/differential-testing.md)
for the corpus rules.
