Repeated `Intl.PluralRules.prototype.select` rebuilds immutable locale/rule data on every call. A checksum-validated native-method probe takes **0.265434 s on JSSE versus 0.138281 s on Boa (1.920x)**. The complete upstream JetStream PluralRules workload also passes on both engines and takes 0.073895 versus 0.061301 s (1.205x process time; 67 versus 51 ms inner time).

## Reproduction and measurement

JSSE `2ea6417535037ca58d78a433a72a2a0e2dfb4d77`; Boa `f0789a20d87ad52eef6460953929c2b3cdbf8716`. Both default fat-LTO release builds, Rust 1.98.1, AMD Ryzen AI 9 HX 370. Three alternating fresh CLI invocations on CPU 0, sequential. Background desktop load was present; these are diagnostic measurements, not a controlled publication. Probe ranges: JSSE 0.264585–0.269851 s; Boa 0.135926–0.144688 s. All six runs print `2498000`, matching Node v26.8.2.

```js
function main(n) {
  var rules = new Intl.PluralRules("fr-CA");
  var sum = 0;
  for (var i = 0; i < n; i++) sum += rules.select(i % 1000).length;
  return sum;
}
console.log(main(500000));
```

Run the same file with `taskset -c 0 /path/to/jsse repro.js` and `taskset -c 0 /path/to/boa repro.js`. The JetStream measurement is one full validated iteration per fresh process, including initialization; it is not an official JetStream score.

## Profile and potential root cause

Uninstrumented release profiling of four fresh probe invocations (`perf record -F 499 -e cpu-clock:u --call-graph dwarf,8192`) collected 528 samples, zero lost. Self samples include ZeroTrie lookup 11.36%, language-identifier parsing 6.25%, locale fallback parent lookup 4.92%, Locale::try_from_utf8 3.79%, malloc/free 4.73%/3.98%, IntlData clone 2.08%. VM dispatch is 11.55%.

A separate release-nolto counter build reports 6,500,022 VM opcodes, **zero AST work, zero collections, zero local-slot misses**, and 500,002 native calls. The counter build is not used for timing. This is not a generator/AST fallback or GC problem.

At [pluralrules.rs:181–237](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/builtins/intl/pluralrules.rs#L181), every `select` clones the complete IntlData variant, extracts/parses the base locale, constructs preferences/options, and calls `IcuPluralRules::try_new`. Locale and cardinal/ordinal rule type are immutable instance state. `selectRange` similarly recreates ICU range rules around line 297. Category strings also repeatedly allocate a member of a tiny fixed set.

Potential direction: cache ICU rule objects and parsed locale/preferences on the PluralRules instance, retain only the minimal immutable option snapshot needed across ToNumber, and reuse fixed category strings where appropriate. Do not retain a RefCell borrow across observable numeric coercion or reentry.

## Acceptance and scope

Compare repeated select and selectRange, cardinal/ordinal locales, rounding/notation options, NaN/infinities, numeric-coercion throws and reentrant valueOf. Preserve all existing Intl test262 behavior and the full regression baseline. Report default-build wall time and separate deterministic counts.

The two tiny Boa DateTimeFormat scripts also have observed ratios 1.147x and 1.001x, but only 7–9 ms process times and overlapping ranges: they do not establish additional performance defects. Their profiles are retained; the format probe includes canonicalize_timezone (5.78%) and locale lookup, while resolvedOptions includes generic string/property allocation and CLI startup. Do not extrapolate the measured PluralRules diagnosis to DateTimeFormat without a longer controlled probe.

Full evidence is retained locally under `docs/perf/2026-10-14/boa-head-audit/`: `issues/pluralrules-select-measurements.json`, `issues/pluralrules-native-profile.txt`, `issues/pluralrules-native-counters.txt`, and the complete registry profile/counters. This audit changes no implementation.
