# #1022: slot-address function lexicals and remember where free names resolve

Function-top-level `let`/`const`/class and function-declaration names were not part of the compiled frame layout, so every
read and write of one was a by-name `LoadName`/`ResolveName`/`StoreResolvedName` walk, and `InitLexical` inserted by name.
Free names (captured variables, script-level `const`/`let`/`class`, globals) walked the whole scope chain on every read
(`walk_scope_chain`, ~200 Ir per lookup in the noble-curves and Air profiles of #1022).

## Change

- **Frame layout.** `compile_function_body_as` appends `compiler::instantiated_names` to the slot layout: the top-level function
  declarations, then the `let`/`const`/class names, in the order declaration instantiation creates them (after the var prologue,
  so the layout is predictable). It applies when every `var` name the tree-walker would hoist is already in the chunk's var
  prologue and the parameters are simple; otherwise the body keeps no hints for those names (hints are verified by atom
  identity, so a wrong guess only costs a by-name fallback). `InitLexical` now takes the frame slot at function top level
  (`Compiler::init_slot`). TDZ reads miss the initialised check and fall to the generic path, which throws; `const` stores take the
  generic path after the right-hand side, as before. `FrameShape::separate_body_env` marks the non-simple-parameter case, whose
  body environment has its own, unpredictable, order.
- **Scope hint per name.** `GlobalHint` (one per `Chunk::names` entry) keeps a `ScopeHint`: the hops from the nearest function
  (or global) scope and the binding slot at which the last full walk found the name, or a marker for "reached the global
  environment". `global_access::walk_scope_chain_hinted` re-verifies it on every use: each scope on the way must be a plain
  declarative scope that does not bind the name (one name-filter probe, exact lookup only on a filter hit), and the remembered
  scope must hold the name's atom at the remembered slot. That is the scope the full walk stops at, so a hit answers exactly as the
  walk does; any mismatch (a `with` object, an eval-introduced `var`, a deleted binding that moved slots, another chain shape)
  runs the full walk, which retrains. Counting from the function scope keeps the hint valid whatever block scopes are open at the site.
  It serves `LoadName`, `LoadCalleeName`, the `LoadLocal` fallback and `resolve_identifier_ref_hinted` (`ResolveName`, compound
  assignment and update of non-local names).
- **Global shadow stamp.** An indexed `BindingMap` (more than 16 names; the global environment of a script with many lexicals)
  can hand out a per-thread-unique stamp of its key set: it is assigned the first time it is read after the key set last changed,
  and every change only clears it (one store per push into a large environment). A global-object hint remembers the stamp at which
  the global environment did not hold the name, so a hit skips that hash probe while the stamp is unchanged. Small maps report
  stamp 0 and are probed (filter, then scan).
- `perf-counters`: `scope_hint_hits`/`scope_hint_misses` (per thread).
- `compiler::LocalLayout` resolves frame locals through a hash index only for frames of more than `LINEAR_MAX` names (a linear
  scan per identifier cost Air's generated functions about 80M Ir before that). A function whose only frame names are top-level
  lexicals or function declarations now takes the second compile pass that emits the slot ops (one extra compile per literal).

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`, whole-process `I refs`. base = `origin/main` `e8255633`, new = this branch. JetStream drivers at
`--iterations 1`; micro = `benchmarks/scripts`.

Per-iteration, startup cancelled with two run lengths (50k and 150k iterations):

| program | base Ir/iter | new Ir/iter | delta |
|---|---:|---:|---:|
| `let sum` + `for (let i...)`, `sum += i & 31` | 1,318 | 915 | -30.6% |
| function-top-level `const K` read in a `var` loop | 1,104 | 1,016 | -8.0% |
| `let` captured by a closure called per iteration | 3,727 | 3,622 | -2.8% |
| script-level `const K` read in a `var` loop | 1,156 | 1,148 | -0.7% |
| script-level `var K` (global-object property) read in a `var` loop | 1,271 | 1,254 | -1.3% |

Whole-process (selection; geometric mean over 39 workloads: -1.27%):

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| lazy-collections | 4,399,030,264 | 3,961,279,185 | -9.95% |
| bigint-noble-secp256k1 | 6,309,019,329 | 5,915,520,611 | -6.24% |
| sync-fs | 6,745,544,949 | 6,390,623,432 | -5.26% |
| bigint-noble-ed25519 | 7,139,027,389 | 6,782,080,986 | -5.00% |
| micro:loop | 3,267,545,324 | 3,126,545,601 | -4.32% |
| bigint-bigdenary | 4,455,242,729 | 4,335,057,329 | -2.70% |
| bigint-noble-bls12-381 | 60,183,768,303 | 58,622,684,000 | -2.59% |
| navier-stokes | 3,915,209,759 | 3,820,063,133 | -2.43% |
| raytrace | 12,310,736,197 | 12,036,629,940 | -2.23% |
| Babylon | 2,478,250,242 | 2,425,794,277 | -2.12% |
| proxy-vue | 1,153,619,960 | 1,131,751,696 | -1.90% |
| micro:fib | 5,654,514,432 | 5,600,672,766 | -0.95% |
| richards | 8,695,339,013 | 8,660,674,128 | -0.40% |
| Box2D | 6,011,838,708 | 5,987,794,730 | -0.40% |
| delta-blue | 6,424,892,260 | 6,408,411,688 | -0.26% |
| Air | 1,892,845,169 | 1,898,735,175 | +0.31% |
| js-tokens | 4,805,301,442 | 4,844,517,541 | +0.82% |
| proxy-mobx (noise: 574.8M-598.9M over four runs of either binary) | 574,664,921 | 586,555,407 | +2.07% |

Air's symbolised build (callgrind, same source) is 1,964.2M base vs 1,960.1M new: the shipped +0.35% is code layout.

## What did not move

richards' 6.8% in `resolve_name_fast` is global-object data properties (`global_prop_fast_hits` = 2.06M, `scope_hint_hits`
2.07M): the hint has to prove that no scope between the site and the global environment binds the name, which is the same
per-hop work the walk did, and `global_object_own_data` (~100 Ir per read) is untouched. The stamp removed one hash probe per
read (richards -0.4% Ir in total). Removing the per-hop checks needs a proof that a name is never bound below the global scope
(an atom-level flag plus a guarantee that the chain ends in the current realm's global environment); not attempted here.
