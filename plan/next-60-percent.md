# Plan: Reach 60% test262 (from 58.61%)

Current: 28,867 / 47,458 (60.83%).

## Phase 1: Private Name Unicode Escapes — ✅ Done (+799 passes, 58.61% → 60.29%)

Fixed both match blocks in `src/lexer.rs` to handle `Token::IdentifierWithEscape(s)` in private name parsing.

## Phase 2: BigInt Support — ✅ Done (+253 passes, 60.29% → 60.83%)

Implemented full BigInt runtime support:
- 2a. BigInt literal evaluation (parse hex/oct/bin/dec BigInt literals)
- 2b. Binary operator dispatch (arithmetic, bitwise, shift with type checking)
- 2c. Unary operator support (-, ~, TypeError on +)
- 2d. Equality operators (strict, abstract with BigInt/Number/String coercion)
- 2e. BigInt constructor & prototype (toString, valueOf, toLocaleString, asIntN, asUintN)
- 2f. Type coercion updates (increment/decrement, relational comparison, to_boolean for 0n)
- Fixed Number() constructor to handle BigInt argument
- Wired bigint_prototype for property access on BigInt primitives

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
