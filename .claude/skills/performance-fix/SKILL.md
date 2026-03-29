---
name: performance-fix
description: >-
  This skill should be used when the user asks to "fix performance issues",
  "optimise the bottlenecks", "implement the performance fixes",
  "apply the performance report", "speed this up based on the report",
  or wants to act on a performance report produced by /performance-analyse.
  Reads a performance report and implements targeted fixes.
version: 0.1.0
---

# Performance Engineer

Fix exactly **one** performance bottleneck per invocation. If the target issue is unclear, interview the user before writing any code.

## Workflow

1. **Identify the target** — Determine which single bottleneck to fix:
   - If the user named a specific hotspot or issue number from the report, use that
   - If the user's request is vague ("fix performance", "speed this up"), load `perf-report.md` and **ask the user which hotspot to tackle** — list the hotspots with their severity and let them choose
   - If `perf-report.md` does not exist or does not contain the issue the user describes, **interview the user**: ask what is slow, how they observe it, what workload triggers it, and what "fast enough" means — then suggest running `/performance-analyse` first
   - Never guess which issue to fix. Always confirm the target with the user before writing code.
2. **Fix one bottleneck:**
   a. Understand the root cause from the report (or from the user interview)
   b. Implement a minimal, targeted fix
   c. Build in release mode
   d. Re-measure using the same benchmark from the report
   e. Record before/after numbers
   f. If improvement is <2% or not statistically significant, reconsider the approach
3. **Update report** — Append the fix result to `perf-report.md` with before/after measurements
4. **Done** — Present the result. Do not proceed to the next bottleneck unless the user explicitly asks

## Fix Strategies by Bottleneck Type

### Allocation-bound
| Symptom | Fix |
|---------|-----|
| Repeated `Vec` growth | `Vec::with_capacity()` or `reserve()` |
| `format!`/`to_string` in hot path | Pre-allocate buffer, use `write!` |
| Many small short-lived allocations | Arena allocator (`bumpalo`), `SmallVec` |
| Excessive cloning | Borrow instead, `Cow<'_, T>`, `Rc`/`Arc` |

### CPU-bound
| Symptom | Fix |
|---------|-----|
| O(n^2) algorithm | Replace with O(n log n) or O(n) |
| Missed auto-vectorization | Restructure loop, use iterators |
| Redundant computation | Cache/memoize results, `LazyLock` for statics |
| Dynamic dispatch in hot loop | Enum dispatch or monomorphize with generics |

### Memory-bound (cache misses)
| Symptom | Fix |
|---------|-----|
| AoS with partial field access | Restructure to SoA |
| Pointer chasing (linked list, tree) | Flatten to `Vec`, use indices |
| Large structs in hot iteration | Split hot/cold fields |
| Random access patterns | Sort data for sequential access |

### Branch-bound
| Symptom | Fix |
|---------|-----|
| Unpredictable branches in tight loop | Branchless arithmetic |
| Trait object dispatch | Enum dispatch |
| Frequent bounds checks | Use iterators, `chunks_exact` |

## Re-measurement Protocol

After each fix:

1. Build with `cargo build --release`
2. Run the exact same workload/benchmark from the report
3. Compare against the report's baseline (not the previous fix)
4. Record: function/area, what changed, before time, after time, speedup factor
5. If criterion is available, check that `p < 0.05` (statistically significant)

## Results Format

Append to the report:

```markdown
## Fix Results

### Fix: <description>
- **Bottleneck:** Hotspot N from report
- **Change:** <what was modified>
- **Before:** X.XXms (median)
- **After:** X.XXms (median)
- **Speedup:** X.Xx
- **Commit:** <hash>
```

## Principles

- **One fix per invocation** — fix exactly one bottleneck, then stop. The user invokes again for the next one.
- **Clarify before coding** — if the target is ambiguous or missing from the report, ask. Never assume.
- **Minimal changes** — change only what the bottleneck requires; do not refactor surrounding code
- **Data over intuition** — if the measurement shows no improvement, revert
- **No premature optimisation** — only fix what the report identifies; do not speculatively optimise other areas

## Additional Resources

- **`references/methodologies.md`** — Statistical benchmarking, allocation reduction patterns, cache optimisation strategies, and detailed code examples
