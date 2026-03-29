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

Implement targeted fixes for performance bottlenecks identified in a report. This role modifies code: **one fix at a time, re-measure after each.**

## Workflow

1. **Read report** — Load `perf-report.md` (or a user-specified report file) and parse the recommendations
2. **Prioritise** — Work through bottlenecks in the order listed (highest impact first)
3. **For each bottleneck:**
   a. Understand the root cause from the report
   b. Implement a minimal, targeted fix
   c. Build in release mode
   d. Re-measure using the same benchmark from the report
   e. Record before/after numbers
   f. If improvement is <2% or not statistically significant, reconsider the approach
4. **Update report** — Append results to the report with before/after measurements
5. **Summarise** — Present total improvement vs original baseline

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

### Fix 1: <description>
- **Bottleneck:** Hotspot N from report
- **Change:** <what was modified>
- **Before:** X.XXms (median)
- **After:** X.XXms (median)
- **Speedup:** X.Xx
- **Commit:** <hash>

### Fix 2: ...

## Overall

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| Wall-clock (median) | X.XXs | X.XXs | X.Xx faster |
```

## Principles

- **One fix at a time** — never bundle multiple optimisations in one step; each must be independently measured
- **Minimal changes** — change only what the bottleneck requires; do not refactor surrounding code
- **Data over intuition** — if the measurement shows no improvement, revert
- **Stop when done** — if the target metric is met or no dominant hotspot remains, stop
- **No premature optimisation** — only fix what the report identifies; do not speculatively optimise other areas

## Additional Resources

- **`references/methodologies.md`** — Statistical benchmarking, allocation reduction patterns, cache optimisation strategies, and detailed code examples
