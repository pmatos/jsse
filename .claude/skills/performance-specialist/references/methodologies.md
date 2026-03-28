# Performance Analysis Methodologies

Structured approaches to diagnosing and resolving performance issues in Rust programs.

---

## The Measure-First Principle

**Never optimize without a measurement.** Intuition about performance is wrong more often than right. The workflow is always:

1. Define what "fast enough" means (target metric, acceptable latency, throughput goal)
2. Measure current performance with a reproducible benchmark
3. Profile to find the actual bottleneck
4. Fix the bottleneck
5. Re-measure to confirm improvement
6. Stop when the target is met

**Anti-patterns to avoid:**
- Optimizing code that isn't the bottleneck
- Optimizing before measuring ("this looks slow")
- Measuring once and treating it as truth (need statistical significance)
- Optimizing debug builds (always `--release`)

---

## Statistical Benchmarking

### Why statistics matter

A single timing measurement is noise. System jitter (context switches, cache state, frequency scaling) means any single run can vary by 10-30%. Reliable benchmarking requires:

- **Multiple iterations** -- at minimum 10, ideally 100+
- **Warm-up** -- discard initial iterations (cold caches, JIT if applicable)
- **Outlier detection** -- identify and handle measurements distorted by system events
- **Confidence intervals** -- report uncertainty, not just a point estimate

### Interpreting criterion output

```
my_func     time:   [1.234 ms 1.256 ms 1.279 ms]
                     change: [-3.2% -1.8% -0.3%] (p = 0.02 < 0.05)
                     Performance has improved.
```

- `[1.234 ms 1.256 ms 1.279 ms]` -- lower bound, estimate, upper bound of the mean
- `change: [-3.2% -1.8% -0.3%]` -- confidence interval of change vs baseline
- `p = 0.02` -- p-value; below 0.05 means the change is statistically significant
- If the confidence interval straddles zero (e.g., `[-1.2% +0.8%]`), the change is noise

### Environment control

For reliable benchmarks:

```bash
# Pin CPU frequency (prevents turbo boost variance)
sudo cpupower frequency-set -g performance

# Disable address space layout randomization
echo 0 | sudo tee /proc/sys/kernel/randomize_va_space

# Isolate CPU cores (boot parameter or cset)
taskset -c 2 ./target/release/my_program

# Close background processes, especially browsers and IDEs
```

When unable to control the environment, run more iterations and accept wider confidence intervals.

---

## Profiling-Driven Optimization

### The top-down approach

1. **Start broad**: `perf stat` for hardware counters
   - Look at IPC (instructions per cycle): below 1.0 is likely memory-bound; above 2.0 is compute-efficient
   - Check cache miss rates and branch misprediction rates

2. **Narrow with flamegraph**: identify the widest stacks
   - Focus on the top 1-3 hotspots; everything else is noise
   - If a single function dominates (>50%), that's the target

3. **Drill into assembly**: `cargo-show-asm` on the hot function
   - Check for vectorization, bounds checks, unexpected function calls

4. **Measure the specific path**: criterion benchmark isolating the hotspot

### Common bottleneck categories

**CPU-bound (high IPC, high instruction count):**
- Algorithmic complexity -- O(n^2) where O(n log n) exists
- Tight loop inefficiency -- missed auto-vectorization, redundant work
- Fix: algorithm change, SIMD, loop restructuring

**Memory-bound (low IPC, high cache miss rate):**
- Poor data locality -- linked lists, pointer chasing
- Working set exceeds cache -- data too large or access pattern too scattered
- Fix: data-oriented design, SoA vs AoS, cache-oblivious algorithms, smaller types

**Allocation-bound (allocator functions dominate flamegraph):**
- Excessive heap allocations in hot path
- `Vec` growth without pre-allocation
- Temporary `String`/`Vec` creation in loops
- Fix: pre-allocate, arena allocator, stack allocation (`SmallVec`, `ArrayVec`), reuse buffers

**Branch-bound (high branch misprediction rate):**
- Unpredictable conditional branches in tight loops
- Virtual dispatch (trait objects) in hot paths
- Fix: branchless algorithms, sorting data to improve prediction, monomorphization

---

## Allocation Analysis

### Identifying allocation problems

Symptoms:
- `alloc::` or `__rust_alloc` prominent in flamegraph
- `heaptrack` shows millions of small, short-lived allocations
- Performance degrades with larger inputs more than expected

### Reduction strategies

**Pre-allocation:**
```rust
// Before: Vec grows incrementally
let mut v = Vec::new();
for item in input {
    v.push(process(item));
}

// After: single allocation
let mut v = Vec::with_capacity(input.len());
for item in input {
    v.push(process(item));
}

// Even better: iterator collect
let v: Vec<_> = input.iter().map(process).collect();
// collect() calls size_hint() to pre-allocate
```

**Buffer reuse:**
```rust
// Before: new String each iteration
for item in items {
    let s = format!("{}: {}", item.key, item.value);
    process(&s);
}

// After: reuse buffer
let mut buf = String::with_capacity(256);
for item in items {
    buf.clear();
    write!(&mut buf, "{}: {}", item.key, item.value).unwrap();
    process(&buf);
}
```

**Stack allocation for small collections:**
```rust
use smallvec::SmallVec;

// Stays on stack for <= 8 elements, spills to heap beyond
let mut items: SmallVec<[Item; 8]> = SmallVec::new();
```

**Arena allocation:**
```rust
use bumpalo::Bump;

let arena = Bump::new();
// All allocations from this arena are freed together
for item in input {
    let data = arena.alloc(process(item));
}
// Single deallocation when arena is dropped
```

---

## Cache Optimization

### Data-oriented design

Structure data for how it's accessed, not how it's conceptually organized.

**Array of Structs (AoS) vs Struct of Arrays (SoA):**
```rust
// AoS -- bad cache utilization if only accessing positions
struct Entity { position: Vec3, velocity: Vec3, health: f32, name: String }
let entities: Vec<Entity> = ...;

// SoA -- excellent cache utilization for position-only iteration
struct Entities {
    positions: Vec<Vec3>,
    velocities: Vec<Vec3>,
    healths: Vec<f32>,
    names: Vec<String>,
}
```

When iterating over `positions` in SoA layout, every cache line is 100% useful data. In AoS layout, each cache line pulls in velocity/health/name data that goes unused.

### Key principles

- **Sequential access is fast** -- iterate arrays linearly, avoid random jumps
- **Smaller data = more fits in cache** -- use `u32` instead of `u64` when range permits; use indices instead of pointers
- **Keep hot data together** -- separate frequently-accessed fields from rarely-accessed ones
- **Avoid pointer chasing** -- `Vec<T>` over `LinkedList<T>`, flat indices over `Box<Node>`

---

## Optimization Patterns

### Avoiding redundant work

```rust
// Before: recompute every call
fn is_valid(input: &str) -> bool {
    let re = Regex::new(r"^[a-z]+$").unwrap();
    re.is_match(input)
}

// After: compile once
use std::sync::LazyLock;
static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z]+$").unwrap());

fn is_valid(input: &str) -> bool {
    RE.is_match(input)
}
```

### Iterator chains over manual loops

Iterator chains enable LLVM optimizations (vectorization, bounds check elimination) better than manual indexing:

```rust
// Prefer
let sum: f64 = data.iter().map(|x| x * x).sum();

// Over
let mut sum = 0.0;
for i in 0..data.len() {
    sum += data[i] * data[i];
}
```

### Reducing dynamic dispatch

```rust
// Before: trait object in hot loop
fn process_all(items: &[Box<dyn Processor>]) {
    for item in items { item.process(); }
}

// After: enum dispatch (monomorphic, inlineable)
enum AnyProcessor { TypeA(ProcessorA), TypeB(ProcessorB) }
impl AnyProcessor {
    fn process(&self) {
        match self {
            Self::TypeA(p) => p.process(),
            Self::TypeB(p) => p.process(),
        }
    }
}
```

### Compile-time computation

Move work from runtime to compile time where possible:

```rust
// Lookup table computed at compile time
const LOOKUP: [u8; 256] = {
    let mut table = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        table[i] = compute_value(i as u8);
        i += 1;
    }
    table
};
```

---

## Profiling Checklist

Use this checklist when starting a performance investigation:

### Setup
- [ ] Build in release mode with debug info (`[profile.release] debug = true`)
- [ ] Establish a reproducible benchmark (fixed input, criterion or hyperfine)
- [ ] Record baseline measurement with statistics

### Investigation
- [ ] Run `perf stat` for hardware counter overview
- [ ] Generate flamegraph to identify hotspots
- [ ] Check allocation profile if allocator shows up in flamegraph
- [ ] Check cache behavior if IPC is low
- [ ] Inspect assembly of hot functions for missed optimizations

### After optimization
- [ ] Re-run the same benchmark on the same input
- [ ] Confirm improvement is statistically significant (criterion's change report)
- [ ] Check for regressions in other benchmarks
- [ ] Document the change: what was slow, why, what fixed it, measured improvement

---

## When to Stop

Performance optimization has diminishing returns. Stop when:

- The target metric is met
- The flamegraph shows no single dominant hotspot (work is evenly distributed)
- Further gains require algorithmic changes beyond the current scope
- The optimization would significantly hurt code readability for marginal gain (<5%)

Document the current performance baseline and the profiling results so future optimization efforts start from a known state rather than re-discovering what was already measured.
