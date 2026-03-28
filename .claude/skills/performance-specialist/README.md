# performance-specialist

A Claude Code skill for Rust performance analysis. It provides structured workflows, tool guidance, and methodology for profiling and optimizing Rust programs.

## How to use

The skill triggers automatically when you mention performance-related tasks in Claude Code. Example prompts:

```
profile Rust code
benchmark this function
find performance bottlenecks
why is this slow
optimize performance
analyze flamegraph
check allocations
cargo bench
run perf
measure runtime
```

You can also invoke it explicitly:

```
/performance-specialist
```

## What it provides

When triggered, Claude gains access to:

1. **Core workflow** (SKILL.md) — the measure-first cycle, tool selection guide, common pitfalls, and how to communicate results
2. **Tool reference** (references/tools.md) — detailed usage for criterion, hyperfine, perf, flamegraph, heaptrack, DHAT, cachegrind, callgrind, cargo-bloat, cargo-show-asm, and more
3. **Methodology reference** (references/methodologies.md) — statistical benchmarking, top-down profiling, allocation analysis, cache optimization, and optimization patterns

The core workflow loads immediately. Reference files load on demand when deeper detail is needed.

## Files

```
performance-specialist/
├── README.md              ← this file
├── SKILL.md               ← core skill (loaded when triggered)
└── references/
    ├── tools.md           ← detailed tool usage guide
    └── methodologies.md   ← analysis methodology deep-dive
```
