# Zod library harness design

## Scope

Add Zod's runtime-focused v4 classic suite as one reproducible real-world
library corpus under issue #279. Pin the latest stable release available when
the issue was assigned, v4.4.3. Its native Vitest run contains 1,092 runtime
tests across 104 files; the library harness runs every registered test in both
normal and global `jitless` modes for an exact 2,184-case lock.

Typecheck-only Vitest projects are outside the JavaScript-engine corpus: they
exercise TypeScript rather than ECMAScript execution. Zod's v3 and v4-mini
suites are also outside this issue, whose normal/jitless requirement targets
the v4 classic validator.

## Approaches considered

1. Add separate `zod` and `zod-jitless` library configurations. This reuses the
   runner unchanged, but permits the two configurations to drift and makes the
   issue's one-command expected-count lock awkward.
2. Generalize the library runner with arbitrary build/run variants. This gives
   the cleanest process isolation, but expands shared infrastructure for a
   requirement currently unique to Zod.
3. Generate one static entry whose Vitest adapter registers each upstream test
   in normal and jitless modes. This keeps one pinned source corpus, one bundle,
   one command, and one doubled count lock. This is the selected approach.

## Design

Add `scripts/libs/zod.sh` and a generator that discovers the sorted
`packages/zod/src/v4/classic/tests/*.test.ts` set during the cached prepare
step. The generator writes a static entry and a narrow `vitest` compatibility
module. Esbuild aliases upstream imports of `vitest` to that module and bundles
all test files, Zod source, assertion support, and fixture dependencies into a
single IIFE. No filesystem or package resolution remains at runtime.

The compatibility module maps `describe`, `test`/`it`, `beforeEach`, and
`afterEach` onto the shared in-process TAP runner. It reuses a pinned published
expect matcher core, adds Vitest-compatible inline snapshot serialization, and
treats `expectTypeOf` as a runtime no-op because its assertions are enforced by
the separately excluded TypeScript project. Every upstream `test` registration
becomes two TAP tests. Each wrapper sets Zod's documented
`globalThis.__zod_globalConfig.jitless` value immediately before invoking the
original body, after upstream `beforeEach` hooks and before upstream cleanup.

The force-test-harness prelude is used on both JSSE and Node. Node therefore
executes the identical bundle and matcher adapter, while the expected count is
independently established by a green native Vitest run at the pinned tag. The
2,184 lock prevents missing files or registrations from becoming a false pass.

Node-only imports in the upstream tests are replaced at bundle time with small
portable modules only where JSSE's host floor lacks that surface. These
replacements must run identically on Node, remain in `scripts/`, and preserve
the tested validator behavior. Host-only cases that cannot be represented
without materially implementing a new platform API will be excluded
explicitly and documented rather than hidden as passing tests.

## Failure handling

A matcher throw, test/hook rejection, missing summary, nonzero failure count,
timeout, count mismatch, or engine-count disagreement fails the library run.
Any Zod failures that remain after narrow harness corrections will stay visible
and be reduced to follow-up engine issues. Engine behavior is changed only for
spec-backed defects, with focused tests and relevant test262 coverage.

## Verification

- Run native Vitest without typechecking to confirm 1,092/1,092 at v4.4.3.
- Run the generated corpus on Node, then JSSE with the Node cross-check, and
  require exactly 2,184 registered tests on each engine.
- Run the shared harness and Node-shim self-tests when their seams are used.
- Run formatting, Clippy, release build/tests, relevant RegExp, BigInt, Proxy,
  Reflect, and Symbol test262 areas for any engine changes, then full test262.
- Update the library harness documentation with the pin, exact count, result,
  and any explicitly tracked residual failures.
