# Plan: Exclude test262-extra/ (and tests/*.js) from SonarCloud analysis and duplication checks

## 1. Problem restated

SonarCloud's Quality Gate fails on PRs that add files under `test262-extra/`
because that directory deliberately contains many small, structurally similar
JS fixtures (shared harness boilerplate across per-site variants: catch /
for-in / for-of / for-await-of / for-init) and deliberately "wrong-looking"
constructs that are the exact subject under test (an un-awaited promise to
observe microtask ordering, an async generator with no `yield`, one-iteration
`for` loops). SonarCloud scores both as production-code defects: 15.5%
duplication on new code (gate requires ≤3%) and a C Reliability Rating (gate
requires ≥A). This has already broken the gate on #787, #789, and #790 without
blocking merges, so it is pure noise that should be configured away at the
analysis-scope level rather than fixed in the fixtures themselves.

## 2. Spec basis

N/A: no JavaScript behavior change. This is a SonarCloud analysis-scope
configuration change only — it touches no file under `src/`, `spec/`, or
`test262/`, and changes no parsing, runtime, or built-in behavior.

## 3. Files to touch

- **`.sonarcloud.properties`** (new file, repo root) — the exclusion config.

  **Important — this is not `sonar-project.properties`.** This repo has no
  `.github/workflows/*.yml` that runs a `sonar-scanner`/`sonarcloud-github-action`
  step (checked: no workflow file references "sonar" anywhere), and no
  `sonar-project.properties` exists today. The `@sonarqubecloud[bot]` PR
  decoration comments quoted in the issue are the signature of SonarCloud's
  **Automatic Analysis** mode (the zero-CI GitHub App integration), not
  CI-based analysis. SonarCloud's own docs state Automatic Analysis **ignores**
  `sonar-project.properties` outright ("If you import a project that already
  contains a `sonar-project.properties` file, SonarQube Cloud will ignore the
  parameters in your `sonar-project.properties` file.") and instead reads a
  distinct `.sonarcloud.properties` file, which does support `sonar.exclusions`
  and `sonar.cpd.exclusions`. Creating `sonar-project.properties` instead (the
  issue's literal suggestion) would silently do nothing — this is the one
  point in the issue proposal this plan deliberately deviates from, and the
  implementation stage should say so in the PR description.
- No changes under `src/`, `scripts/`, `benchmarks/`, `.github/`, `docs/`, or
  `CONTEXT.md` — this is a single standalone config file with no code
  dependents, so there is nothing else to wire up.

### `.sonarcloud.properties` content

```properties
# Automatic Analysis reads this file; sonar-project.properties is ignored.
sonar.exclusions=test262-extra/**,tests/**/*.js
sonar.cpd.exclusions=test262-extra/**,tests/**/*.js
```

Pattern syntax confirmed against SonarCloud docs: `**` matches directory
segments (recursive), patterns are comma-separated, case-sensitive, and
relative to the project base dir.

`tests/**/*.js` (not all of `tests/**`) is scoped to just the `.js` fixtures
(e.g. `tests/console-assert.js`, `tests/basic-expressions.js`) — these
reimplement the same `sameValue`-style harness per file, the same pattern
flagged in `test262-extra/`. The `.rs` integration tests under `tests/`
(`tests/test262_smoke_oracle.rs`, `tests/console_stderr_routing.rs`, etc.) are
real Rust test code exercising engine behavior, not JS fixtures, and are left
subject to normal analysis. This is a judgment call on the issue's hedged
"(and probably `tests/**`)" wording — narrower than a blanket `tests/**` so
Rust test-code reliability issues, if any, keep surfacing.

The implementation stage should leave a one-line PR description note (and
may cross-post to the issue) flagging both judgment calls for override: (a)
`.sonarcloud.properties` instead of `sonar-project.properties`, (b)
`tests/**/*.js` instead of all of `tests/**`.

## 4. TDD slices

This is a config-only change with no unit-testable behavior (SonarCloud
analysis runs server-side against an opened PR, not locally), so there is no
red/green cycle in the usual sense. The closest equivalent:

1. **Slice 1 — add the exclusion file.**
   - "Test": `./scripts/lint.sh` and the pre-commit hooks
     (`end-of-file-fixer`, `trailing-whitespace`, `check-yaml`/`check-toml`
     don't apply to `.properties`, `typos`) must pass on the new file —
     i.e. it must be a clean, newline-terminated, typo-free properties file.
     Run `pre-commit run --files .sonarcloud.properties` (or let the commit
     hook run it) before committing.
   - "Production code": `.sonarcloud.properties` as drafted above.
   - Red state: before this slice, SonarCloud has no exclusion config at all
     (Automatic Analysis default-scans everything), which is the documented
     failure.
   - Green state: the file exists at repo root with the two properties.
2. **Slice 2 — external verification (cannot be forced locally, and this PR
   alone cannot prove it).**
   This PR's only change is a `.properties` file with no JS in it, so the
   Quality Gate will pass on *this* PR regardless of whether the exclusion
   actually works — a green gate here is not evidence. SonarCloud only
   re-evaluates against real `test262-extra/`/`tests/*.js` content when a PR
   touching those paths is next analyzed, and there is no local scanner to
   run as a stand-in. The implementation stage should either:
   - query the SonarCloud web API for the project (project key visible on the
     bot's existing #789 decoration) scoped to this PR, e.g. a
     `components/tree` or `issues/search` call filtered to `test262-extra/`,
     to confirm those paths are no longer indexed/flagged for this branch; or,
     if that call can't distinguish "excluded" from "just has no new issues
     this PR happens to introduce",
   - say plainly in the PR body that the fix is unverified until the next PR
     that adds `test262-extra/`/`tests/*.js` content re-triggers the gate,
     rather than claiming a false green from this PR's own passing check.

## 5. Test surface

No `test262/test/...` directory is relevant — this changes no engine
behavior. The gates that actually cover this change:

- `./scripts/lint.sh` — general repo lint/format gate; should pass trivially
  since no Rust/Python source changes.
- `cargo test --release` — unaffected; no `src/` changes.
- No `run-test262.py` / `run-custom-tests.py` run is needed; this PR carries
  no behavior change to validate against the baseline.
- The real gate is SonarCloud's own PR decoration check on the PR this change
  travels in (see Slice 2) — that is external to this repo's CI and can only
  be observed after the PR is opened, not proven in this stage or guaranteed
  synchronously in the implementation stage.

## 6. Regression risk

Very low and non-engine:

- `.sonarcloud.properties` is not read by `cargo`, the interpreter, the
  parser, or either test runner — it cannot move `test262-pass.txt`, affect
  `eval_expr`/`exec_statement`, `property.rs`, GC rooting, the `ObjectKind`
  matches, the bytecode fast path, or any Node-compat library harness.
- Main risk is getting the SonarCloud-side mechanics wrong in a way that
  silently does nothing: using the wrong filename
  (`sonar-project.properties`, ignored in Automatic Analysis — see §3) or a
  pattern typo (e.g. forgetting the recursive `**` and only matching
  top-level files). Mitigated by using the documented filename and the
  documented Ant-pattern syntax verbatim.
- Secondary risk: over-broad exclusion silently hiding a real future issue in
  `test262-extra/` or `tests/*.js` (e.g. an actual accidental infinite loop
  typo'd as intentional). Accepted: these are test fixtures validated by
  `run-test262.py`/`run-custom-tests.py` actually executing them, which is a
  stronger correctness signal than a static-analysis heuristic tuned for
  production code.
- No risk to `tests/*.rs` reliability analysis, since those files are
  deliberately excluded from the new patterns (see §3).

## 7. Out of scope

- Switching the project from Automatic Analysis to CI-based analysis (would
  enable `sonar-project.properties` but is a much larger, unrelated change to
  how SonarCloud is wired up).
- Adding a `sonar-project.properties` file as well "just in case" — redundant
  dead config under Automatic Analysis; would only confuse a future reader.
- Narrowing `sonar.sources` or adding `sonar.inclusions` — not requested by
  the issue and risks silently dropping real production files from analysis.
- Excluding `scripts/` or `benchmarks/` JS-like content — not flagged by the
  issue or by any cited SonarCloud finding.
- Per-rule suppressions (`javascript:S9383`, `S3531`, `S1751`) via SonarCloud's
  own issue-resolution UI ("Won't Fix") — a heavier, per-issue mechanism;
  path-level exclusion is the right-sized fix here per the issue's own
  proposal.
- Touching `tests/**/*.rs`.
- Any formatting/refactor of unrelated files.
