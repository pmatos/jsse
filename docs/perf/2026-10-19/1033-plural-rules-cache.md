# #1033: cache ICU plural rules on the PluralRules instance

`Intl.PluralRules.prototype.select` cloned the whole `IntlData` variant (five `String`s), re-parsed the locale
(`base_locale`, `IcuLocale::parse`, `PluralRulesPreferences`) and ran `IcuPluralRules::try_new` (ZeroTrie locale fallback and
rule-data lookup) on every call; `selectRange` did the same with `PluralRulesWithRanges::try_new`. All of it depends only on the
instance's immutable locale and type. Each result also allocated a fresh category string.

## Change

- `IntlData::PluralRules` holds an `Rc<PluralRulesState>` (new `builtins/plural_rules_state.rs`): the option snapshot
  (`PluralRulesOptions`, the old fields) plus the ICU rules, built on first `select` and the range rules built on first
  `selectRange` (`OnceCell`, so a constructor that is never followed by a select costs nothing extra).
- `select`/`selectRange`/`resolvedOptions` clone the `Rc` and drop the object borrow before `ToNumber`, so a throwing or reentrant
  `valueOf` sees the same behaviour as before and no `RefCell` borrow crosses user code.
- The six category strings are built once per thread; a call clones an `Arc` instead of allocating.
- `types.rs` shrinks (13 fields became one `Rc`); no file over the 20,000-byte budget grows.

An ICU construction failure is cached as `None` and answered with `"other"`, as before.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

Base = `origin/main` at `c07afbca` (binary sha256 `2a0ee0c07b19...`), new = base plus this change (`171485ecad4e...`, commit `449adce2`).
cachegrind `--cache-sim=no`, whole-process `I refs`. Wall: one pinned core (CPU 100), alternating runs, `/usr/bin/time`, load
average 1.4-2.1 during the runs (a little above the 1.5 gate; the ratios below are far outside that noise, the control
differences are not and are judged on Ir).

### Target workloads

| workload | base | new | delta |
|---|---:|---:|---:|
| issue repro (`rules.select(i % 1000)` x 500,000 in a function), wall median of 7 | 0.88 s | 0.22 s | -75% |
| same repro, Boa 0.22 on the same host | 0.39 s | | |
| `select` fixture, Ir per iteration ((300k - 100k) / 200k) | 11,389 | 5,333 | -53.2% |
| `selectRange` fixture (`ar`), Ir per iteration | 10,096 | 6,531 | -35.3% |
| `select` fixture x 300,000, wall median of 9 | 0.720 s | 0.300 s | -58% |
| `selectRange` fixture x 300,000, wall median of 9 | 0.670 s | 0.400 s | -40% |
| JetStream `intl/PluralRules` (2 iterations, one process), Ir | 1,309,142,699 | 657,763,389 | -49.8% |
| same, wall median of 9 | 0.300 s | 0.140 s | -53% |

The remaining ~5,000 Ir per iteration of the `select` fixture is the global-scope loop and the native call, not PluralRules: the
same loop calling `Math.abs` instead costs 5,064 Ir per iteration, so `select` itself is now about 300 Ir above a trivial native
call (and the Ir of the JetStream workload is dominated by instance construction and `Math.random`).
`selectRange` carries the two `ToNumber` calls and ICU's range lookup (`category_for_range`).

### Control set (no regression allowed)

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| `bigint-noble-secp256k1` | 4,761,295,246 | 4,763,757,649 | +0.052% |
| `Box2D` | 5,352,224,871 | 5,344,694,129 | -0.141% |
| `delta-blue` | 6,208,700,231 | 6,208,717,990 | +0.000% |
| `fib` | 4,896,557,354 | 4,896,562,166 | +0.000% |
| `gaussian-blur` | 19,901,426,177 | 19,901,424,990 | -0.000% |
| `lazy-collections` | 3,711,024,685 | 3,711,689,977 | +0.018% |
| `opmix` | 38,800,939,981 | 38,800,943,021 | +0.000% |
| `richards` | 8,448,469,109 | 8,448,440,138 | -0.000% |
| `splay` | 7,579,081,846 | 7,573,455,885 | -0.074% |

Peak RSS (median of 5 runs, KB, base -> new): fib 17,028 -> 17,284; richards 19,216 -> 19,272; delta-blue 23,348 -> 23,516;
Box2D 39,348 -> 39,240; lazy-collections 27,812 -> 27,984; secp256k1 40,264 -> 39,988; opmix 17,660 -> 17,428; gaussian-blur
21,088 -> 21,236; splay 595,076 -> 596,180. Run-to-run spread within one binary is about 1% and the two binaries trade places, so
there is no growth beyond noise. The PluralRules fixtures sit at 17.6-18.7 MB in both builds.

Control wall (median of 5): fib 0.92 -> 0.92 s, richards 1.91 -> 1.92, delta-blue 1.39 -> 1.38, Box2D 1.22 -> 1.22,
lazy-collections 0.86 -> 0.84, secp256k1 1.13 -> 1.14, opmix 6.97 -> 6.93, gaussian-blur 3.98 -> 3.94, splay 2.03 -> 2.03. An
earlier build of the same change (before the review cleanups) read `richards` +1.6% slower over 15 runs with Ir flat; the final
build does not, which confirms that was code-layout noise between two distinct binaries, not executed work (neither runs any
PluralRules or `IntlData` code).

## Tests

`test262-extra/Intl-PluralRules-select-instance-state.js`: interleaved instances of different locale/type/notation, repeated calls,
NaN/Infinity/string inputs, `selectRange` errors, a throwing `valueOf` leaving the instance usable, reentrant `valueOf` calling
`select`/`selectRange` on the same instance, coercion order (`start` before `end`), `resolvedOptions` stability, foreign receivers.
Expected values were checked against Node v26.8.2 (Node answers `selectRange` for ordinal rules differently from JSSE, which is
unchanged by this PR, so the test does not assert ordinal ranges).
