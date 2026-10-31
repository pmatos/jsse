# Triage rules for TypeScript-on-jsse failures

What to do when an oracle, the pass-list gate or the nightly job reports a difference between jsse `--node` and Node, or a regression against the recorded baseline. How to run them: [typescript-suite-local.md](typescript-suite-local.md). Architecture: [node-host.md](node-host.md). Issue tracker commands: [issue-tracker.md](issue-tracker.md); labels: [triage-labels.md](triage-labels.md); domain docs layout: [domain.md](domain.md).

## 1. Find the root cause first

A difference is never "a TypeScript problem". TypeScript is a large, correct JavaScript program that passes on Node, so a failure on jsse is one of:

| class | sign | fix lives in |
| --- | --- | --- |
| ECMAScript engine bug | the same script fails without `--node`; the spec says what is right | `src/` (parser, interpreter, bytecode VM, built-ins) |
| host (Node API) bug | only under `--node`; Node's behaviour is the oracle | `node-lib/`, `scripts/node-*.js` or the Rust floor, see [node-host.md](node-host.md) |
| resource limit | `RangeError: Maximum call stack size exceeded`, memory over the 6x budget, a timeout | call-depth limits, GC, performance; weigh against [the budget](library-tests-typescript-budget.md) |
| oracle or harness bug | the oracle's own comparison, normalisation or process handling | `scripts/` (its unit test first) |
| ordering or timing | passes alone, fails under load or differs between runs | find the unsynchronised assumption; never raise a timeout to hide it |

Reduce to a minimal script (`jsse --node -e` or a file; add `$262.gc()` or `JSSE_GC_STRESS=N` if a freed object is suspected, see AGENTS.md) and compare with `node`. The authority order is the ECMAScript spec, then test262, then node. Do not special-case a TypeScript file name, a test title or an output string.

## 2. Fix, with the tests that keep it fixed

- Engine bugs: a regression test in `test262-extra/` that follows the exact pattern of a test262 test (header with the spec section, `includes`, `flags`, harness asserts). Both execution engines must agree: run it plain and with `--no-bytecode` (CI runs both, and `JSSE_GC_STRESS=7`). `tests/` is for host-diagnostic and resource checks, not ECMAScript behaviour.
- Host bugs: a fixture in the area's `scripts/<area>-fixtures/` diffed against Node by the area's `run-node-<area>-oracle.sh`, and a case in `tests/node_<area>.rs`. Keep the default (non-`--node`) global environment unchanged and the test262 baseline unregressed: run the directory you touched with `uv run python scripts/run-test262.py test262/test/<dir>/` and let CI run the rest.
- Oracle bugs: a unit test in `scripts/test_*.py` that fails first.
- Never weaken a check to get green: no raised threshold, no deleted baseline entry, no widened skip list, no `deny.toml` change, no `--no-verify`, `SKIP` or overridden `core.hooksPath`. A gate threshold, baseline or skip list changes only with a human ruling recorded on the issue.

## 3. When it is not fixed in this change: record it

A failing test that is not fixed gets an issue and a known-failure entry that names it; the ratchet then makes sure the entry cannot go stale.

1. **File the issue under the follow-up tracker #1418** (itself a sub-issue of epic #1214, which is at GitHub's 100 sub-issue limit). Title in the style of its children (`node-host: ...`, `parser: ...`, `perf: ...`). Body: the oracle and case or the full test title, the command that reproduces it, Node's output against jsse's, the reduced script, the spec section or Node source it comes from, and "Part of #1214". Then attach it:

       gh issue create --repo pmatos/jsse --title "node-host: ..." --body-file body.md
       ID=$(gh api repos/pmatos/jsse/issues/<new> --jq .id)
       gh api --method POST repos/pmatos/jsse/issues/1418/sub_issues -F sub_issue_id=$ID

   (`gh api repos/pmatos/jsse/issues/1418/sub_issues --jq '.[] | "\(.number) \(.state) \(.title)"'` lists the children; a second POST of the same issue answers 422 "duplicate sub-issues", which also proves the syntax.) Search first (`gh issue list --search`) so you do not file a duplicate. New issues carry `needs-triage` when a maintainer ruling is needed; an engine ruling (for example a limit that trades memory for depth) is a question, so state it as one.
2. **Add the entry to your own known-failures file**, named for your issue, never to a shared one another change is editing. The formats, by tool:

   | tool | file | entry |
   | --- | --- | --- |
   | `run-ts-compiler-oracle.py`, conformance, fourslash, project, transpile and core unit-test oracles | `ts-suite/<name>-known-failures*.txt` | full test title, TAB, `#issue`; `--record` rewrites the entries of the slice |
   | `run-ts-lint-oracle.py`, budget | `ts-suite/lint-known-failures.txt`, `budget-1297-known-failures.txt` | case or test, TAB, `#issue` |
   | unit-test oracles | `ts-suite/unittests-*.json` | `"known_failures": {"<test title>": "#issue"}` |
   | hereby stacks | `ts-suite/hereby-stacks-known-failures.json` | `"<case>": "#issue"` |
   | pass-list gate | `ts-suite/pass-list-1296-not-passing.txt` | runner, TAB, bucket, TAB, id, TAB, `#issue` (recorded with `--update-baseline --issue '#1418'`) |

3. **The ratchet.** A test that is listed and now passes fails the run (delete the entry in the change that fixes it); a test that passes on Node, fails on jsse and is not listed fails the run; a Node failure or a changed count against `ts-suite/oracle.json` fails the run. The pass list `ts-suite/jsse-pass-1296.json` may only gain: a recorded bucket may not be removed or change its size or digest, the not-passing set may only shrink, and the TypeScript pin may not change without a ruling. `scripts/check_ts_pass_list_ratchet.py` enforces it in the `ts-pass-list-ratchet` pre-commit hook and in CI; details in [the pass-list page](library-tests-typescript-pass-list-1296.md). The test262 baseline (`test262-pass.txt`) is likewise gain-only.
4. **Gain entries when you can.** After a fix lands, `run-ts-pass-list.py <runner>:<bucket> --update-baseline` records a completely passing bucket; the nightly's `new-pass` counts show what is ready. Never record under `JSSE_GC_STRESS`, with `--sample`, or on a binary that is not the one being merged.

## 4. Changing the tree: the mechanics

- Work in your own git worktree on a branch `vibe/<issue>-<slug>` branched from the newest `origin/main`; never in the main checkout. Run `./scripts/ensure-hooks.sh` first and again before committing; a commit goes through the hooks (token budget, ratchet, ruff, typos, conventional commit message).
- Each authored file stays under 20,000 bytes (5,000 tokens by the hook's estimate); an oversized file may not grow. So new code, new known-failure lists and new docs go in **new sibling files** named for the issue (`scripts/ts_<area>_<issue>.py`, `ts-suite/<area>-<issue>-known-failures.txt`, `docs/agents/library-tests-typescript-<area>.md`) and are linked from [library-tests-typescript.md](library-tests-typescript.md) when the change lands. When several changes run in parallel, do not edit a file another change also edits (the shared docs, `CONTEXT.md`, `ts-suite/oracle.json`); that is what the sibling files avoid.
- Commits: [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/), lowercase first letter, stage explicit paths (never `git add -A`). Publish with `git push -u origin HEAD`; no plain `--force`, no rebase of a published branch.
- PR body: "Part of #1214", plus "Fixes #N" only when every acceptance criterion of #N is met, else "Part of #N". Required checks: `build`, `Validate PR title` and `SonarCloud analysis` on the final head commit (`timeout 600 gh pr checks <pr> --watch`, repeated). Read the SonarCloud findings and fix the real ones.
- Docs: a new page must be linked from an index so it is reachable from AGENTS.md; `uv run python scripts/check_agent_docs_links.py --strict` says which are not.
