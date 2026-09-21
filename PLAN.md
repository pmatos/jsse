# Plan: issue #680 — `console.error`/`warn`/`info`/`debug` missing

## 1. Problem restated

jsse's `console` host object exposes only `log` and `assert`
(`src/interpreter/builtins/mod.rs:479-512`); any other WHATWG Console Standard
method — `error`, `warn`, `info`, `debug` — is `undefined`, so calling it
throws `TypeError: undefined is not a function`. This broke `scripts/run-jetstream.py`'s
`printErr` polyfill (issue #655), which now defensively falls back to
`console.log` when `console.error` is missing, but the underlying gap — no way
to write to stderr from JS — remains. Add `console.error`, `console.warn`
(stderr) and `console.info`, `console.debug` (stdout, aliasing `log`) so
`console` covers the common WHATWG logging surface, matching Node's stream
routing.

## 2. Spec basis

N/A: no JavaScript language behavior change. `console` is not part of
ECMA-262 — it is defined by the WHATWG Console Standard, which `spec/`
(the tc39/ecma262 submodule) does not contain and has no clause governing.
jsse already treats `console` as a host object outside spec authority: it is
a lexical `const` binding installed directly in `setup_globals`
(`src/interpreter/builtins/mod.rs:513-521`), not a property of the global
object, so it is invisible to `Object.getOwnPropertyNames(globalThis)` /
`for-in` over the global and cannot appear in any test262 global-object
enumeration test. `console.log`, `console.assert`, and the `print` global are
existing precedent for the same host-API pattern this issue extends.

Design choice (Node-convention, used here only as a host-API precedent, not
as justification for any ECMAScript semantics): `error` and `warn` write to
stderr; `info` and `debug` write to stdout and behave identically to `log`.
This matches Node's stream routing, which is exactly what `scripts/run-jetstream.py`'s
`printErr` polyfill (`typeof console.error === "function" ? console.error :
console.log`) already expects. No separate `printErr` global is added — the
issue floats it as an alternative, but `console.error` alone closes the
stated gap without adding a second host surface.

## 3. Files to touch

- `src/interpreter/builtins/mod.rs` — add `error`, `warn`, `info`, `debug` to
  the `console` object setup block (currently lines ~479-512, alongside `log`
  and `assert`).
- `tests/console-methods.js` — new custom test (sibling to the existing
  `tests/console-assert.js`), covering `typeof`, `.length`, and return-value
  behavior for all four new methods plus a joint smoke check with `log`.
- `tests/console_stderr_routing.rs` — new Rust integration test (pattern:
  `tests/perf_counters_report_paths.rs`'s `Command::new(env!("CARGO_BIN_EXE_jsse"))`
  + captured `output()`, minus the `#![cfg(feature = "perf-counters")]` gate)
  asserting `error`/`warn` land on stderr and not stdout, and `info`/`debug`
  land on stdout and not stderr.
- No `docs/adr/` or `CONTEXT.md` change — this is an incremental extension of
  an already-established host-API pattern (`console.log`/`console.assert`),
  not a new architectural decision or new domain vocabulary.

## 4. TDD slices

Each slice is red (test fails against current `undefined` methods) → green
(implementation makes it pass). All four methods share the same shape as the
existing `log`/`assert` closures, so this is one small vertical slice split
into reviewable steps rather than a horizontal refactor.

1. **`console.error`/`console.warn` exist and route to stderr.**
   - Test: `tests/console_stderr_routing.rs`, a case that runs
     `jsse -e 'console.error("x"); console.warn("y")'` and asserts stdout is
     empty while stderr contains `x` and `y`.
   - Production: in the `console` setup block, add `error` and `warn` native
     functions built the same way as `assert`'s stderr write — reuse
     `format_host_args(args)` for formatting (identical to `log`/`assert`;
     `Display for JsValue` already collapses all object kinds to
     `[object Object]`, so no new formatting logic is needed) and
     `std::io::stderr().write_all(...)` with a trailing `\n` for the write,
     matching the existing `assert` stderr path rather than introducing a
     second stderr-write idiom. `insert_builtin` on `console_id` for both
     keys, function length `0` (matching `log`).
2. **`console.info`/`console.debug` exist and route to stdout, behaving like `log`.**
   - Test: extend `tests/console_stderr_routing.rs` with a case asserting
     `console.info("a"); console.debug("b")` produce `a`/`b` on stdout and
     nothing on stderr.
   - Production: add `info` and `debug` native functions using the same
     `println!("{}", format_host_args(args))` body as `log`, `insert_builtin`
     on `console_id`, length `0`.
3. **Shape/contract checks for all four methods.**
   - Test: `tests/console-methods.js`, following the exact structure of
     `tests/console-assert.js` (`sameValue` helper): `typeof console.error ===
     "function"` (and `warn`/`info`/`debug`), `console.error.length === 0`
     (and the other three), and that each call with 0, 1, and multiple
     arguments returns `undefined` and does not throw.
   - Production: none beyond slices 1-2 — this slice only locks the contract
     down with assertions runnable through the existing exit-code-based
     custom-test harness, since `tests/*.js` cannot itself observe which
     stream received output.

No refactor step: the new closures are structurally identical to `log`/`assert`,
so there is nothing to extract yet. If a fifth method is added later,
extracting a shared "stream + formatter" helper becomes worth it — not now,
per the no-premature-abstraction rule.

## 5. Test surface

- No `test262/test/...` directory exercises this — `console` is outside
  ECMA-262, so test262 has (and should have) no coverage of it.
- No `test262-extra/` addition — that directory is reserved for spec-correct
  behavior not covered by test262; `console` is not spec behavior at all, so
  per `CLAUDE.md` this belongs in `tests/` ("exact host-compatibility
  diagnostics ... remain in `tests/`").
- Gates for this change:
  - `uv run python scripts/run-custom-tests.py tests/console-methods.js` (and
    the full `uv run python scripts/run-custom-tests.py` sweep, to confirm no
    regression in `tests/console-assert.js` or others).
  - `cargo test --release` (runs `tests/console_stderr_routing.rs` along with
    the rest of the Rust test suite).
  - `./scripts/lint.sh`.

## 6. Regression risk

- **Cannot move `test262-pass.txt`.** `console` is a lexical `const` binding
  installed by `setup_globals`, never a property of the global object
  (`src/interpreter/builtins/mod.rs:513-521` uses `global_env.declare`/
  `initialize_binding`, not a global-object property insertion), so no
  test262 test enumerating `globalThis` own properties, `for-in` over the
  global, or global-object shape can observe the new methods. The change
  touches no shared tree-walker path (`eval_expr`/`exec_statement`), no
  `property.rs` MOP code, no GC rooting/`gc_safepoint()`, and no `ObjectKind`
  variant — it is four more native-function closures registered once at
  startup, structurally identical to the existing `log`/`assert` registration.
  Net risk to the test262 baseline: none.
- **Node-compat library harnesses** (`scripts/run-library-tests.sh <lib>`):
  several shims/polyfills feature-detect `console.error`/`warn` (e.g. the
  `printErr` polyfill in `scripts/run-jetstream.py`, and potentially bundled
  library code that logs warnings via `console.warn`). Adding real
  implementations only changes behavior for code that already gated on
  `typeof console.error === "function"` — previously false, now true — so
  output that was silently dropped or routed to `console.log` may now appear
  on stderr instead. This is the intended fix, not a regression, but it means
  a library harness run could show *new* stderr lines that previously never
  printed; that is expected and should not be treated as a failure by itself.
  No currently-green library (`decimal.js`, `big.js`, `acorn`, `prismjs`,
  `uglify-js`, `highlight.js`, `uuid`, `luxon`, `zod`, `moment`) is expected
  to regress, since their verdicts are Node-cross-checked test counts, not
  console output.
- **`scripts/run-jetstream.py`'s `printErr` fallback** (`typeof console.error
  === "function" ? console.error : console.log`) becomes dead-but-harmless
  once `console.error` exists; left as-is (see §7).

## 7. Out of scope

- No `printErr`-style dedicated global — `console.error` covers the issue's
  stated need without adding a second host surface.
- No `console.trace`, `console.table`, `console.group`/`groupEnd`,
  `console.count`, `console.time`/`timeEnd`, `console.dir`, or other WHATWG
  Console methods beyond `error`/`warn`/`info`/`debug` — not requested by the
  issue; can be follow-up issues if a future use case needs them.
- No change to `scripts/run-jetstream.py`'s `printErr` feature-detection
  fallback — it is defensive against binaries built before this fix and
  against any future console gap; removing it is unrelated cleanup, not part
  of closing this issue.
- No extraction of a shared "stream + formatter" helper for the `console`
  closures — four structurally-identical closures do not yet justify an
  abstraction (see §4).
- No `docs/adr/` entry — this is an incremental extension of an established
  pattern, not a new architectural decision.
