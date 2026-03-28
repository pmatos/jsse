---
name: performance-specialist
description: >-
  This skill should be used when the user asks to "profile Rust code",
  "benchmark this", "find performance bottlenecks", "optimize performance",
  "analyze flamegraph", "measure runtime", "check allocations",
  "run perf", "cargo bench", "why is this slow", or mentions
  performance analysis, profiling, or benchmarking in a Rust codebase.
version: 0.1.0
---

# Rust Performance Specialist

A structured approach to performance analysis in Rust. The cardinal rule: **measure first, then optimize**. Never guess where time is spent -- prove it with data.

## Core Workflow

Every performance investigation follows this cycle:

1. **Establish baseline** -- Measure current performance with reproducible benchmarks
2. **Profile** -- Identify where time/memory is actually spent
3. **Analyze** -- Understand *why* the hotspot exists
4. **Optimize** -- Make a targeted, minimal change
5. **Re-measure** -- Verify improvement against the baseline
6. **Repeat** -- Go back to step 2 if further gains are needed

Never skip step 1. An optimization without a baseline is just a guess.

## Measurement Tools (Quick Reference)

| Goal | Tool | Install |
|------|------|---------|
| Microbenchmarks | `criterion` / `cargo bench` | `cargo add --dev criterion` |
| Wall-clock timing | `hyperfine` | `cargo install hyperfine` |
| CPU profiling | `perf record` + `perf report` | system package (`linux-tools`) |
| Flame graphs | `cargo-flamegraph` | `cargo install flamegraph` |
| Heap allocations | `heaptrack` / DHAT | system package / valgrind |
| Cache behavior | `cachegrind` | valgrind suite |
| Call counts | `callgrind` | valgrind suite |
| Binary size | `cargo-bloat` | `cargo install cargo-bloat` |
| Generated assembly | `cargo-show-asm` | `cargo install cargo-show-asm` |
| LLVM IR codegen | `cargo-llvm-lines` | `cargo install cargo-llvm-lines` |

For detailed usage of each tool, consult **`references/tools.md`**.

## Choosing the Right Approach

### "This function is slow"
1. Write a criterion benchmark isolating the function
2. Run `cargo flamegraph` on the benchmark to find the hotspot
3. Inspect assembly with `cargo-show-asm` if the bottleneck is in tight loops

### "The whole program is slow"
1. Measure end-to-end with `hyperfine` to establish a baseline
2. Profile with `perf record` + flamegraph to find top-level hotspots
3. Drill into the dominant hotspot

### "It uses too much memory"
1. Run under `heaptrack` to track allocation counts and sizes
2. Look for allocation-heavy call sites
3. Consider arena allocation, `SmallVec`, or pre-allocation

### "It's slow but I don't know where"
1. Start broad: `perf stat` for hardware counters (IPC, cache misses, branch mispredicts)
2. If IPC is low: likely memory-bound -- run `cachegrind`
3. If branch mispredicts are high: look at conditional-heavy code paths
4. If instructions are high: CPU-bound -- flamegraph to find hotspot

## Benchmarking Discipline

- **Always build with `--release`** -- debug builds are meaningless for performance
- **Warm up** -- criterion handles this automatically; for manual benchmarks, discard first iterations
- **Use `black_box()`** -- prevent the compiler from optimizing away benchmark code
- **Control the environment** -- pin CPU frequency, close background processes, run multiple iterations
- **Report statistics** -- mean, median, stddev, and confidence intervals (criterion provides these)
- **Watch for noise** -- if variance exceeds 5%, the benchmark environment is unreliable

## Common Rust Performance Pitfalls

| Pitfall | Symptom | Fix |
|---------|---------|-----|
| Excessive cloning | Allocator-heavy flamegraph | Borrow instead, use `Cow<'_, T>`, or `Rc`/`Arc` |
| Unbounded `Vec` growth | Sawtooth memory pattern | `Vec::with_capacity()` or `reserve()` |
| Hash map overhead | `HashMap` in hot path | Try `FxHashMap`, `IndexMap`, or sorted `Vec` |
| Dynamic dispatch in hot loop | vtable calls in flamegraph | Monomorphize with generics or enum dispatch |
| String formatting in hot path | `format!`/`to_string` allocations | Pre-allocate, use `write!` to a buffer |
| Unnecessary bounds checks | Panic infrastructure in assembly | Use iterators, `get_unchecked` (unsafe, last resort) |

## Communicating Results

When presenting performance findings:

1. **State the baseline**: "Function X takes 4.2ms per call (p50), 6.1ms (p99)"
2. **Show the evidence**: flamegraph screenshot, perf output, criterion report
3. **Identify the root cause**: "78% of time is in `alloc::vec::Vec<T>::push` due to repeated reallocations"
4. **Propose the fix**: "Pre-allocate with `Vec::with_capacity(expected_size)`"
5. **Show the improvement**: "After fix: 1.1ms per call (p50) -- 3.8x improvement"

Always present before/after numbers. Relative improvements (percentages, speedup factors) are more meaningful than absolute numbers alone.

## Additional Resources

### Reference Files

For detailed tool usage and methodology deep-dives, consult:
- **`references/tools.md`** -- Comprehensive guide to each profiling/benchmarking tool with command examples
- **`references/methodologies.md`** -- Statistical benchmarking, cache analysis, allocation profiling, and advanced optimization strategies
