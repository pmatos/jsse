# numeric — arithmetic loops and array / typed-array element traffic

jsse HEAD `2b621c5` vs boa 0.22.0, buildbox3, 2026-10-10. Medians of 5, jsse and boa interleaved, repeat spread 1-2%. Load average was 14-28 from the other investigation agents sharing the box.

## Summary

- The VM is name-based: variables are `HashMap<String, Binding>` entries hashed with SipHash. Each `x = e;` costs four dispatches (`ResolveName`, `e`, `StoreResolvedName`, `Pop`). The VM is 3.3x boa on pure-local arithmetic and 1.8-2.5x on array and closure kernels.
- Bytecode eligibility is all-or-nothing per function. One `throw`, `switch`, `typeof`, array literal, `obj.x++` or `let`/`const` sends the whole function to the tree-walker, which is 1.8x slower than the VM and 5.6-6.6x boa on the same loop.
- Not bottlenecks: typed-array element access is ~7% of instructions, GC safepoints 1-1.5%, fat LTO with `codegen-units=1` moves results by ±5%.

## Gap table

`nobc` = `--no-bytecode`.

| repro | jsse s | boa s | ratio | nobc s | load1 |
|---|---|---|---|---|---|
| intarith | 3.23 | 0.97 | 3.33 | 5.69 | 23.8 |
| bail_none (VM control) | 2.07 | 0.62 | 3.34 | 3.73 | 27.2 |
| bail_throw | 3.72 | 0.62 | 6.00 | 4.08 | 27.1 |
| bail_switch | 3.68 | 0.62 | 5.94 | 3.68 | 26.3 |
| bail_typeof | 3.78 | 0.62 | 6.10 | 3.73 | 25.8 |
| bail_arrayliteral | 3.73 | 0.57 | 6.54 | 3.73 | 13.7 |
| bail_memberupdate | 3.73 | 0.67 | 5.57 | 3.78 | 28.0 |
| bail_letconst | 3.73 | 0.67 | 5.57 | 3.98 | 20.3 |
| gaussblur | 2.83 | 1.12 | 2.53 | 5.43 | 25.6 |
| array | 3.78 | 1.62 | 2.33 | 6.19 | 14.2 |
| closurevars | 2.78 | 1.32 | 2.11 | 6.54 | 27.6 |
| math | 2.93 | 1.37 | 2.14 | 4.58 | 23.4 |
| typedarray_global | 2.78 | 1.42 | 1.96 | 5.09 | 20.7 |
| globalvars | 3.18 | 1.77 | 1.80 | 3.98 | 23.4 |
| typedarray_local | 2.22 | 1.27 | 1.75 | 4.64 | 18.2 |

`bench_opmix` per phase (ms, 3 interleaved runs, median):

| phase | jsse | boa | ratio |
|---|---|---|---|
| arith | 1975 | 508 | 3.89 |
| elem | 3064 | 1373 | 2.23 |
| called | 4244 | 1193 | 3.56 |
| mixed | 4892 | 1996 | 2.45 |
| labeledLoop | 2519 | 1161 | 2.17 |
| labeledControl | 14200 | 6347 | 2.24 |

Going from `arith` to `elem` adds ~1.1 s on jsse and ~0.87 s on boa: element cost is similar in both engines and only dilutes the variable-access gap.

## Root causes (ranked)

**RC1 — SipHash name lookups for every variable.**
- `types.rs:844-847` declares `Environment.bindings: HashMap<String, Binding>` with std `RandomState`.
- VM ops that hash it: `vm.rs:272-296` (LoadName), `:319-337` (ResolveName), `:350-380` (StoreResolvedName), `:381-397` (UpdateName, via `eval.rs:2320` and `env_set`).
- callgrind: SipHash plus `memcmp` is 28-47% of instructions (arith 36.7%, gaussblur 39.7%, closurevars 32.8%, globalvars 47%). The hash is recomputed at every scope level of a miss because each map has its own `RandomState`.
- `LoadName` is 20-35% of dispatched ops in every VM-bound JetStream benchmark; ResolveName + StoreResolvedName + Pop add another 25-35%.

**RC2 — no op fusion.**
- `compiler.rs:295-312` and `:551-569` emit the four-dispatch assignment sequence.
- opmix `arith` runs 49 ops per iteration (a register VM needs ~18). `bail_none` runs 46.7 ops per iteration at ~210 instructions per op (hashing plus dispatch).
- `run_chunk_inner` self cost is 30-35% of instructions. `push_value` (`vm.rs:57`, not inlined) is 5-8%. Binary ops dispatch twice (`vm.rs:681-733` then `eval.rs:1897`), 4-8%.

**RC3 — ToInt32/ToUint32 go through a software `fmod`.**
- `types.rs:921-938` uses `trunc` and `%`, which lowers to `compiler_builtins` `fmod`: 5-7% of bit-op loops. JS `%` takes the same path (`types.rs:832`).

**RC4 — whole-function bail on unsupported constructs.** Sites: `compiler.rs:371, 375-391, 486, 553, 705-719, 720`. Counter evidence (perf-counters build):

| benchmark | VM ops | AST units | dominant bails (share of tree-walker work) | est. share of time on tree-walker |
|---|---|---|---|---|
| stanford-crypto-aes | 2.3M | 61M | `throw` 99% (guard clause in `function t`, `sjlc.js:68`) | ~98% |
| gbemu | 95M | 77M | `switch` 49%, member `++` 30%, array literal 9%, `try` 7% | ~60% |
| Box2D | 39.5M | 2.9M | `switch` 82%, member `++` (164 bodies) | ~12% |
| crypto | 56.5M | 1.9M | member `++`/`--` 91% (`montReduce`) | ~6% |
| sha256, pbkdf2 | ~50M | ~1.1M | array literal, `typeof`, nested tail call | ~4% |
| mandreel | 1.64B | 26M | `switch` 98% | ~3% |

Time-share conversion uses 1 AST unit ≈ 1 VM op × 1.8. The "share of tree-walker work" column says which construct to compile next, not how much a benchmark gains.

**RC5 — plain `Array` element access is slow.**
- Loads (`property.rs:167-177`) format the index as a string and hash it into `properties` on every `a[i]`.
- Stores have no fast path (`vm.rs:496-523`; `property.rs:185` handles typed arrays only): ToPropertyKey, `parse_array_index`, `from_utf8`.
- The same stencil loop takes 3.66 s on `Array` and 2.79 s on `Float64Array` (+31%); ~22% of instructions in callgrind.

**RC6 — free and global variable reads walk the scope chain uncached.**
- `resolve_identifier` (`eval.rs:7696`) takes one hash per level, then `get_own_property` on the global object, which clones a descriptor.
- The gap is smaller (1.8-2.1x) because boa also hashes globals; this is why the control `bench_loop` sits at 1.08x (script-level globals, both engines hash).

**RC7 — per-call `var` hoisting (calls theme, note only).** `vm.rs:246-254` does `contains_key` + `to_string()` + insert for every `var` on every call. mandreel makes 13.2M calls.

## Experiments (throwaway patches on HEAD, medians of 3)

E2 = int32 fast path alone; E1+E2 adds FxHash for `Environment.bindings`.

| repro | HEAD | E2 | E1+E2 |
|---|---|---|---|
| intarith | 3.23 | 2.88 | 2.33 |
| bail_none | 2.07 | 1.72 | 1.47 |
| gaussblur | 2.83 | 2.78 | 2.37 |
| closurevars | 2.78 | 2.88 | 2.22 |
| bail_letconst (tree-walker) | 3.73 | 3.48 | 2.92 |
| array | 3.78 | 3.74 | 3.18 |

E1+E2 gives -10% to -29% (median ≈ -22%) and closes none of the 3.3x gap.

**Caution on W2.** The FxHash experiment repeats #389 (-13% on mandreel setup), deliberately reverted in #393. Rationale: `docs/specs/2026-07-25-environment-binding-hashing-design.md` — environment names come from script source, so the project wants HashDoS resistance. W2 must be a keyed fast hasher (foldhash/ahash with a per-process seed) or names pre-hashed at compile time. Plain Fx will be rejected.

## Proposed work items

| # | Title | Size | Expected impact |
|---|---|---|---|
| W3 | `perf(vm): slot-resolved function locals` | L | 1.8-2.5x on `intarith`, `bail_none`, `gaussblur`; the item that closes the boa gap |
| W4 | `perf(vm): tighter instruction set` | M | 25-40% fewer dispatches |
| W1 | `perf(number): int32 fast path for ToInt32/ToUint32 and integer %` | S | -11% on `intarith` |
| W2 | `perf(interpreter): keyed fast hasher or precomputed name hashes for env bindings` | S | up to -20% combined with W1 |
| W5 | `perf(vm): compile more constructs` | M each | AES ~1.7x, gbemu ~1.35x, single digits elsewhere |
| W6 | `perf(property): dense Array get/set fast paths` | M | -25% on array kernels |
| W7 | `perf(interpreter): global and free-variable inline cache` | M | -15 to -25% |

**W3.** Add `LoadLocal`/`StoreLocal`/`IncLocal`, resolved for `var` and parameter bindings at compile time (`Chunk::var_names`, `compiler.rs:219`). Back function scopes with an ordered slot vector plus a per-function name-to-slot layout; the tree-walker, closures, `arguments` mapping and GC still go through `Environment`. Add `(hops, slot)` pairs for free variables.
- Acceptance: `intarith` ≤1.6 s, `bail_none` ≤1.1 s, `gaussblur` ≤1.6 s; no LoadName/ResolveName/StoreResolvedName in the gaussian-blur hot loop.
- Risks: direct eval, `with`, `arguments` aliasing, closures capturing loop vars, generators, GC roots.

**W4.** Statement-context assignment becomes `StoreNamePop`; statement-context `x++` becomes `IncName`; fuse compare and branch; small ints as immediates; inline number-number ops in the dispatch loop; `#[inline(always)]` on `push_value`. The reference must still be resolved before the RHS is evaluated.
- Acceptance: arith ops per iteration ≤25.

**W1.** If `|x| < 9e18` return `(x as i64) as u32`. Add an integer `%` fast path with correct `-0` and NaN handling.
- Acceptance: `intarith` ≤2.9 s, `fmod` below 1% of instructions, boundary tests added in `test262-extra`.

**W2.**
- Acceptance: `intarith` ≤2.6 s together with W1, `hash_one::<&str>` out of the callgrind top 5, and a short DoS-resistance argument in the PR.

**W5.** Sub-issues in order: `switch`, member `++/--`, `throw`, `typeof`/array/object literal/function expression, nested tail call, `try`, `let`/`const`. Worth evaluating first: a per-statement `ExecAst` fallback op that removes the cliff for every construct at once.
- Acceptance: BAIL entries go to 0 for the named benchmarks, each `bench_numeric_bail_*` within 1.1x of `bail_none`, AES AST units below 1% of total.

**W6.** Reads: replace the string-keyed shadow check with an "indexed props outside elements" flag. Writes: in-bounds and append fast paths. Preserve frozen, non-writable, holes with prototype setters and Proxy behaviour.
- Acceptance: `array` ≤2.9 s.

**W7.** Cache `(shape, slot)` in the `LoadName` op for global data properties.
- Acceptance: `globalvars` ≤2.3 s, `typedarray_global` within 1.15x of `typedarray_local`.

## What did NOT reproduce

- Typed-array element access cost: `numeric_index_fast_get`/`set` plus `typed_array_*` total ~7% of instructions on `typedarray_local`.
- LTO: fat LTO with `codegen-units=1` changed the 15 repros by -5% to +12% (≈ -3% median).
- GC safepoints: 1-1.5% of instructions; GC never ran in these kernels.
- Global-variable access as the worst case: 1.8x boa versus 3.3x for pure locals.

## Reproduction

```
R=/home/igalia/pmatos/dev/bench-workspace/run-2026-10-10
python3 $R/microbench.py '{"jsse":["'$R'/jsse-head/target/release/jsse"],"boa":["'$R'/boa-v0.22-src/target/release/boa"]}' <dir with bench_numeric_*.js> out.json 5
# add "jsse-nobc":["<jsse bin>","--no-bytecode"] to compare against the tree-walker
```

Counters: build with `--features perf-counters`, read the PERF/BAIL/BODY/OP tables on stderr. callgrind: use a ~50x smaller `n`. Scratch builds and raw outputs: `$R/work-numeric/` on buildbox3 (`src-exp`, `cg/`, `pc/`, `micro-*.json`, `opmix.log`).
