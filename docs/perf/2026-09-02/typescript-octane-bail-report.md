# TypeScript-Octane bytecode eligibility bail report

Generated 2026-09-02 for issue #524. This completes the issue's missing
bail-reason measurement for the TypeScript-Octane workload and measures the
`this`-expression slice against the same generated driver.

## Result

| counter | v0.6.0 | with compiled `this` | delta |
| --- | ---: | ---: | ---: |
| successful compile attempts | 1,188 | 1,352 | +164 |
| bailed compile attempts | 11,213 | 11,049 | -164 |
| compile-attempt eligibility | 9.58% | 10.90% | +1.32 points |
| compiled Body dispatches | 1,381,051 | 2,947,682 | +1,566,631 |
| compiled invocation share | 12.82% | 27.35% | +14.53 points |
| tree-walker work units | 265,305,414 | 247,434,149 | -17,871,265 (-6.74%) |
| VM opcodes | 19,089,196 | 38,065,540 | +18,976,344 |

Invocation share more than doubles because 164 newly eligible compile
attempts account for 1.57 million Body dispatches. The work and opcode units
are deterministic diagnostic counts, not interchangeable time units; this
table makes no wall-time claim.

## Baseline bail order

The baseline's first unsupported construct per compile attempt was:

| bail reason | count |
| --- | ---: |
| `call callee` | 10,360 |
| `expression:This` | 307 |
| `nested tail call` | 184 |
| `statement:FunctionDeclaration` | 165 |
| `statement:Switch` | 86 |
| `expression:Array` | 39 |
| `expression:New` | 39 |
| `expression:Typeof` | 12 |
| `assign target` | 8 |
| `expression:Function` | 2 |
| `expression:Object` | 2 |
| `literal` | 2 |
| `statement:Continue` | 2 |
| `update target` | 2 |
| `lexical declaration` | 1 |
| `statement:ForIn` | 1 |
| `statement:Try` | 1 |

The report records the first blocker, not every unsupported site. After
`this` becomes eligible, some formerly `expression:This` Bodies reveal a
later blocker: `call callee` rises to 10,464 and `statement:Switch` to 89 even
though neither construct changed. The 307 baseline `this` bails split into
164 successful compilations and Bodies that advanced to another reason.

## Work behind the bails

Grouping the 40 ranked `BODY` rows in the baseline raw report by reason gives:

| bail reason | exclusive tree-walker units | share of ranked rows |
| --- | ---: | ---: |
| `call callee` | 98,817,542 | 47.54% |
| `expression:This` | 71,571,457 | 34.43% |
| `update target` | 13,595,087 | 6.54% |
| `expression:New` | 12,327,419 | 5.93% |
| `expression:Typeof` | 10,570,933 | 5.09% |
| `statement:Switch` | 995,111 | 0.48% |

Method-call callees dominate both attempt count and ranked work, but require
the bytecode-side IC design deferred from #398. `this` is the next largest
self-contained blocker and is present in the OO-heavy Bodies that method-call
support will eventually target. `try`, originally guessed to be the largest
blocker in #524, causes one of 12,401 attempts.

## Method and raw data

- Source: WebKit JetStream revision
  `de88e36ae91d5bd13126fa4cc4b0e0346d779842`.
- Driver: `scripts/run-jetstream.py`'s polyfill, deterministic-random setup,
  benchmark sources, and synchronous harness with one outer iteration.
- Baseline: `dd4fd88` (v0.6.0). Slice: `8cb4f74`.
- Build: `cargo build --release --features perf-counters`; execution used
  `jsse --bytecode`. Instrumented runs were not timed.
- The generated driver completed and printed its one benchmark result in both
  runs.
- Raw reports: `counters-typescript-octane-before.txt` and
  `counters-typescript-octane-after.txt` in this directory.

The baseline totals independently reproduce the planning run (1,188 / 11,213)
and the issue's earlier 1,187 / 11,211 sample to within two attempts.
