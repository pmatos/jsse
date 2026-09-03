# OfflineAssembler bytecode eligibility bail report

Generated 2026-09-02 for issue #524. This completes the issue's missing
bail-reason measurement for OfflineAssembler and measures the
`this`-expression slice against the same generated driver.

## Result

| counter | v0.6.0 | with compiled `this` | delta |
| --- | ---: | ---: | ---: |
| successful compile attempts | 3 | 81 | +78 |
| bailed compile attempts | 4,428 | 4,350 | -78 |
| compile-attempt eligibility | 0.07% | 1.83% | +1.76 points |
| compiled Body dispatches | 460 | 485,097 | +484,637 |
| compiled invocation share | 0.04% | 37.23% | +37.19 points |
| tree-walker work units | 13,339,220 | 11,501,945 | -1,837,275 (-13.77%) |
| VM opcodes | 2,572 | 1,921,405 | +1,918,833 |

The compiler caches per function object, and this parser creates fresh
closures while parsing. Compile-attempt volume therefore scales with the work
executed and is not a unique-source-function count. That explains why this
run's 3 / 4,428 differs from #524's earlier 1 / 123 sample; the proportions
and ordering are the relevant evidence.

The work and opcode units are deterministic diagnostic counts, not
interchangeable time units. The table makes no wall-time claim.

## Baseline bail order

| bail reason | count |
| --- | ---: |
| `call callee` | 4,313 |
| `expression:This` | 91 |
| `lexical declaration` | 15 |
| `binary op` | 3 |
| `nested tail call` | 3 |
| `expression:Object` | 1 |
| `expression:Typeof` | 1 |
| `statement:FunctionDeclaration` | 1 |

After `this` becomes eligible, 78 of those attempts compile. The remaining
Bodies advance to later blockers, so the first-bail table changes rather than
simply losing one row; `call callee` becomes 4,316.

## Work behind the bails

Grouping the 40 ranked `BODY` rows in the baseline report by reason gives:

| bail reason | exclusive tree-walker units | share of ranked rows |
| --- | ---: | ---: |
| `call callee` | 3,460,149 | 26.68% |
| `statement:FunctionDeclaration` | 3,335,043 | 25.72% |
| `binary op` | 1,896,102 | 14.62% |
| `expression:This` | 1,870,249 | 14.42% |
| `lexical declaration` | 1,733,230 | 13.37% |
| `nested tail call` | 672,663 | 5.19% |

`this` is the only self-contained expression blocker ahead of the lexical and
binary-op slices, and removing it eliminates 13.77% of run-wide tree-walker
work units. The largest individual fallback Body remains
`lex`, blocked by a nested function declaration; method calls remain the
largest reason by aggregate ranked work. `try` and `throw` cause no baseline
bails.

## Method and raw data

- Source: WebKit JetStream revision
  `de88e36ae91d5bd13126fa4cc4b0e0346d779842`.
- Driver: `scripts/run-jetstream.py`'s polyfill and synchronous harness with
  one outer iteration.
- Baseline: `dd4fd88` (v0.6.0). Slice: `8cb4f74`.
- Build: `cargo build --release --features perf-counters`; execution used
  `jsse --bytecode`. Instrumented runs were not timed.
- OfflineAssembler completes its benchmark iteration, then its validation
  reports the existing line-42 numeric-rendering mismatch. The exit report is
  still complete; both raw files retain that final diagnostic.
- Raw reports: `counters-offlineassembler-before.txt` and
  `counters-offlineassembler-after.txt` in this directory.

The baseline totals exactly reproduce the planning run (3 / 4,428).
