# Strict Mode Enforcement Fixes Plan

## Summary

Strict mode enforcement has gaps across both parser and interpreter, causing ~185 failing strict-specific tests plus ~316 failing `onlyStrict` tests and ~598 failing `noStrict` tests. The issues cluster into 8 distinct categories.

## Issue Categories

### 1. Strict Mode Inheritance Not Propagated to Function Objects (~145 tests)
**Root Cause**: `is_strict` on `JsFunction::User` is set only by `is_strict_mode_body(&body)` which checks only the function's own body for `"use strict"`. It does NOT check the enclosing environment's strictness.

**Locations to fix**:
- `src/interpreter/eval.rs:142` — Function expression
- `src/interpreter/eval.rs:162` — Arrow function
- `src/interpreter/exec.rs:29` — Function declaration
- `src/interpreter/mod.rs:1526` — (class methods/etc)

**Fix**: Change `is_strict: Self::is_strict_mode_body(&f.body)` to `is_strict: Self::is_strict_mode_body(&f.body) || env.borrow().strict`

**Impact**: This single fix will cascade to fix:
- Strict `arguments.callee` thrower (arguments object already checks `is_strict`)
- Strict assignment to read-only properties (env.strict checks in assignment paths)
- Strict delete of non-configurable properties
- Strict `this` binding (undefined in strict function calls)
- Many other strict-mode-aware code paths

**Estimated test impact**: ~50-100 tests

### 2. Eval Does Not Inherit Caller Strict Mode (~30 tests)
**Root Cause**: In `src/interpreter/builtins/mod.rs:1542-1581`, the eval builtin:
1. Creates a new `Parser` that starts with `strict: false`
2. Only checks if the eval source itself has `"use strict"`
3. Does not check if the calling environment is strict
4. Always uses `global_env` as the execution environment (no direct eval)

**Fix**:
- Pass the calling env's `strict` flag to the parser (needs `Parser::set_strict()` call)
- If caller is strict, set the parser to strict mode before parsing
- Use the calling scope (not just global_env) for direct eval

**Tests affected**: `eval-strictness-inherit-strict`, `strict-caller-function-context`, `strict-caller-global`, directive-prologue tests with eval

### 3. Directive Prologue Detection Ignores Escape Sequences (~5-10 tests)
**Root Cause**: `is_directive_prologue()` and `is_strict_mode_body()` check the decoded string value `"use strict"`, but escaped strings like `"use str\x69ct"` or `'use str\ict'` (line continuation) should NOT be treated as directives.

**Locations**:
- `src/parser/mod.rs:481-486` — `is_directive_prologue()`
- `src/interpreter/mod.rs:320-331` — `is_strict_mode_body()`
- `src/interpreter/builtins/mod.rs:1564-1566` — eval's strict check

**Fix**:
- Option A: Track whether a `StringLiteral` had escape sequences (add `StringLiteralRaw(String, String)` token variant or a flag)
- Option B: Compare source text slice against `"use strict"` or `'use strict'` (exact 12-char match without escapes)

**Tests affected**: `14.1-4-s`, `14.1-5-s`, `get-accsr-not-first-runtime`

### 4. Arrow Function Parameter Validation Missing (~18 tests)
**Root Cause**: Arrow functions don't validate parameters against strict-mode rules:
- `eval`/`arguments` as parameter names not rejected in strict mode
- Non-simple params + `"use strict"` in body not rejected
- No retroactive param validation when arrow body has `"use strict"`

**Locations**: `src/parser/expressions.rs` — multiple arrow function parse sites (lines 610-835, 1286-1360)

**Fix**: After parsing arrow function body, if `body_strict` or `self.strict`:
1. Check params for `eval`/`arguments` names
2. Check non-simple params + use strict body
3. Validate duplicate params

### 5. Object/Class Method Bodies Don't Check Strict + Non-Simple Params (~18 tests)
**Root Cause**: Several object method parse sites discard `body_strict` with `let (body, _) = ...`:
- `src/parser/expressions.rs:1003` — async methods
- `src/parser/expressions.rs:1037` — generator methods

**Fix**: Use `body_strict` and check:
1. `body_strict && !is_simple_parameter_list` -> SyntaxError
2. Retroactive eval/arguments param check

### 6. Function With `eval`/`arguments` Params + Strict Body Not Rejected (~2-4 tests)
**Root Cause**: When a function body has `"use strict"`, the parser checks for duplicate params but NOT for `eval`/`arguments` as param names retroactively.

**Location**: `src/parser/declarations.rs:205-212`

**Fix**: Add `check_strict_binding_identifier` for each param name when `body_strict` is true.

### 7. Sloppy Mode `this` Not Wrapped to Object (~12 tests)
**Root Cause**: In `call_function()` at `src/interpreter/eval.rs:5085-5095`, when a sloppy function is called with a primitive `this` (number, string, boolean), it should be wrapped via `ToObject()`. Currently only `undefined`/`null` are handled (replaced with global).

**Fix**: Add a branch:
```
if !is_strict && !is_arrow {
    if matches!(_this_val, JsValue::Undefined | JsValue::Null) {
        // Use globalThis
    } else if !matches!(_this_val, JsValue::Object(_)) {
        // ToObject() wrapper
    }
}
```

**Tests affected**: `10.4.3-1-103` through `10.4.3-1-106`, `.call()` with primitives

### 8. Strict Function Declarations in Statement Position Not Rejected (~2 tests)
**Root Cause**: In strict mode, function declarations are only allowed as top-level statements or in blocks, not as the body of `if`, `while`, `for`, etc. The parser doesn't check this.

**Location**: `src/parser/statements.rs` — `parse_if_statement`, `parse_while_statement`, etc.

**Fix**: After parsing the body statement of `if`/`while`/`for`/`with`/labelled, check if it's a `FunctionDeclaration` and if strict mode is active, throw SyntaxError.

## Implementation Priority (by test impact)

1. **Strict mode inheritance** (Issue #1) — ~50-100 tests, single conceptual change in 4 locations
2. **Eval strict inheritance** (Issue #2) — ~30 tests, moderate complexity
3. **Arrow param validation** (Issue #4) — ~18 tests
4. **Method body strict checks** (Issue #5) — ~18 tests
5. **Sloppy this wrapping** (Issue #7) — ~12 tests
6. **Directive prologue escapes** (Issue #3) — ~5-10 tests
7. **Eval/arguments retroactive check** (Issue #6) — ~2-4 tests
8. **Function in statement position** (Issue #8) — ~2 tests

## Out of Scope

These are related but NOT strict-mode-specific issues:
- **Function expression name binding**: Named function expressions don't create a binding for their name inside the body (e.g., `var f = function x() { return x; }`). This affects ~20+ tests but is a general feature gap, not strict mode.
- **Sloppy implicit globals in nested scopes**: Assignment to undeclared variables inside blocks/functions in sloppy mode should create global properties, but JSSE scopes them locally. This affects ~20+ `noStrict` tests.
- **Escaped reserved word identifiers**: 86 `syntax-error-ident-ref-*-escaped` failures are parser-level (escaped keyword identifiers should still be rejected).
- **TCO tests**: Require `tail-call-optimization` feature support.
- **Function.prototype.caller/arguments throwers**: Missing %ThrowTypeError% on Function.prototype. Affects ~5 `restricted-properties` tests.

## Regression Risk

- Issue #1 (strict inheritance) could cause regressions if any tests rely on functions inside strict contexts being treated as sloppy. Careful testing needed.
- Issue #3 (directive detection) changes could affect tests that currently pass by using `"use strict"` with escapes.
- Issue #7 (this wrapping) requires `to_object()` which creates wrapper objects — need to ensure `Number`, `String`, `Boolean` wrapper prototypes are set up correctly.

## Test Verification Strategy

After each fix:
1. Run targeted test262 directories: `language/directive-prologue/`, `language/function-code/`, `language/eval-code/`
2. Run `language/expressions/assignment/dstr/` for strict destructuring
3. Run full suite to check for regressions
4. Compare against baseline `test262-pass.txt`
