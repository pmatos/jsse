# Plan: Reach 60% test262 (from 58.61%)

Current: 28,614 / 47,458 (60.29%). ✅ Hit 60% target!

## Phase 1: Private Name Unicode Escapes — ✅ Done (+799 passes, 58.61% → 60.29%)

Fixed both match blocks in `src/lexer.rs` to handle `Token::IdentifierWithEscape(s)` in private name parsing.

## Phase 2: BigInt Support (~300+ tests)

The type system (`JsBigInt`), operations module (all arithmetic/bitwise), lexer, and parser are already implemented. What's missing is the runtime glue:

### 2a. BigInt literal evaluation
- `src/interpreter/eval.rs` line 635: `Literal::BigInt(_) => JsValue::Undefined` → parse string to `JsValue::BigInt`

### 2b. Binary operator dispatch
- `src/interpreter/eval.rs` lines ~973-1059: For arithmetic (+, -, *, /, %, **), bitwise (&, |, ^, <<, >>), and comparison (<, >, <=, >=) operators, add BigInt type checks before the existing Number path. Throw TypeError on mixed BigInt/Number.

### 2c. Unary operator support
- `src/interpreter/eval.rs` lines ~728-734: Handle unary `-` and `~` for BigInt. Throw TypeError for unary `+` on BigInt (per spec).

### 2d. Equality operators
- `abstract_equality()` and `strict_equality()`: Add BigInt === BigInt, BigInt == Number (coerce), BigInt == String (parse) cases.

### 2e. BigInt constructor & prototype
- Add `setup_bigint_prototype()` in builtins: `BigInt.prototype.toString()`, `.valueOf()`, `.toLocaleString()`, `BigInt.asIntN()`, `BigInt.asUintN()`
- Register `BigInt()` as a callable (not constructable) global

### 2f. Type coercion updates
- `to_numeric()`: return BigInt as-is (not convert to Number)
- `to_primitive()`: handle BigInt wrapper objects
- Increment/decrement operators: support BigInt

**Tests affected:** `test262/test/built-ins/BigInt/` (75), language bigint tests (~207), plus TypedArray BigInt tests (~362 from BigInt64Array/BigUint64Array fix).
**Estimated gain: ~300-400 tests** (conservative, excluding TypedArray which already partially works)

## Phase 3: Array Method Robustness (~150-200 tests)

### 3a. Array-like object support
Many Array.prototype methods fail when called on non-array objects via `.call()`. Fix `reduce`, `reduceRight`, `every`, `some`, `map`, `filter`, `forEach`, `find`, `findIndex`, `indexOf`, `lastIndexOf` to use `ToObject(this)` + `LengthOfArrayLike` instead of assuming `this` is an array.

### 3b. TypeError/RangeError throws
- Throw TypeError when callback is not callable
- Throw TypeError for non-object `this` in strict mode
- Throw RangeError for invalid array lengths in constructors and `push`/`unshift`

### 3c. Array.prototype.toLocaleString
Implement missing method (delegates to each element's `.toLocaleString()`).

### 3d. Symbol.isConcatSpreadable
Add check in `concat` implementation.

**Estimated gain: ~150-200 tests**

## Execution Order

1. Phase 1 (private names) — smallest change, biggest per-line impact
2. Phase 2a-2c (BigInt literals + operators) — unlock most BigInt tests
3. Phase 2d-2e (BigInt equality + constructor) — complete BigInt
4. Phase 3a-3d (Array robustness) — incremental gains

## Verification

After each phase:
- `cargo build --release`
- `uv run python scripts/run-test262.py` (full suite)
- Verify no regressions against `test262-pass.txt`
- Update README.md and PLAN.md with new counts

## Expected Outcome

~835-985 new passes → ~28,650-28,800 / 47,458 → **60.4%-60.7%**
