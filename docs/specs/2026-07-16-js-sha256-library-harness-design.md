# js-sha256 library harness design

## Context

Issue #236 tracks four independent Node-compatibility library targets and
requires one library per pull request. This slice adds `js-sha256`, whose
deterministic SHA-224/SHA-256 and HMAC vectors exercise UTF-16 input handling,
32-bit Number bitwise operations, and ArrayBuffer/TypedArray element access
without requiring entropy or another host API.

The upstream Node entry repeats the same vector files under several module
loader configurations and includes a worker-only smoke test. A single esbuild
bundle cannot reproduce Node's `require.cache` eviction, and JSSE does not
provide workers. The cryptographic behavior itself lives in two self-contained
upstream vector files.

## Requirements

- Pin an immutable upstream js-sha256 release.
- Run the upstream SHA-224/SHA-256 and HMAC vector files without copying their
  cases into JSSE.
- Exercise string, Array, Buffer, TypedArray, and ArrayBuffer inputs.
- Use the shared in-process Mocha-shaped harness on JSSE and real Mocha on Node.
- Require equal, non-zero test counts on both engines.
- Keep Node host compatibility in `scripts/`; do not add it to JSSE globals.

## Approaches considered

1. Add `qs` first. It provides broader string and object coverage, but its tape
   suite requires assertion semantics that the shared harness does not yet
   expose.
2. Add `tweetnacl-js` first. Its known-answer vectors are deterministic and
   valuable, but tape integration and its heavier curve arithmetic make it a
   larger first slice.
3. Add `js-sha256` first. Its compact Mocha suite fits the existing
   describe/it runner and isolates dense bitwise, UTF-8, and typed-array
   behavior. This is the selected approach.

## Design

Add a `scripts/libs/js-sha256.sh` configuration pinned to upstream `v0.11.1`.
Its prepare hook installs only the pinned Mocha dependency and copies a
repository-owned bundle entry into the clone.

The entry detects JSSE's `--node` host floor. On JSSE it uses the shared
Mocha-shaped harness, adding Mocha's `context` alias locally. On Node it creates
a real programmatic Mocha runner before loading the same upstream vector files.
Both paths expose the module's SHA functions and the two assertion operations
used by the upstream files in their expected global shapes, enable the Buffer
cases, and load each vector file once. The focused assertion seam avoids
bundling `expect.js` 0.3.1, whose sloppy-mode initialization writes to a
function's read-only `length` property and throws after strict-mode bundling.
Both paths also select upstream's browser/webpack mode so the vectors exercise
the library's pure-JavaScript implementation instead of Node's native `crypto`.

The existing PASS/FAIL/TOTAL summary is parsed by the library verdict. The
exact observed count is locked in the config, so changes to bundling or
upstream preparation cannot silently reduce coverage.

## Validation

- Run js-sha256 on Node alone to establish the oracle count.
- Run the default js-sha256 harness to require JSSE/Node agreement.
- If an engine defect is found, add focused spec-derived coverage and run the
  corresponding test262 area before changing engine code.
- Run the shared harness and Buffer shim self-tests, repository quality gate,
  and full test262 suite to catch regressions.
