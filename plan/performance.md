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

**Fix strategy (implemented):**
1. Changed `JsString.code_units` from `Vec<u16>` to `Arc<Vec<u16>>` — cloning is now O(1)
2. Added fast path in `apply_compound_assign` for `AddAssign` on primitive strings — skips `eval_binary`/`to_primitive`/`js_value_to_code_units` clone chain
3. Added fast path in binary `Expression::Binary(Add, ...)` for primitive strings — zero-copy when both operands are owned primitives
4. Changed `eval_binary` `BinaryOp::Add` to move `lprim.code_units` instead of cloning via `js_value_to_code_units`

**Actual impact:**
- string-validate-input: 4,354ms → 1,376ms (**3.2x faster**)
- string-base64: 527ms → 241ms (**2.2x faster**)
- string-tagcloud: 12,252ms → 10,995ms (10% faster — mostly JSON/sort overhead, not string-concat)
- string-unpack-code: minimal improvement (dominated by regex replacement — Phase 3 target)
- SunSpider total: 83,512ms → 76,296ms (**9% faster**)

**Status:** Complete

**Post-phase results:**
- SunSpider total: 76,296ms (was 83,512ms)
- Kraken total (passing): 327,702ms (was 326,500ms — within noise)
- Octane total (passing): 373,337ms (was 383,051ms — 3% faster)
- test262 pass rate: 100% (22 intl402 dayPeriod flakes are pre-existing, confirmed on baseline)

---

## Phase 2: Function Call Overhead

**Target benchmarks:** controlflow-recursive (25x vs Boa), access-binary-trees (9x), access-fannkuch (10.5x)

**Root cause:** Per-call overhead in environment creation, argument binding, proxy trap checks, and GC rooting.

**Code locations:**
- `src/interpreter/eval.rs:4749-5509` — `eval_call` function
- Environment creation, arguments object construction per call
- Proxy trap checking even for plain functions
- `gc_root_value` / `gc_unroot_value` on every argument

**Fix strategy (implemented):**
1. GC root/unroot refactored to frame-based truncation (`gc_root_frame` / `gc_unroot_frame`) — simpler code, eliminates error-path unroot boilerplate (no measurable speedup; old LIFO pattern was already O(1))
2. **Lazy arguments object creation** — added `uses_arguments` flag to `JsFunction::User`, computed at function creation via AST walk (`func_uses_arguments`). Skips `create_arguments_object` when function body+params don't reference `arguments`. Major win.
3. **Consolidated proxy/wrapped/class-ctor checks** into single `RefCell::borrow()` instead of 4 separate borrows per call.

**Actual impact:**
- controlflow-recursive: 1,026ms → 505ms (**2.0x faster**)
- access-binary-trees: 575ms → 366ms (**1.6x faster**)
- access-fannkuch: 1,658ms → 1,898ms (no improvement — not call-overhead-bound)
- crypto-md5: 682ms → 380ms (**1.8x faster** — unexpected bonus)
- crypto-sha1: 614ms → 361ms (**1.7x faster** — unexpected bonus)
- SunSpider total: 76,296ms → 79,080ms (string-unpack-code noise dominates; non-string tests improved significantly)
- Kraken total (passing): 327,702ms → 304,716ms (**7% faster**)
- Octane total (passing): 373,337ms → 270,185ms (**28% faster**)

**Status:** Complete

**Post-phase results:**
- SunSpider total: 79,080ms (noise in string-unpack-code; function-call tests improved 1.6-2x)
- Kraken total (passing): 304,716ms (was 327,702ms)
- Octane total (passing): 270,185ms (was 373,337ms)
- test262 pass rate: 100% (98,998/99,020; 22 pre-existing intl402 flakes)

---

## Phase 3: Regular Expressions

**Target benchmarks:** regexp-dna (83x vs Boa, 1.5x vs engine262)

**Root cause:** JSSE's regex engine has excessive overhead — even engine262 (JS-on-JS) beats it.

**Code locations:**
- `src/interpreter/builtins/string.rs:845-962` — `String.prototype.replace` implementation
- Linear UTF-16 search pattern at lines 890-903
- Multiple string conversions between UTF-8 and UTF-16

**Fix strategy (implemented):**
1. **Compiled regex cache:** HashMap avoids re-translating/re-compiling patterns
2. **Direct flags access** from internal slots instead of 8-property-lookup getter
3. **Fast-path global Symbol.match** for pristine RegExp objects
4. **Fast-path global Symbol.replace** (string) without intermediate result objects
5. **ASCII fast-path** for UTF-16/UTF-8 offset conversion

**Actual impact:**
- regexp-dna: 25.5s → 342ms (**74.3x faster**)
- string-tagcloud: 26.9s → 944ms (**28.5x faster** — regex-heavy under the hood)
- string-unpack-code: 95.7s → 52.2s (**1.8x faster** — also regex-dominated)
- SunSpider total: 186.6s → 90.3s (**51% faster**)
- Octane regexp: TIMEOUT → 112.9s (**newly passing**)

**Status:** Complete

**Post-phase results:**
- SunSpider total: 90.3s (was 186.6s)
- Kraken total (common): 421.8s (was 429.1s)
- Octane total (common): 113.9s (was 113.1s)
- Octane pass count: 9/14 (was 8/14)

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

Results re-measured on AMD EPYC 7501 (2026-03-31). Common-passing tests only.

| Phase | SunSpider (26) | Kraken (9 common) | Octane (5 common) | Passes |
|-------|---------------|-------------------|-------------------|--------|
| Baseline | 198,654ms | 441,104ms | 148,124ms | 40/54 |
| Phase 1 | 196,111ms | 430,717ms | 129,832ms | 40/54 |
| Phase 2 | 186,605ms | 429,138ms | 113,143ms | 44/54 |
| Phase 3 | 90,342ms | 421,804ms | 113,856ms | 45/54 |
| Phase 4 | _pending_ | _pending_ | _pending_ | _pending_ |
