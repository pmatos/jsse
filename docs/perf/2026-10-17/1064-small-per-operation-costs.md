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
- **2 name pool.** `Compiler::add_name` scans pools of up to 16 names by text (no interning on a hit); beyond that it
  keeps a `HashMap<Atom, u16>` keyed on the interned atom's precomputed hash. Pool order is unchanged.
  `add_private_name` keeps its scan (private-name pools are a handful of entries).
- **3 parseInt.** A Number argument with radix `undefined` returns `trunc(n)` for `1e-6 <= |n| < 1e21` (the range where
  the string form has no exponent), `+0` for zeros and NaN for NaN/Infinity; everything else takes the string path.
- **5 spread.** `iterate_to_vec` copies a plain dense Array when `@@iterator` is the realm's original
  `Array.prototype.values` on `%Array.prototype%` and `%ArrayIteratorPrototype%.next` is the original; holes need
  `proto_chain_index_free`. No iterator object is created. This helps every `iterate_to_vec` caller, not just calls.
- **6 exit teardown.** `cli::retire_interpreter` forgets the interpreter in release builds (debug builds, including
  `release-checked`, still drop it so the arena and root-stack assertions run). Nothing in the `Drop` impls has an
  observable effect.
- **7 object patterns.** `excluded_keys` is only built when the pattern has a rest element, in binding
  (`bind_pattern`) and assignment (`destructure_object_assignment`) patterns alike.
- `JsString::from_units(&[u16])` builds a short slice copy straight into the inline form (substring, split, regexp
  captures, literals) instead of through a temporary `Vec`.

Already fixed (no change): **4 primitive-string `.length`/method get.** `access_property_on_value` already tests
`length` and the index on the borrowed key; the probes below show 0 blocks and a primitive `.length` cheaper than an
object `.length`.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

cachegrind `--cache-sim=no`. base = `origin/main` `36ccf740` (binary sha256 `01ed35ea6416...`), new = this change
(`4d21e8568300...`). Loop probes: Ir per iteration = (20,000 minus 2,000 iterations) / 18,000. Blocks come from dhat on
`--no-default-features` builds of the same trees (jemalloc hides allocations from dhat; the block counts below are
from the earlier build of this change, before `from_str`/`from_units` went inline, so they are upper bounds).

| probe | base | new | note |
|---|---:|---:|---|
| 1 `"..." + tag + "..."` (control `last = tag`: 1,335) | 2,562 | 2,262 | net 1,227 -> 925 Ir (-24.6%); blocks 6 -> 2 per iteration (1 per result) |
| 3 `parseInt(i*0.5+3.25)` (`Math.trunc` control: 3,030) | 3,756 | 2,627 | net over trunc +726 -> -405 Ir; blocks 1 -> 0 |
| 4 `s.length` on a primitive (object `.length`: 1,600) | 1,490 | 1,493 | already 0 blocks; `s.charCodeAt(5)` 2,708 -> 2,712, 0 blocks |
| 4' `s[5]` (one-unit string result) | 2,129 | 2,099 | blocks 3 -> 2 |
| 5 `g(...[])` (control `g(a0,a1,a2)`: 3,804) | 6,763 | 4,512 | +2,959 -> +701 over control |
| 5 `g(...[1,2,3])` | 8,888 | 4,956 | +5,084 -> +1,145 |
| 5 `g(...[12 elements])` | 15,224 | 5,491 | +11,420 -> +1,680 |
| 5 `new C(i, ...args)` / `a.push(...args)` | 12,177 / 12,601 | 8,395 / 8,647 | |

| probe (whole process) | base | new |
|---|---:|---:|
| 2 name pool K=500, distinct minus same | 7.51M (15.0K/name) | 1.59M (3.2K/name) |
| 2 K=2000 | 80.16M (40.1K/name) | 6.60M (3.3K/name) |
| 2 K=4000 | 381.32M (95.3K/name) | 13.66M (3.4K/name) |
| 6 retained heap N=50,000 | 668.25M | 608.73M (-8.9%) |
| 6 retained heap N=200,000 | 2,815.04M | 2,582.42M (-8.3%) |
| 7 `d2` / `o2` (Ir per call, minus `none`) | 1,824 / 2,797 | 1,826 / 2,252 |
| 7 `d4` / `o4` | 3,806 / 5,343 | 3,792 / 4,438 |

Item 7 pattern-versus-explicit-reads gap: +973 -> +426 Ir per call (2 names), +1,537 -> +646 (4 names). The rest of the
gap is the pattern path's by-name `declare`/`initialize_binding`/string-keyed property gets (`memcmp`, `foldhash`
lookups on `String` keys), which only a native lowering of simple object patterns removes.

JetStream drivers at `--iterations 1`, whole-process Ir:

| benchmark | base | new | delta |
|---|---:|---:|---:|
| Air | 2,382,092,488 | 2,134,347,378 | -10.40% |
| splay | 10,409,263,928 | 9,843,372,287 | -5.44% |
| Box2D | 8,151,931,650 | 8,047,666,884 | -1.28% |
| Babylon | 3,308,326,475 | 3,286,724,155 | -0.65% |
| bigint-noble-bls12-381 | 71,220,927,377 | 70,838,200,411 | -0.54% |
| bigint-noble-ed25519 | 7,837,257,789 | 7,799,053,520 | -0.49% |
| delta-blue | 8,778,616,682 | 8,771,938,479 | -0.08% |
| fib (micro) | 7,425,602,898 | 7,427,550,759 | +0.03% |
| richards | 13,399,886,114 | 13,410,710,700 | +0.08% |
| hash-map | 18,506,235,116 | 18,546,696,706 | +0.22% |

hash-map's +0.22% is the VM loop's inlining shifting (an earlier iteration had `strict_equality` out of line at
+0.50%; the `string_units_equal` split brought it back). richards and fib have no string work and move by the same
order, so differences below ~0.25% are code-layout noise.

Memory and long-append controls (the inline variant makes every `StrView` 112 bytes, so these were checked):
`INLINE_UNITS` 24 / 32 / 40 gave splay Ir 9.894G / 9.894G / 9.828G, splay maxrss +2.2% / +3.4% / +4.4% over base
(537 MB), and the same 2.78 s splay wall at all three against 2.85 s for base (5 interleaved runs). A 20,000-step
`s += "x"` chain costs 1,267 -> 1,279 Ir per append, and retaining every prefix of a 40 KB built string raises maxrss
20.5 -> 21.5 MB. 40 stays: smaller values lose the 35-unit concat probe (net 1,085 Ir at 24 instead of 925).
A release build now leaves the heap undropped at exit, so a LeakSanitizer or valgrind leak check of a release binary
reports it as leaked; debug builds still tear it down.

## Not met / left for follow-ups

- 5: the 0-element probe is +701 over the no-spread control (criterion 500) and the 12-element probe +1,680
  (criterion 1,500); the remaining cost is the tree-walker hand-off for the whole call. A native
  `CallSpread`/`ConstructSpread` op is the follow-up.
- 7: o2/o4 are not within 2% of d2/d4 (native lowering of simple object patterns, which needs slot-resolved
  `InitLexical`).
- 6: -8.3% at N=200,000 against the >= 9% criterion; jemalloc (#1047) made the freed blocks cheaper, so the teardown's
  share shrank since the audit.
- 1: the concat probe is -24.6% net (criterion 25%); one block per result is the floor of this layout.
- 1: Splay's timed-window `binary_slow` + `drop_slow` ledger was not re-measured with callgrind; the whole-process
  Ir drops 5.4%.
- 4: Babylon's dhat `member_get` block count was not re-run; the probes show 0 blocks for the primitive get.

## Correctness gates

`test262-extra/` adds `spread-arguments-intrinsic-array-iteration.js`, `parseInt-number-argument-boundaries.js`,
`object-pattern-binding-excluded-keys.js`, `string-concat-inline-boundary.js` and
`compiler-name-pool-many-names.js`. All five also pass on the base binary (no semantic change).
