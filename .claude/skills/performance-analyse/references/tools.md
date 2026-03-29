# Rust Performance Tools Reference

Comprehensive guide to profiling, benchmarking, and analysis tools for Rust programs.

---

## Benchmarking

### criterion.rs

The standard Rust microbenchmarking framework. Provides statistically rigorous measurements with warm-up, outlier detection, and confidence intervals.

**Setup:**
```toml
# Cargo.toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name = "my_benchmark"
harness = false
```

**Basic benchmark:**
```rust
// benches/my_benchmark.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_function(c: &mut Criterion) {
    c.bench_function("my_func", |b| {
        b.iter(|| my_func(black_box(input)))
    });
}

criterion_group!(benches, bench_function);
criterion_main!(benches);
```

**Run:**
```bash
cargo bench                          # Run all benchmarks
cargo bench -- "specific_name"       # Filter by name
cargo bench -- --save-baseline base  # Save as named baseline
cargo bench -- --baseline base       # Compare against saved baseline
```

**Key features:**
- Automatic warm-up and iteration count tuning
- Statistical analysis with confidence intervals
- HTML reports in `target/criterion/`
- Baseline comparison for regression detection
- `BenchmarkGroup` for parameterized benchmarks

**Parameterized benchmarks:**
```rust
fn bench_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("sort");
    for size in [100, 1000, 10000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &size,
            |b, &size| {
                let data: Vec<u64> = (0..size).collect();
                b.iter(|| data.clone().sort());
            },
        );
    }
    group.finish();
}
```

### hyperfine

Command-line benchmarking tool for measuring whole-program execution time.

```bash
# Basic usage
hyperfine './target/release/my_program input.txt'

# Compare two implementations
hyperfine './old_version input.txt' './new_version input.txt'

# With warm-up runs
hyperfine --warmup 3 './target/release/my_program'

# Parameterized
hyperfine --parameter-scan size 100 10000 \
  './target/release/my_program --size {size}'

# Export results
hyperfine --export-json results.json './target/release/my_program'
hyperfine --export-markdown results.md './target/release/my_program'

# Set minimum number of runs
hyperfine --min-runs 20 './target/release/my_program'

# Prepare command (run before each timing run)
hyperfine --prepare 'sync; echo 3 | sudo tee /proc/sys/vm/drop_caches' \
  './target/release/my_program'
```

### cargo bench (built-in)

Rust's built-in benchmarking (nightly only, `#[bench]` attribute). Less featureful than criterion but zero setup.

```rust
#![feature(test)]
extern crate test;

#[bench]
fn bench_add(b: &mut test::Bencher) {
    b.iter(|| test::black_box(1 + 1));
}
```

---

## CPU Profiling

### perf (Linux)

The primary Linux profiling tool. Samples the call stack at regular intervals to identify CPU hotspots.

```bash
# Record a profile (default: CPU cycles)
perf record --call-graph dwarf ./target/release/my_program

# Record with specific event
perf record -e cache-misses --call-graph dwarf ./target/release/my_program

# View the profile interactively
perf report

# Hardware counter summary (no recording needed)
perf stat ./target/release/my_program

# Detailed hardware counters
perf stat -d ./target/release/my_program
# Shows: cycles, instructions, IPC, cache refs/misses, branch misses

# Record for a running process
perf record -p <PID> --call-graph dwarf sleep 10
```

**Important flags:**
- `--call-graph dwarf` -- use DWARF debug info for accurate stack traces (preferred for Rust)
- `--call-graph fp` -- use frame pointers (faster, but Rust doesn't emit them by default)
- `-F 99` -- sample at 99 Hz (avoid harmonics with timer interrupts)
- `-g` -- shorthand for `--call-graph fp`

**Enable frame pointers in Rust** (for `--call-graph fp`):
```toml
# .cargo/config.toml
[build]
rustflags = ["-C", "force-frame-pointers=yes"]
```

**Enable debug info in release builds** (for better symbols):
```toml
# Cargo.toml
[profile.release]
debug = true      # Full debug info
# or
debug = 1         # Line tables only (smaller, usually sufficient)
```

### cargo-flamegraph

Generates SVG flame graphs from perf data. The fastest path from "it's slow" to "here's where."

```bash
# Install
cargo install flamegraph

# Profile a binary
cargo flamegraph -- input.txt

# Profile a specific benchmark
cargo flamegraph --bench my_benchmark -- --bench "specific_name"

# Profile a test
cargo flamegraph --test my_test -- test_name

# Reverse/icicle graph (callers on top)
cargo flamegraph --reverse -- input.txt

# With specific perf frequency
cargo flamegraph --freq 997 -- input.txt

# Output to specific file
cargo flamegraph -o profile.svg -- input.txt
```

**Reading flame graphs:**
- Width = proportion of total time (wider = more time spent)
- Y-axis = call stack depth (bottom = entry point, top = leaf functions)
- Look for wide, flat plateaus -- those are the hotspots
- Narrow, deep stacks indicate deep call chains but not necessarily slow
- Color is random and meaningless (unless using differential flame graphs)

### samply

Modern sampling profiler with a web-based UI. Alternative to perf + flamegraph.

```bash
cargo install samply
samply record ./target/release/my_program
# Opens Firefox profiler UI automatically
```

---

## Memory Profiling

### heaptrack

Tracks every heap allocation: where it was made, how large, and when it was freed.

```bash
# Record
heaptrack ./target/release/my_program

# Analyze (GUI)
heaptrack_gui heaptrack.my_program.<pid>.gz

# Analyze (CLI)
heaptrack_print heaptrack.my_program.<pid>.gz
```

**What to look for:**
- Total allocations count and peak memory
- Allocation hotspots (functions that allocate the most)
- Temporary allocations (allocated and freed quickly -- optimization targets)
- Memory leaks (allocated but never freed)

### DHAT (Dynamic Heap Analysis Tool)

Valgrind tool that profiles heap usage with detailed per-allocation statistics.

```bash
valgrind --tool=dhat ./target/release/my_program
# Produces dhat.out.<pid>
# Open in DHAT viewer: https://nnethercote.github.io/dh_view/dh_view.html
```

**What DHAT reports:**
- Total bytes allocated, total blocks
- "Access counts" per allocation site (how often allocated memory is read/written)
- Short-lived allocations (high turnover = optimization target)
- Maximum memory live at any point

### Rust allocator instrumentation

Use a counting allocator to measure allocations within specific code sections:

```rust
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct CountingAllocator;
static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
static ALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static A: CountingAllocator = CountingAllocator;
```

---

## Cache & Branch Analysis

### cachegrind

Simulates CPU caches to identify cache-unfriendly access patterns.

```bash
valgrind --tool=cachegrind ./target/release/my_program
# Produces cachegrind.out.<pid>

# Annotate source with cache miss info
cg_annotate cachegrind.out.<pid>

# Compare two runs
cg_diff cachegrind.out.before cachegrind.out.after
```

**Key metrics:**
- `I1mr` / `ILmr` -- L1/LL instruction cache miss rate
- `D1mr` / `DLmr` -- L1/LL data read cache miss rate
- `D1mw` / `DLmw` -- L1/LL data write cache miss rate

**Interpretation:**
- High D1 miss rate on a loop -- data doesn't fit in L1, consider restructuring for locality
- High LL miss rate -- working set doesn't fit in LLC, reduce data size or improve access patterns

### callgrind

Counts function call frequencies and instruction execution counts.

```bash
valgrind --tool=callgrind ./target/release/my_program
# Produces callgrind.out.<pid>

# Visualize with KCachegrind
kcachegrind callgrind.out.<pid>

# CLI annotation
callgrind_annotate callgrind.out.<pid>
```

---

## Binary & Codegen Analysis

### cargo-bloat

Shows what contributes to binary size -- functions, crates, and generics.

```bash
cargo install cargo-bloat

# Largest functions
cargo bloat --release -n 20

# By crate
cargo bloat --release --crates

# Filter by crate
cargo bloat --release --filter regex_syntax
```

### cargo-show-asm

Inspect the assembly, LLVM IR, or MIR generated for a specific function.

```bash
cargo install cargo-show-asm

# Show assembly for a function
cargo asm my_crate::my_function

# Show LLVM IR
cargo asm --llvm my_crate::my_function

# Show MIR
cargo asm --mir my_crate::my_function

# List available functions
cargo asm --lib
```

**What to look for in assembly:**
- Vectorized loops (SIMD instructions: `vmovaps`, `vaddps`, etc.)
- Bounds check panics (`call core::panicking::panic_bounds_check`)
- Unnecessary memcpy/memmove calls
- Branch-heavy code where branchless alternatives exist

### cargo-llvm-lines

Shows which functions generate the most LLVM IR (monomorphization bloat).

```bash
cargo install cargo-llvm-lines

cargo llvm-lines --release | head -20
# Shows: lines of IR, copies, function name
```

High line counts from generic functions indicate monomorphization bloat -- consider dynamic dispatch or reducing generic parameters.

---

## Compile-Time Analysis

### cargo build --timings

Built-in compilation timing analysis.

```bash
cargo build --release --timings
# Opens an HTML report showing per-crate compilation times
```

### cargo-udeps

Find unused dependencies that slow compilation.

```bash
cargo install cargo-udeps
cargo +nightly udeps --release
```
