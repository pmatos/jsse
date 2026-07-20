# UglifyJS library harness design

## Scope

Add UglifyJS as the second parser/transform/regex library covered by the
follow-up to issue #233. Pin the latest release, v3.19.3, and run all 4,233
top-level cases from its 126 `test/compress/*.js` DSL fixtures. Every case must
exercise UglifyJS's own parser, compressor and tree transforms, scope analysis,
identifier and property mangling when requested, output generator, AST
validation, and output reparse.

The upstream runner's `expect_stdout` stage executes generated programs in a
fresh Node `vm` or subprocess. That host-runtime equivalence layer is outside
this additive parser/transform/codegen slice and cannot run in the existing
filesystem-free library harness without adding `vm` and `child_process` host
APIs. The transformation and exact-output assertions still run for every case,
including cases that also declare `expect_stdout`.

## Considered approaches

1. Generate a self-contained compress-suite entry (chosen). A Node-only prepare
   step embeds UglifyJS's implementation sources, DOM property catalog, and all
   compress fixtures, then adapts the upstream in-process test logic to run
   synchronously without filesystem or subprocess access. This preserves the
   requested pipeline and maximizes fixture coverage without changing jsse's
   default globals.
2. Emulate the native runner's Node host APIs. This could retain its subprocess
   orchestration and runtime-output checks, but would require broad `fs`, `vm`,
   and `child_process` shims unrelated to the issue and would no longer be a
   small additive library-harness change.
3. Commit a curated fixture subset. This would produce a smaller and faster
   bundle, but it would weaken the issue's intended broad compressor workload
   and make omissions hard to audit.

## Runtime design

The prepare-time generator discovers fixture files in deterministic sorted
order. It concatenates the same UglifyJS library files used by upstream's
`test/node.js`, substitutes the upstream test exports, and embeds that source
plus the DOM property catalog into the entry. At runtime the bundle constructs
the upstream test API with `Function`, parses each embedded DSL fixture, and
discovers its top-level labeled cases exactly as the native runner does.

The adapted case runner retains upstream configuration evaluation, AST
validation, exact expected-code comparison, warning comparison, compression,
scope analysis, rename/mangle/property-mangle paths, code generation, and
output reparse. It reports `UglifyJS compress: P passed, F failed, T total` and
exits nonzero on any failure. The library config locks the expected count at
4,233, and the shared harness requires both jsse and Node to pass the identical
bundle with that count.

The first full run exposed one engine seam across ten Unicode-output cases:
non-Unicode RegExp ranges spanning `U+D800–U+DFFF` omitted jsse's internal
PUA-mapped surrogate code units, and functional `@@replace` converted the
matched/replacement strings lossily. The implementation expands those ranges
onto the mapped interval and carries `JsString` values through replacement.
The final library run is green without skips.

## Validation

- Generate the entry twice and compare it byte-for-byte.
- Run the generated suite on Node and through the normal jsse/Node cross-check.
- Run Acorn as the adjacent parser-harness regression check.
- Run repository formatting, linting, release build/tests, and full test262
  without updating the feature-branch baseline.
