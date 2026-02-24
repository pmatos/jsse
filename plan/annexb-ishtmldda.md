# Annex B Legacy Syntax + IsHTMLDDA Implementation Plan

**Estimated impact: ~117 new test262 passes**

## Failure Summary (131 total scenarios, 117 addressable)

| Category | Scenarios | Description |
|----------|-----------|-------------|
| IsHTMLDDA | 48 | Missing `$262.IsHTMLDDA` special object |
| Global code (evalScript) | 13 | `$262.evalScript` not implemented + global property descriptors |
| Function code hoisting | 11 | Default param/arguments/nested block checks |
| RegExp Annex B literals | 12 | `\c` class escape, identity escape, octal, quantifiable assertions |
| HTML comments in Function() | 10 | `<!--` and `-->` as comments in Function constructor |
| Call expr as assignment target | 7 | Runtime ReferenceError instead of SyntaxError in sloppy mode |
| Quick wins | 8 | escape/unescape props, Date.setYear, String.substr |
| for-in initializer | 1 | `for (var a = expr in obj)` evaluation |
| eval-code lex collision | 2 | evalScript interaction with eval |
| **Cross-realm (out of scope)** | **14** | **Requires $262.createRealm()** |

---

## Part A: IsHTMLDDA Implementation (48 scenarios)

### A1. Add `is_htmldda` flag to `JsObjectData`

**File:** `src/interpreter/types.rs`

Add `pub(crate) is_htmldda: bool` to `JsObjectData` (after `intl_data`). Initialize to `false` in all constructors.

### A2. Create the `$262.IsHTMLDDA` object

**File:** `src/interpreter/builtins/mod.rs`

In the `$262` setup block (~line 388), create a special object:
- Callable (native function that returns `null` when called)
- `is_htmldda: true` on its object data
- Attached to `$262` as `IsHTMLDDA` property

### A3. Modify `typeof_val` for IsHTMLDDA

**File:** `src/interpreter/helpers.rs` (~line 410-430)

In the `JsValue::Object(o)` arm, before checking callable, check `is_htmldda == true` and return `"undefined"`.

### A4. Modify `to_boolean` for IsHTMLDDA

**File:** `src/interpreter/helpers.rs` (~line 36-46)

Change `to_boolean` to accept the `objects` slice (like `typeof_val`), check if object has `is_htmldda`. Update all ~20+ call sites. Currently returns `true` for all `JsValue::Object(_)`.

### A5. Modify `abstract_equality` for IsHTMLDDA

**File:** `src/interpreter/eval.rs` (~line 1787-1852)

Per spec B.3.6.2: if left is Object with `is_htmldda` and right is null/undefined (or vice versa), return true.

### A6-A8. Verify existing behavior

- `??` (nullish coalescing): should NOT special-case IsHTMLDDA (checks value type, not truthiness)
- Destructuring defaults: should NOT trigger for IsHTMLDDA (checks `JsValue::Undefined`)
- IsConstructor: should reject IsHTMLDDA (callable but not constructor)

---

## Part B: Annex B Legacy Syntax Fixes

### B1. Implement `$262.evalScript` (unlocks 13+ scenarios)

**File:** `src/interpreter/builtins/mod.rs`

Add `evalScript` method to `$262`:
1. Takes a string argument
2. Parses as Script
3. Executes in global scope (same environment as calling script)
4. Returns completion value

Unlike `eval()`, this runs as a separate `<script>` tag in the same global scope.

### B2. Fix Annex B function hoisting with default parameters (7 scenarios)

**File:** `src/interpreter/exec.rs`

Per B.3.3.1: if enclosing function has non-simple parameters (defaults, destructuring, rest), Annex B block-level function hoisting should NOT apply. Propagate "has non-simple parameters" to `exec_statements` and `collect_annexb_function_names`.

### B3. Fix Annex B function hoisting with `arguments` name (1 scenario)

**File:** `src/interpreter/exec.rs`

When function named `arguments` is in a block inside a function body, Annex B hoisting should NOT promote to outer var scope because `arguments` is already in `parameterNames`.

### B4. Fix nested block function declarations (1 scenario)

**File:** `src/interpreter/exec.rs`

Function declaration inside nested block should NOT get Annex B treatment if replacing with `var` would cause redeclaration error with outer block-scoped declarations.

### B5. Fix duplicate function declarations in blocks/switch (2 scenarios)

**File:** `src/parser/statements.rs` or `src/interpreter/exec.rs`

In sloppy mode, duplicate function declarations within same block/switch should be allowed.

### B6. Fix global `CreateGlobalVarBinding` property descriptors (12 scenarios)

**File:** `src/interpreter/types.rs`

When Annex B creates global var binding for block-scoped function, `CreateGlobalVarBinding(F, true)` should NOT change enumerability/configurability of existing global property.

### B7. HTML comments in `Function()` constructor (10 scenarios)

**Files:** `src/lexer.rs`, `src/interpreter/builtins/mod.rs`

Per Annex B.1.1: `<!--` treated as single-line comment, `-->` at start of line treated as single-line comment. Support these in non-module Script code and Function constructor bodies.

### B8. Call expression as assignment target (7 scenarios)

**File:** `src/parser/expressions.rs`

Per Annex B, in sloppy mode, `f() = expr` should NOT be parse-time SyntaxError but runtime ReferenceError. Parser should allow CallExpression as assignment target in sloppy mode.

### B9. Fix for-in initializer evaluation (1 scenario)

**File:** `src/interpreter/exec.rs`

`for (var a = expr in obj)` should evaluate initializer before loop begins.

### B10. Add `escape`/`unescape` to global object properties (4 scenarios)

**File:** `src/interpreter/builtins/mod.rs`

Add `"escape"` and `"unescape"` to the `global_names` array (~line 3164).

### B11. Fix `Date.prototype.setYear` / `getFullYear` for negative years (2 scenarios)

**File:** `src/interpreter/builtins/date.rs`

`setYear(-1)` produces correct time but `getFullYear()` returns wrong value.

### B12. Fix `String.prototype.substr` with surrogate pairs (2 scenarios)

**File:** `src/interpreter/builtins/string.rs`

`'\ud834\udf06'.substr(1)` should return `'\udf06'`. Must operate on UTF-16 code units, not Rust chars.

### B13. Fix RegExp Annex B literal parsing (12 scenarios)

**File:** `src/interpreter/builtins/regexp.rs`

- `\c` + digit/underscore inside character classes
- `\C`, `\P`, `\8`, `\9` as identity escapes outside character classes
- Legacy octal escapes in regex
- Non-empty class ranges with non-dash characters
- Quantifiable assertions: `(?=...)` and `(?!...)` followed by quantifiers

---

## Implementation Order

**Phase 1: Quick wins (8 scenarios)**
1. B10 — escape/unescape on global object (4 scenarios, trivial)
2. B11 — Date.setYear/getFullYear fix (2 scenarios)
3. B12 — String.substr surrogate pairs (2 scenarios)

**Phase 2: IsHTMLDDA (48 scenarios)**
4. A1 — Add is_htmldda field
5. A2 — Create $262.IsHTMLDDA object
6. A3 — Modify typeof_val
7. A4 — Modify to_boolean (~20+ call site updates)
8. A5 — Modify abstract_equality
9. A6-A8 — Verify coalesce, destructuring, IsConstructor

**Phase 3: Annex B function hoisting (11 scenarios)**
10. B2 — Default parameter blocks hoisting (7 scenarios)
11. B3 — arguments name blocks hoisting (1 scenario)
12. B4 — Nested block function declarations (1 scenario)
13. B5 — Duplicate function declarations (2 scenarios)

**Phase 4: evalScript + global code (15 scenarios)**
14. B1 — Implement $262.evalScript
15. B6 — Global CreateGlobalVarBinding descriptors (12 scenarios)
16. B15 — eval-code lex collision (2 scenarios)

**Phase 5: Parser/Lexer changes (17 scenarios)**
17. B7 — HTML comments in Function constructor (10 scenarios)
18. B8 — Call expression as runtime ReferenceError (7 scenarios)

**Phase 6: RegExp Annex B (12 scenarios)**
19. B13 — RegExp literal parsing fixes

---

## Critical Files

- `src/interpreter/types.rs` — Add `is_htmldda` to JsObjectData
- `src/interpreter/helpers.rs` — Modify `typeof_val`, `to_boolean`, `abstract_equality`
- `src/interpreter/builtins/mod.rs` — Create `$262.IsHTMLDDA`, implement `$262.evalScript`, fix global_names
- `src/interpreter/exec.rs` — Annex B function hoisting fixes, for-in initializer
- `src/lexer.rs` — HTML comment support
- `src/parser/expressions.rs` — Call expression as sloppy-mode assignment target
- `src/interpreter/builtins/regexp.rs` — RegExp Annex B patterns
- `src/interpreter/builtins/date.rs` — setYear fix
- `src/interpreter/builtins/string.rs` — substr fix
