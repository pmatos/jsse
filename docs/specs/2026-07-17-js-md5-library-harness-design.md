# js-md5 library harness design

## Context

Issue #301 is the js-md5 follow-up to the js-sha256 Node-compatibility slice.
The upstream suite is deterministic and exercises MD5 and HMAC-MD5 through
string, Array, Buffer, TypedArray, and ArrayBuffer inputs. Its implementation is
a useful engine stress test for UTF-16-to-UTF-8 conversion and dense 32-bit
Number bitwise operations.

Upstream's Node entry repeats the same vector files under several module-loader
configurations and includes Worker tests. A single esbuild bundle cannot
reproduce `require.cache` eviction, and JSSE does not expose the Worker host API.
The cryptographic coverage itself is contained in two self-contained vector
files with generated Mocha cases.

## Requirements

- Pin an immutable upstream js-md5 release.
- Run the upstream MD5 and HMAC-MD5 vector files without copying their cases.
- Exercise every input and output shape registered by those vector files,
  including UTF-8 strings, Buffer, TypedArray, and ArrayBuffer values.
- Use the shared in-process Mocha-shaped harness on JSSE and real Mocha on Node.
- Require equal, non-zero test counts on both engines and lock the observed
  count in the library configuration.
- Keep Node host compatibility in `scripts/`; do not add default globals to
  JSSE.

## Approaches considered

1. Load the two upstream vector files once through a focused bundle entry. This
   retains the library's behavioral cases while fitting the existing js-sha256
   harness pattern. This is the selected approach.
2. Reproduce upstream's full Node entry. Its repeated module modes depend on
   `require.cache`, and its Worker cases require a host API outside this slice;
   bundling it would provide misleading rather than faithful coverage.
3. Generate separate bundles for each loader mode and emulate Workers. This
   would add substantial harness complexity for repeated cryptographic vectors
   and host behavior that the issue does not require.

## Design

Add `scripts/libs/js-md5.sh`, pinned to upstream `v0.8.3`. Its prepare hook
removes unrelated development tooling, installs pinned Mocha for the Node
oracle, and copies a repository-owned entry into the upstream clone.

The entry detects JSSE's `--node` host floor. On JSSE it uses the shared
Mocha-shaped runner and installs Mocha's `context` alias. On Node it creates a
real programmatic Mocha runner before loading the identical upstream files.
Both paths install the two `expect.js` operations used by the vectors directly,
select js-md5's pure-JavaScript path, enable Buffer cases, expose the imported
`md5` function globally, and load `test.js` and `hmac-test.js` once.

The library verdict parses the common PASS/FAIL/TOTAL summary, rejects non-zero
engine exits through the generalized runner, and requires the exact count
observed on both engines.

## Validation

- Run js-md5 on Node alone to establish the oracle count.
- Run the default js-md5 harness with a clean cache to require JSSE/Node
  agreement.
- Run the shared harness and shim fixture self-tests.
- Run targeted test262 coverage for Number bitwise operators, TypedArray,
  ArrayBuffer, and `String.prototype.charCodeAt`.
- Run the repository format, lint, release build, release tests, and full
  test262 suite. Since this design changes only scripts and documentation, the
  full suite must match the current main-branch result.
