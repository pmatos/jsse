# PrismJS library harness design

## Scope

Add PrismJS as the first parser/transform/regex library covered by issue #233.
Pin PrismJS v1.30.0 and run all 2,563 token-stream fixtures in
`tests/languages/**/*.test` across 367 language identifiers. The 11
`*.html.test` rendering fixtures are outside this slice because the issue calls
for token-stream coverage; plugin, DOM, coverage, and test-runner unit suites
remain for later work.

## Considered approaches

1. Generate a self-contained fixture entry (chosen). A Node-only prepare step
   reads Prism's fixtures and component catalog, then writes one JavaScript
   entry containing all inputs, expected token streams, dependency-ordered
   component sources, and a small in-process runner. This keeps jsse's runtime
   free of filesystem and Mocha dependencies while preserving the upstream
   fixture semantics.
2. Bundle Prism's existing Mocha runner. This retains upstream code directly,
   but it requires runtime filesystem/path support plus Chai, Prettier, yargs,
   and Mocha integration solely to discover and compare static fixtures.
3. Commit a curated fixture subset. This would keep the bundle smaller but
   would weaken the intended broad regex stress coverage and make omissions
   difficult to audit.

## Runtime design

The prepare-time generator discovers fixtures deterministically by sorted
language directory and filename. It parses each fixture using Prism's documented
three-section separator, excludes HTML fixtures, parses the expected JSON, and
uses Prism's own dependency resolver to record the exact component load order.
It embeds Prism core and each referenced component as source strings.

The generated entry compiles Prism core once and component functions lazily.
For every fixture it creates a fresh Prism object, loads that fixture's ordered
components, tokenizes the code with the same main-language selection rule as
Prism's runner, simplifies tokens to `[type, content]` pairs while dropping
blank strings, and compares JSON forms. This preserves upstream isolation:
language grammars cannot leak mutations into later fixtures.

Failures print the fixture name and compact expected/actual JSON. A final line
reports `PrismJS: P passed, F failed, T total`; failures exit non-zero. The
library config locks the expected total at 2,563, and the shared harness requires
both jsse and Node to pass with that exact count.

## Validation

- Run the generator twice and compare output to prove deterministic discovery.
- Run the generated suite on Node, then on jsse with the normal cross-check.
- Run the existing Acorn library suite to guard the adjacent harness path.
- Run repository formatting, linting, release build/tests, and test262 without
  updating the feature-branch baseline.
