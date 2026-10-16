Compared unchanged JSSE HEAD `2ea6417535037ca58d78a433a72a2a0e2dfb4d77` against freshly cloned/built Boa HEAD `f0789a20d87ad52eef6460953929c2b3cdbf8716`. This tracker links the actionable findings; individual issues contain independently reproducible programs, measured comparisons, source pointers, profiles/counters and potential root causes.

## Coverage and limits

- 343 cases / 2,359 timed process invocations: 11 JSSE microbenchmarks, 35 Boa script benchmarks, 23 Boa engine scripts, all 88 JavaScript leaves in the pinned JetStream registry, 146 retained diagnostic repros, 23 Boa expression-output checks and 17 paired controls.
- JetStream source `c603c04db8505477867974a69789309ded2cc948`; 109 registry leaves total. Excluded 19 WebAssembly and two Worker leaves unsupported by these CLIs. Optional/disabled JavaScript leaves were included. source-map-wtb was attempted but depends on WebAssembly unavailable in JSSE.
- Both engines complete 329 cases; JSSE completes 340, Boa 330. All 343 Node oracle runs complete. Execution success is distinguished from output equivalence. SSR exposes #1021 despite passing its HTML validation.
- Three alternating fresh CLI runs per comparable successful workload, sequential CPU 0, 180-second/8-GiB per-process limits; AMD Ryzen AI 9 HX 370 / Rust 1.98.1 / Node v26.8.2. Both default fat-LTO release builds; Boa default Intl/allocator enabled. Separate release-nolto counter binary is never timed.
- JetStream uses **one complete validated iteration per fresh process**, not official 4–160-iteration schedules or official scores. Boa V8 scripts are adaptive; their ratios use inverse reported throughput, not process wall time. Tiny core-script timings mostly measure CLI startup.
- Desktop background load existed. These are diagnostic measurements with all ranges retained. No pooled engine geomean: overlapping diagnostics, startup and heterogeneous metrics would bias it.
- Every one of 107 observed JSSE slowdowns has a sampling profile and counter run. Three further profiles cover slower phases/inner iterations hidden by process totals (110 cases total), plus a focused native PluralRules probe. Tiny/overlapping observations are not promoted to established defects.

## Confirmed bugs

- [ ] #1032 — Function declarations after return/yield are absent from earlier async/generator state fragments. Minimal async function returning a later declaration fails in both JSSE modes; Boa/Node pass. Breaks real Prettier workload. Likely missing whole-body FunctionDeclarationInstantiation before state execution.
- [ ] #1021 — Console is a lexical binding disconnected from globalThis.console; replacing/disabling the global console leaves bare console logging. Breaks SSR logging suppression. Host compatibility defect, not a core ECMAScript claim.

## Performance findings

| Finding | Representative slowdown | Potential cause | Issue |
|---|---:|---|---|
| Generator/manual iteration | 2.341x manual next; 2.012x lazy-collections | AST state payloads and resume bookkeeping | #1031 |
| Default parameters | 2.033x supplied; 2.179x omitted | Re-interned names and fresh body environments per call | #1024 |
| ICU plural selection | 1.920x focused select probe | Reparse locale and rebuild rules on every select | #1033 |
| Closures/small allocations | 1.873x closure creation | Eager function descriptors/prototypes and property storage | #1028 |
| BigInt VM operations | 1.726x pure operator loop | Generic dispatch and Arc ownership transitions | #1026 |
| Arguments materialization | 1.594x arguments-only control | Property-map construction and arguments-name interning | #1025 |
| Lexical/captured names | 1.540x let versus 0.917x var pair | Missing verified frame/outer lexical slot plan | #1022 |
| Property access | 1.564x Richards; 1.436x hash-map | Bytecode bypasses AST property IC and repeats generic MOP/key work | #1029 |
| Compiled leaf calls | See leaf/recursive/callback controls | CallFrame/environment/binding setup and teardown | #1023 |
| VM and lexical blocks | 1.282x arithmetic control; 1.769x block let | Dispatch/operand work and block environment lifecycle | #1030 |
| Minor GC graph tracing | 1.170x Three.js | Repeated captured-environment/remembered graph traversal | #1027 |

Each issue has narrower acceptance and semantic constraints. Overlapping paths are identified; the table does not add the ratios or claim each hypothesis explains the entire workload gap.

## Useful negative results and failures

- Preloading jemalloc does not improve the closure/construction gap: default JSSE 0.066146 s, jemalloc JSSE 0.066581 s, Boa 0.035369 s. Allocation volume/metadata is the supported target, rather than an allocator-only swap.
- Moving one captured constant into the local function narrows a 1.191x gap to 1.061x in a five-repeat follow-up, supporting the free-binding diagnosis.
- DateTimeFormat's benchmark minimum-total-length assertion fails on both engines. An 800-case, 25-locale follow-up has zero errors and exact formatToParts/format consistency everywhere. Locale data/text differ; no spec bug is established from the aggregate length floor.
- WSL fully validates on JSSE in 147.70 s; Boa rejects before execution. No comparative speed ratio is reported.
- Boa also fails Basic, OfflineAssembler, TypeScript Octane, validatorjs, Babel WebTooling, Prism startup hashes, some Intl features and source-map parsing. These are retained as failed executions, never speedups. All 23 expression-output checks pass on both engines.
- Some large workloads strongly favor JSSE: Babylon startup ES5 1.341 versus 52.104 s; ES6 0.970 versus 34.426 s; jsdom-D3 startup 6.987 versus 14.111 s; Mandreel 10.388 versus 12.992 s. These are the same one-iteration fresh-process comparisons, not steady-state engine rankings.

## Retained evidence

Local report: `docs/perf/2026-10-14/boa-head-audit/README.md`; raw samples `measurements.jsonl`; all medians/ranges/statuses `summary.json` and `results.csv`; source manifest/hashes `manifest.json`; immutable script archive `workloads.tar.zst`; registry/exclusions JSON; 110 sampling/counter reports under `profiles/`; focused proofs/probes and exact issue bodies under `issues/`. Raw perf data and snapshot binaries remain in the owned temporary audit directory, recorded by profile-index.json. The temporary Boa/JetStream clones and diagnostic JSSE worktree are removed after the audit.

No implementation, spec/test262 submodule or conformance baseline changed. Full test262 was not rerun on the unchanged engine; this is a benchmark/differential-validation audit, not a conformance certification.
