# Design: Running Acorn's Test Suite on JSSE

## Goal

Run acorn's full test suite (~3,500 test cases x 4 modes = ~12,000 invocations) on
jsse as a permanent integration test. Acorn is a JavaScript parser written in
JavaScript — if jsse can run it and pass its own tests, that proves the engine
handles real-world JS correctly.

## Approach: Single Full Bundle

Bundle the entire acorn test suite (all test files, all 4 modes, acorn + acorn-loose)
into a single IIFE file using esbuild. This eliminates all CommonJS `require()` calls
and produces a self-contained `.js` file that jsse can execute directly.

## Script Architecture

A single script `scripts/run-acorn-tests.sh` handles everything:

1. **Clone & build acorn** — cached in `/tmp/acorn`, skipped if already present
2. **Bundle** with `npx esbuild` into a single IIFE file (`/tmp/acorn-tests-bundle.js`)
3. **Prepend runtime shims** — produces `/tmp/acorn-tests-final.js`
4. **Build jsse** — `cargo build --release`
5. **Run** the bundled file on jsse
6. **Report** pass/fail counts from test runner output

The script is idempotent. A `--clean` flag forces a fresh acorn clone.

## Runtime Shims

The bundled test runner references Node.js globals that jsse doesn't provide:

```javascript
if (typeof console.group === "undefined") {
  console.group = function(name) { console.log("--- " + name + " ---"); };
}
if (typeof console.groupEnd === "undefined") {
  console.groupEnd = function() {};
}
if (typeof process === "undefined") {
  globalThis.process = {
    exit: function(code) {
      if (code !== 0) throw new Error("Process exit with code " + code);
    },
    stdout: { write: function(s, cb) { if (cb) cb(); } }
  };
}
```

No other shims are needed — esbuild eliminates all `require()` / `fs` / `path` references.

## Output Parsing

Acorn's test runner outputs per-mode results:
```
--- Normal ---
3456 tests passed
3 failures:
  ...
```

The script captures jsse's stdout/stderr and reports:
- Whether jsse crashed (non-zero exit, parse error)
- Per-mode pass/fail counts
- Total pass rate
- Error messages or stack traces

## Success Criteria

| Level | Definition |
|-------|-----------|
| L0 | jsse parses the bundle without crashing |
| L1 | Test runner starts executing (mode headers appear in output) |
| L2 | Normal mode: >50% of acorn tests pass |
| L3 | Normal mode: >90% pass |
| L4 | All 4 modes: >90% pass |
| L5 | All 4 modes: 100% pass |

## Scope

This design covers infrastructure only: the bundling script, shims, and test runner.
Fixing jsse features to increase the pass rate is a separate effort informed by the
failure output.

## Key Technical Details

- **Bundle format:** IIFE via `--format=iife --platform=neutral`
- **Acorn dist:** buble-transpiled to ES5-ish (classes->functions, arrows->functions)
- **Test harness:** mostly ES5 with 2 arrow functions and `Object.assign` in run.js
- **Bundle size:** estimated multi-MB (thousands of AST object literals as test expectations)
- **Acorn version:** latest from main branch (pinned by clone date)
