# Zod library harness design

## Scope

Add Zod's runtime-focused v4 classic suite as one reproducible real-world
library corpus under issue #279. Pin the latest stable release available when
the issue was assigned, v4.4.3. Its native Vitest run contains 1,092 runtime
tests across 79 files; the library harness runs every registered test in both
normal and global `jitless` modes for an exact 2,184-case lock.

Typecheck-only Vitest projects are outside the JavaScript-engine corpus: they
exercise TypeScript rather than ECMAScript execution. Zod's v3 and v4-mini
suites are also outside this issue, whose normal/jitless requirement targets
the v4 classic validator.

## Approaches considered

1. Add separate `zod` and `zod-jitless` library configurations. This reuses the
   runner unchanged, but permits the two configurations to drift and makes the
   issue's one-command expected-count lock awkward.
2. Extend the runner with prefixed bundle variants and optional process
   isolation. This keeps one source corpus and count lock while isolating the
   heap and event loop as well as module state. This is the selected approach.
3. Concatenate two independently scoped IIFE copies into one process. This was
   prototyped first, but sustained async work in the normal copy polluted the
   jitless copy and exposed continuation-liveness failures before the corpus
   could finish.

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
the separately excluded TypeScript project. The runner builds two files from
the same IIFE, one under each mode prefix, and launches them in independent
engine processes. Each copy registers the full upstream suite with a mode
suffix. Each wrapper sets Zod's documented config `jitless` value immediately
before invoking the original body, after upstream `beforeEach` hooks and before
upstream cleanup. Normal and jitless therefore share source bytes and assertion
logic without sharing mutable schemas, probe caches, GC state, or host jobs.

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

The static-corpus portability patches are guarded against upstream drift. The
10 MiB base64 throughput fixture is scaled symmetrically to 64 KiB, still a
large-input validation without dominating a tree-walker run. Its artificial
500 ms async-refinement delay becomes a zero-delay timer on both engines while
retaining the asynchronous boundary (#310). Vitest's per-file cache and
prototype cleanup are reproduced explicitly. One jitless-only async function
refinement remains a visible `# SKIP` for the continuation bug in #309.

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

## Result

Both JSSE and Node register exactly 2,184 cases. Node is green. JSSE reports
2,176 passing and eight failing cases: the same four failures in normal and
jitless mode, reduced to Date parsing (#313), array integrity levels (#314),
and Node-compatible JSON parse diagnostics (#315). The optional-chain parser
gap surfaced during bundle startup is fixed with focused test262 coverage.
