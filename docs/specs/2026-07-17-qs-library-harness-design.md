# qs library harness design

## Context

Issue #299 adds `qs` as a Node-compatibility stress test. Its upstream suite is
170 KB of tape tests covering nested query-string parsing, array and depth
limits, UTF-8 and legacy charset encoding, surrogate pairs, prototype-pollution
guards, Buffer inputs, and side-channel implementations backed by Map and
WeakMap.

The shared library harness already supplies Node globals, Buffer, and
Mocha/Jest/QUnit-shaped runners on JSSE, but it does not expose tape's
assertion-object API. Bundling tape itself would pull in Node's stream, events,
path, and filesystem runner stack, which is intentionally above the host shim's
small syscall floor.

## Requirements

- Pin an immutable upstream qs release and run its test files without copying
  or weakening their assertions.
- Use a reusable tape adapter on JSSE and real tape on Node.
- Match tape's assertion count, including skipped subtests, rather than merely
  checking that the bundle exits successfully.
- Exercise the shared Buffer shim and the engine's existing Map/WeakMap paths.
- Keep all compatibility code under `scripts/`; do not add Node APIs to JSSE's
  default global object.

## Approaches considered

1. Bundle upstream tape unchanged on both engines. This keeps exact framework
   semantics, but tape's runner requires Node stream, events, path, and
   filesystem modules that the library shim deliberately does not implement.
2. Rewrite qs tests into a bespoke entry. This would minimize runner code but
   make the oracle less faithful and would not satisfy the shared tape
   prerequisite for later libraries.
3. Add a focused tape adapter to the shared harness and select it only on JSSE,
   while the identical bundle selects real tape on Node. This is the selected
   approach because it preserves upstream cases, gives Node an independent
   framework implementation, and creates the reusable seam requested by the
   issue.

## Design

Extend `scripts/node-test-harness.js` with a tape adapter that queues top-level
and nested tests, emits TAP, counts individual assertions, supports tape's
skip/plan/end/teardown lifecycle, and implements the assertion and interception
surface used by qs. Reuse the harness's established structural-equivalence
function for deep comparisons. Add a deterministic harness fixture that covers
nested and skipped tests, plans, throws/match assertions, teardown, and
temporary prototype interception.

Add a small CommonJS selector module. Under JSSE's `--node` host mode it exports
the shared adapter; on Node it exports bundled upstream tape. The qs config
aliases `require('tape')` to that selector, so upstream test sources remain
unchanged. Its generated entry loads the parse, stringify, and utility test
files. Pin qs to v6.15.3, tape to 5.10.2 through the upstream lock-compatible
dependency, and lock the observed oracle count at 1,013 assertions.

The verdict requires a zero engine exit, a non-zero TAP plan whose pass count
equals its test count, no failing assertions, and the pinned expected count.

## Validation

- Run the harness self-test before and after the adapter change.
- Run qs on Node alone to establish the 1,013-assertion oracle.
- Run qs on JSSE with the default Node cross-check and exact-count lock.
- Run Buffer shim fixtures because qs's charset cases consume Buffer.
- If qs exposes an engine defect, add focused spec-derived coverage and run the
  corresponding test262 area before changing engine code.
- Run the repository formatting, linting, release test/build, and full test262
  gates. Since this design changes scripts only, the test262 pass count should
  remain unchanged.
