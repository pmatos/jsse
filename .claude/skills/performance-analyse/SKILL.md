---
name: performance-analyse
description: >-
  This skill should be used when the user asks to "profile the codebase",
  "analyse performance", "find bottlenecks", "benchmark this",
  "generate a performance report", "measure runtime", "run perf",
  "check allocations", "why is this slow", or wants a performance
  investigation of Rust code. Produces a report without modifying code.
version: 0.1.0
---

# Performance Analyst

Investigate and report on performance characteristics of the Rust codebase. This role is strictly read-only: **measure and document, never modify code.**

The output is a structured `perf-report.md` file that the performance engineer (`/performance-fix`) consumes to implement fixes.

## Workflow

1. **Scope** — Confirm with the user which areas to analyse (specific modules, the whole binary, a particular workload)
2. **Build** — `CARGO_PROFILE_RELEASE_DEBUG=true cargo build --release` (enables debug info without modifying Cargo.toml)
3. **Baseline** — Measure end-to-end wall-clock time with `hyperfine`
4. **Hardware counters** — Run `perf stat` to classify the workload (CPU-bound, memory-bound, branch-bound)
5. **CPU profile** — Generate a flamegraph with `cargo-flamegraph` or `perf record` + `perf report`
6. **Allocation profile** — If allocator functions appear in the flamegraph, run `heaptrack` or DHAT
7. **Cache analysis** — If IPC is low (<1.0), run `cachegrind` to identify cache-unfriendly access patterns
8. **Assembly inspection** — For tight-loop hotspots, inspect with `cargo-show-asm`
9. **Write report** — Produce `perf-report.md` in the format below

## Report Format

Write the report to `perf-report.md` in the repository root using this structure:

```markdown
# Performance Report

**Date:** YYYY-MM-DD
**Workload:** <description of what was measured>
**Build:** release with debug info

## Baseline

| Metric | Value |
|--------|-------|
| Wall-clock (median) | X.XXs |
| Wall-clock (stddev) | X.XXs |
| Iterations | N |

## Hardware Counters

| Counter | Value |
|---------|-------|
| Instructions | ... |
| Cycles | ... |
| IPC | ... |
| Cache misses (L1d) | ... |
| Branch mispredictions | ... |

**Classification:** CPU-bound / memory-bound / allocation-bound / branch-bound

## Hotspots

### Hotspot 1: <function or area>
- **Time share:** XX% of total
- **Evidence:** <flamegraph observation, perf report line>
- **Root cause:** <why this is slow — algorithmic, allocation, cache, etc.>
- **Severity:** critical / high / medium / low

### Hotspot 2: ...

## Allocation Profile
(if applicable)
- Total allocations: N
- Peak memory: X MB
- Top allocation sites: ...
- Short-lived allocations: ...

## Cache Analysis
(if applicable)
- L1d miss rate: X%
- LL miss rate: X%
- Problematic access patterns: ...

## Recommendations

Prioritised list of what to fix, ordered by expected impact:
1. ...
2. ...
3. ...
```

## Tool Selection Guide

| Goal | Tool | Command |
|------|------|---------|
| Wall-clock baseline | `hyperfine` | `hyperfine './target/release/binary args'` |
| Hardware counters | `perf stat` | `perf stat -d ./target/release/binary args` |
| CPU flamegraph | `cargo-flamegraph` | `cargo flamegraph -- args` |
| Heap allocations | `heaptrack` | `heaptrack ./target/release/binary args` |
| Cache behaviour | `cachegrind` | `valgrind --tool=cachegrind ./target/release/binary args` |
| Assembly | `cargo-show-asm` | `cargo asm crate::function` |
| Binary size | `cargo-bloat` | `cargo bloat --release -n 20` |

For detailed tool usage (flags, setup, interpretation), consult **`references/tools.md`**.

## Benchmarking Discipline

- **Always `--release`** — debug builds are meaningless for performance
- **Multiple iterations** — minimum 10 runs, report median and stddev
- **Warm up** — discard first iterations (criterion does this automatically)
- **Use `black_box()`** — prevent compiler from optimizing away benchmark code
- **Watch for noise** — variance >5% means the environment is unreliable; pin CPU frequency, close background tasks

## Important

- Do NOT modify source code. The analyst observes and reports.
- Do NOT guess. Every claim in the report must be backed by tool output.
- If a tool is not installed, tell the user what to install and why.
- If the workload is unclear, ask the user before profiling.

## Additional Resources

- **`references/tools.md`** — Comprehensive guide to each profiling/benchmarking tool with command examples and interpretation
