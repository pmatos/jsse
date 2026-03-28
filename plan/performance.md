# JSSE Performance Optimization Plan

**Baseline date:** 2026-03-28
**Baseline commit:** `845ad427cc7e74fb2fbf1d74446d1c72417112e5`
**test262 pass rate:** 100% (99,148/99,148)

## Baseline Benchmark Summary

### SunSpider 1.0.2 (26 tests, all pass on all engines)

| Engine | Total | vs Node | vs Boa |
|--------|-------|---------|--------|
| Node.js | 605ms | 1.0x | — |
| Boa | 2,912ms | 4.8x | 1.0x |
| JSSE | 83,512ms | 138x | 29x |
| engine262 | 786,181ms | 1,299x | 270x |

### Kraken 1.1

| Engine | Passed | Total (passing) |
|--------|--------|----------------|
| Node.js | 14/14 | 863ms |
| Boa | 14/14 | 74,994ms |
| JSSE | 12/14 | 326,500ms |
| engine262 | 2/14 | 8,292ms |

### Octane 2.0

| Engine | Passed | Total (passing) |
|--------|--------|----------------|
| Node.js | 14/14 | 24,472ms |
| Boa | 14/14 | 106,652ms |
| JSSE | 10/14 | 383,051ms |
| engine262 | 3/14 | 92,715ms |

---

## Phase 1: String Concatenation

**Target benchmarks:** string-unpack-code (115x vs Boa), string-tagcloud (67x), string-validate-input (25x)

**Root cause:** O(n²) string building. Every `+=` on strings clones the entire left operand.

**Code locations:**
- `src/interpreter/helpers.rs:105-110` — `js_value_to_code_units()` clones `s.code_units` via `.clone()`
- `src/interpreter/eval.rs:1927-1928` — BinaryOp::Add creates a new `Vec<u16>` from cloned left, then extends with cloned right

**Fix strategy:** Avoid cloning the left operand when it's consumed by the `+=` operation. Options:
- Take ownership of the left `Vec<u16>` instead of cloning when possible
- Use `Cow<[u16]>` or similar to defer cloning
- Consider a rope or builder for repeated concatenation patterns

**Expected impact:** 10-50x improvement on string-heavy SunSpider benchmarks. Should cut SunSpider total by ~60%.

**Status:** Not started

**Post-phase results:**
- SunSpider total: _pending_
- test262 pass rate: _pending_

---

## Phase 2: Function Call Overhead

**Target benchmarks:** controlflow-recursive (25x vs Boa), access-binary-trees (9x), access-fannkuch (10.5x)

**Root cause:** Per-call overhead in environment creation, argument binding, proxy trap checks, and GC rooting.

**Code locations:**
- `src/interpreter/eval.rs:4749-5509` — `eval_call` function
- Environment creation, arguments object construction per call
- Proxy trap checking even for plain functions
- `gc_root_value` / `gc_unroot_value` on every argument

**Fix strategy:**
- Skip proxy checks for known non-proxy functions (fast path)
- Pre-allocate or pool environment objects
- Avoid arguments object creation when not referenced
- Reduce GC root/unroot overhead for short-lived call frames

**Expected impact:** 2-5x improvement on recursive benchmarks.

**Status:** Not started

**Post-phase results:**
- SunSpider total: _pending_
- Kraken total: _pending_
- test262 pass rate: _pending_

---

## Phase 3: Regular Expressions

**Target benchmarks:** regexp-dna (83x vs Boa, 1.5x vs engine262)

**Root cause:** JSSE's regex engine has excessive overhead — even engine262 (JS-on-JS) beats it.

**Code locations:**
- `src/interpreter/builtins/string.rs:845-962` — `String.prototype.replace` implementation
- Linear UTF-16 search pattern at lines 890-903
- Multiple string conversions between UTF-8 and UTF-16

**Fix strategy:**
- Profile the regex engine to identify specific bottlenecks
- Reduce unnecessary UTF-8/UTF-16 conversions
- Consider using the `regex` crate as a backend for common patterns
- Optimize `String.prototype.replace` for the common single-match case

**Expected impact:** 5-10x improvement on regex-heavy benchmarks.

**Status:** Not started

**Post-phase results:**
- SunSpider total: _pending_
- test262 pass rate: _pending_

---

## Phase 4: Numeric Loop Overhead

**Target benchmarks:** math-cordic (16x vs Boa), crypto-md5 (19x), access-fannkuch (10.5x), bitops-* (10-17x)

**Root cause:** Per-AST-node walk overhead in tight arithmetic loops. Every operation goes through the full eval_expr dispatch, type checking, and value boxing/unboxing.

**Code locations:**
- `src/interpreter/eval.rs:295-430` — `eval_expr` match statement
- `src/interpreter/eval.rs:352-389` — BinaryOp handling with type coercion per operation

**Fix strategy:**
- Specialize numeric binary operations to avoid repeated type checks when both operands are known numbers
- Consider caching the "is this value a number" check
- Reduce JsValue boxing/unboxing in arithmetic hot paths
- Fast-path for common patterns like `i++`, `i < n`, `a[i]` in for loops

**Expected impact:** 2-3x improvement on arithmetic-heavy benchmarks.

**Status:** Not started

**Post-phase results:**
- SunSpider total: _pending_
- Kraken total: _pending_
- Octane total: _pending_
- test262 pass rate: _pending_

---

## Progress Tracking

After each phase:
1. `cargo build --release`
2. Run test262: `uv run python scripts/run-test262.py` — must remain at 100%
3. Run benchmarks: `uv run scripts/run-benchmarks.py --suite sunspider kraken octane --engines jsse node boa engine262 --timeout 120 --repetitions 3`
4. Update this file with results
5. Commit with benchmark data

| Phase | SunSpider | Kraken (passing) | Octane (passing) | test262 |
|-------|-----------|-----------------|------------------|---------|
| Baseline | 83,512ms | 326,500ms | 383,051ms | 100% |
| Phase 1 | _pending_ | _pending_ | _pending_ | _pending_ |
| Phase 2 | _pending_ | _pending_ | _pending_ | _pending_ |
| Phase 3 | _pending_ | _pending_ | _pending_ | _pending_ |
| Phase 4 | _pending_ | _pending_ | _pending_ | _pending_ |
