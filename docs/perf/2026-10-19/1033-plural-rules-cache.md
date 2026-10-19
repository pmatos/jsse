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

Base = `origin/main` at `c07afbca` (binary sha256 `2a0ee0c07b19...`), new = base plus this change (`736f93d4202b...`).
cachegrind `--cache-sim=no`, whole-process `I refs`. Wall: one pinned core (CPU 100), alternating runs, `/usr/bin/time`, load
average 1.4-2.1 during the runs (a little above the 1.5 gate; the ratios below are far outside that noise, the control
differences are not and are judged on Ir).

### Target workloads

| workload | base | new | delta |
|---|---:|---:|---:|
| issue repro (`rules.select(i % 1000)` x 500,000 in a function), wall median of 7 | 0.87 s | 0.22 s | -75% |
| same repro, Boa 0.22 on the same host | 0.39 s | | |
| `select` fixture, Ir per iteration ((300k - 100k) / 200k) | 11,389 | 5,333 | -53.2% |
| `selectRange` fixture (`ar`), Ir per iteration | 10,096 | 6,531 | -35.3% |
| `select` fixture x 300,000, wall median of 9 | 0.710 s | 0.310 s | -56% |
| `selectRange` fixture x 300,000, wall median of 9 | 0.660 s | 0.390 s | -41% |
| JetStream `intl/PluralRules` (2 iterations, one process), Ir | 1,309,142,699 | 646,796,612 | -50.6% |
| same, wall median of 9 | 0.300 s | 0.140 s | -53% |

The remaining ~5,000 Ir per iteration of the `select` fixture is the global-scope loop and the native call, not PluralRules: the
same loop calling `Math.abs` instead costs 5,064 Ir per iteration, so `select` itself is now about 300 Ir above a trivial native
call (and the Ir of the JetStream workload is dominated by instance construction and `Math.random`).
`selectRange` carries the two `ToNumber` calls and ICU's range lookup (`category_for_range`).

### Control set (no regression allowed)

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| `bigint-noble-secp256k1` | 4,761,295,246 | 4,758,322,404 | -0.062% |
| `Box2D` | 5,352,224,871 | 5,356,511,446 | +0.080% |
| `delta-blue` | 6,208,700,231 | 6,209,172,621 | +0.008% |
| `fib` | 4,896,557,354 | 4,896,559,823 | +0.000% |
| `gaussian-blur` | 19,901,426,177 | 19,901,423,899 | -0.000% |
| `lazy-collections` | 3,711,024,685 | 3,711,993,878 | +0.026% |
| `opmix` | 38,800,939,981 | 38,800,941,180 | +0.000% |
| `richards` | 8,448,469,109 | 8,449,189,424 | +0.009% |
| `splay` | 7,579,081,846 | 7,574,311,669 | -0.063% |

Peak RSS (median of 5 runs, KB, base -> new): fib 17,276 -> 17,084; richards 19,208 -> 19,428; delta-blue 23,444 -> 23,420;
Box2D 39,044 -> 39,136; lazy-collections 27,960 -> 27,920; secp256k1 40,344 -> 40,356; opmix 17,508 -> 17,596; gaussian-blur
21,432 -> 21,184; splay 596,896 -> 596,312. Run-to-run spread within one binary is about 1% (richards 19,168-19,500 for base), and the
two binaries trade places, so there is no growth beyond noise. The PluralRules fixtures sit at 17.5-18.8 MB in both builds.

Control wall (median of 5): fib 0.92 -> 0.91 s, delta-blue 1.38 -> 1.39, lazy-collections 0.86 -> 0.85, secp256k1 1.12 -> 1.12,
opmix 6.90 -> 6.85, gaussian-blur 4.00 -> 3.90, splay 2.03 -> 2.04. `richards` (1.91 -> 1.94 s, +1.6% over 15 runs) and `Box2D`
(1.22 -> 1.23 s) read slightly slower while their Ir is flat (+0.009%, +0.080%) and neither runs any PluralRules or
`IntlData` code: this is code-layout noise between two distinct binaries, not executed work.

## Tests

`test262-extra/Intl-PluralRules-select-instance-state.js`: interleaved instances of different locale/type/notation, repeated calls,
NaN/Infinity/string inputs, `selectRange` errors, a throwing `valueOf` leaving the instance usable, reentrant `valueOf` calling
`select`/`selectRange` on the same instance, coercion order (`start` before `end`), `resolvedOptions` stability, foreign receivers.
Expected values were checked against Node v26.8.2 (Node answers `selectRange` for ordinal rules differently from JSSE, which is
unchanged by this PR, so the test does not assert ordinal ranges).
