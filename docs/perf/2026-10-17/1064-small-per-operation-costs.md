# #1064: seven small per-operation costs (first batch)

The audit numbers in #1064 predate #1074-#1097, so each item was re-profiled on `origin/main` (`36ccf740`) first.
Outcome per item: implemented with a measured gain, or already fixed. Residuals that need a larger change are listed at
the end.

## Changes

- **1 string concat.** `StrView` keeps results of at most 40 code units inline (`Units::Inline`), so a short result is
  one allocation instead of view + buffer header + units vector. Longer strings keep the shared-buffer form and the
  tail-claim append (`Units::Shared`), so the `str_append` growth chain is unchanged. `ptr_eq`/`units_addr` read the
  units address, `JsString: Eq` short-circuits on the same `Arc`, and distinct-string `===` moved into an out-of-line
  `string_units_equal` so `strict_equal_non_number` stays small enough to inline into the VM loop.
- **2 name pool.** `Compiler::add_name` interns first, then dedupes on `Atom` identity: a linear pointer scan up to 16
  names, a `HashMap<Atom, u16>` with the atoms' precomputed hash beyond that. Pool order is unchanged.
  `add_private_name` keeps its scan (private-name pools are a handful of entries).
- **3 parseInt.** A Number argument with radix `undefined` returns `trunc(n)` for `1e-6 <= |n| < 1e21` (the range where
  the string form has no exponent), `+0` for zeros and NaN for NaN/Infinity; everything else takes the string path.
- **5 spread.** `iterate_to_vec` copies a plain dense Array when `@@iterator` is the realm's original
  `Array.prototype.values` on `%Array.prototype%` and `%ArrayIteratorPrototype%.next` is the original; holes need
  `proto_chain_index_free`. No iterator object is created. This helps every `iterate_to_vec` caller, not just calls.
- **6 exit teardown.** `cli::retire_interpreter` forgets the interpreter in release builds (debug builds, including
  `release-checked`, still drop it so the arena and root-stack assertions run). Nothing in the `Drop` impls has an
  observable effect.
- **7 object patterns.** `excluded_keys` is only built when the pattern has a rest element.

Already fixed (no change): **4 primitive-string `.length`/method get.** `access_property_on_value` already tests
`length` and the index on the borrowed key; the probes below show 0 blocks and a primitive `.length` cheaper than an
object `.length`.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`. base = `origin/main` `36ccf740` (binary sha256 `01ed35ea6416...`), new = this change
(`68a3b38aae50...`). Loop probes: Ir per iteration = (20,000 minus 2,000 iterations) / 18,000. Blocks come from dhat on
`--no-default-features` builds of the same two trees (jemalloc hides allocations from dhat).

| probe | base | new | note |
|---|---:|---:|---|
| 1 `"..." + tag + "..."` (control `last = tag`: 1,335) | 2,562 | 2,256 | net 1,227 -> 920 Ir (-25.0%); blocks 6 -> 2 per iteration (1 per result) |
| 3 `parseInt(i*0.5+3.25)` (`Math.trunc` control: 3,030) | 3,756 | 2,627 | net over trunc +726 -> -405 Ir; blocks 1 -> 0 |
| 4 `s.length` on a primitive (object `.length`: 1,600) | 1,490 | 1,493 | already 0 blocks; `s.charCodeAt(5)` 2,708 -> 2,711, 0 blocks |
| 4' `s[5]` (one-unit string result) | 2,129 | 2,099 | blocks 3 -> 2 |
| 5 `g(...[])` (control `g(a0,a1,a2)`: 3,804) | 6,763 | 4,523 | +2,959 -> +711 over control |
| 5 `g(...[1,2,3])` | 8,888 | 4,968 | +5,084 -> +1,156 |
| 5 `g(...[12 elements])` | 15,224 | 5,503 | +11,420 -> +1,691 |
| 5 `new C(i, ...args)` / `a.push(...args)` | 12,177 / 12,601 | 8,407 / 8,690 | |

| probe (whole process) | base | new |
|---|---:|---:|
| 2 name pool K=500, distinct minus same | 7.51M (15.0K/name) | 1.02M (2.0K/name) |
| 2 K=2000 | 80.16M (40.1K/name) | 4.40M (2.2K/name) |
| 2 K=4000 | 381.32M (95.3K/name) | 9.30M (2.3K/name) |
| 6 retained heap N=50,000 | 668.25M | 610.79M (-8.6%) |
| 6 retained heap N=200,000 | 2,815.04M | 2,587.54M (-8.1%) |
| 7 `none` / `d2` / `o2` (Ir per call, minus `none`) | d2 1,824 / o2 2,797 | d2 1,826 / o2 2,268 |
| 7 `d4` / `o4` | d4 3,806 / o4 5,343 | d4 3,792 / o4 4,443 |

Item 7 pattern-versus-explicit-reads gap: +973 -> +442 Ir per call (2 names), +1,537 -> +651 (4 names). The rest of the
gap is the pattern path's by-name `declare`/`initialize_binding`/string-keyed property gets (`memcmp`, `foldhash`
lookups on `String` keys), which only a native lowering of simple object patterns removes.

JetStream drivers at `--iterations 1`, whole-process Ir:

| benchmark | base | new | delta |
|---|---:|---:|---:|
| Air | 2,382,092,488 | 2,140,154,982 | -10.16% |
| splay | 10,409,263,928 | 9,838,323,487 | -5.48% |
| Box2D | 8,151,931,650 | 8,054,240,252 | -1.20% |
| Babylon | 3,308,326,475 | 3,287,713,508 | -0.62% |
| bigint-noble-ed25519 | 7,837,257,789 | 7,798,952,626 | -0.49% |
| bigint-noble-bls12-381 | 71,220,927,377 | 70,955,396,513 | -0.37% |
| delta-blue | 8,778,616,682 | 8,773,920,655 | -0.05% |
| richards | 13,399,886,114 | 13,411,664,407 | +0.09% |
| fib (micro) | 7,425,602,898 | 7,428,406,867 | +0.04% |
| hash-map | 18,506,235,116 | 18,549,876,179 | +0.24% |

hash-map's +0.24% is the VM loop's inlining shifting (an earlier iteration had `strict_equality` out of line at
+0.50%; the `string_units_equal` split brought it back). richards and fib have no string work and move by the same
order, so differences below ~0.25% are code-layout noise.

## Not met / left for follow-ups

- 5: the 0-element probe is +711 over the no-spread control (criterion 500) and the 12-element probe +1,691
  (criterion 1,500); the remaining cost is the tree-walker hand-off for the whole call. A native
  `CallSpread`/`ConstructSpread` op is the follow-up.
- 7: o2/o4 are not within 2% of d2/d4 (native lowering of simple object patterns, which needs slot-resolved
  `InitLexical`).
- 6: -8.1% at N=200,000 against the >= 9% criterion; jemalloc (#1047) made the freed blocks cheaper, so the teardown's
  share shrank since the audit.
- 1: Splay's timed-window `binary_slow` + `drop_slow` ledger was not re-measured with callgrind; the whole-process
  Ir drops 5.5%.
- 4: Babylon's dhat `member_get` block count was not re-run; the probes show 0 blocks for the primitive get.

## Correctness gates

`test262-extra/` adds `spread-arguments-intrinsic-array-iteration.js`, `parseInt-number-argument-boundaries.js`,
`object-pattern-binding-excluded-keys.js`, `string-concat-inline-boundary.js` and
`compiler-name-pool-many-names.js`. All five also pass on the base binary (no semantic change).
