# Sticky RegExp: anchored search, slot-cached matcher, multi-entry subject cache (#921)

## Change

js-tokens spent its time in `RegExp.prototype.exec`: every failed sticky attempt
ran an unanchored search over the rest of a 14 KB subject and rejected a match
that did not start at `lastIndex`, and every exec re-derived constants and
reconverted the subject.

- **Anchored search.** A sticky (`y`) match now runs anchored at `lastIndex`
  (`regex_captures_at(.., anchored)`): `fancy_regex::RegexInput::anchored(true)`
  for the backtracking VM (its `SplitUnanchored` and seek preamble both honour
  `is_anchored()`), `regex_automata::meta` `Anchored::Yes` for the plain and WTF-8
  matchers. The `regex` crate cannot anchor at an offset, so the plain and bytes
  matchers are now built through the meta engine directly with exactly the
  `regex` crate's default configuration (`regex-automata` is a direct dependency
  with the feature set `regex` already enables; error text still comes from
  `regex`). The custom-lookbehind matcher cannot anchor, so
  `regex_captures_at` enforces the contract by rejecting an anchored result that
  starts elsewhere. `@@match`, `@@replace` and `@@split` (which execs a sticky
  splitter at every position) all take the same path.
- **Matcher cached on the RegExp slot.** `RegExpData.exec_plan` memoizes an
  `ExecPlan` (decoded pattern text, flag bits, `Rc<CachedRegex>`) on first exec.
  It is derived from `source`/`flags`, which only change when `compile()`
  replaces the whole `RegExpData`, so it cannot go stale; the exec path reads it
  after `ToLength(lastIndex)`, as the spec requires. This removes the per-exec
  pattern/flags conversion (done twice before) and the owned `(source, flags)`
  key allocation; the process-wide cache is now `flags -> source -> regex` and
  looks up by borrowed `&str`.
- **Capture-reset tables derived once.** `build_quantified_parent_map` is a pure
  function of the source and was rescanned on every successful match; it is now
  a `OnceCell` on `CachedRegex`.
- **Subject cache.** The single-entry `RegexInput` cache is replaced by
  `SubjectCache`: subjects of at least 256 code units live in a 4-entry LRU keyed
  by `JsString` identity, shorter ones share a separate single slot, so a
  `.test(shortString)` between two execs of the same long subject no longer
  evicts it.

## Measurement

Host load average 25-37 throughout (shared host, other jobs running), so only
the ratios carry weight. Interleaved, median of 5 process runs, wall seconds;
`before` is `origin/main` at `a4773823`, `boa` is 0.22.0.

| benchmark | before | after | boa 0.22 |
| --- | --- | --- | --- |
| `bench_generators_tokenizer.js` | 1.17 | 0.83 | 0.60 |
| `bench_generators_sticky_regex.js` | 1.32 | 0.020 | 0.47 |
| `bench_generators_sticky_lookahead.js` | 0.62 | 0.009 | 0.53 |
| `benchmarks/scripts/bench_regex.js` (median of 21) | 0.041 | 0.037 | 0.078 |

JetStream 3 through `scripts/run-jetstream.py --iterations 1 --repeats 5
--no-idle-gate` (score, higher is better, two rounds each):

| benchmark | before | after | boa 0.22 |
| --- | --- | --- | --- |
| js-tokens | 0.8 / 1.0 | 4.9 / 4.5 | 2.3 |
| regexp-octane | 7.1 / 7.3 | 9.7 / 9.4 | 4.3 |

`tokenizer` is 1.4x boa on this host (target 1.8x); `sticky_lookahead` is 0.009 s
(target 0.2 s); regexp-octane is not regressed.

## Not measured

The two callgrind criteria (`fancy_regex::vm::run_with` under 20% and
`js_string_to_regex_input_mode` under 2% of js-tokens instructions) need
`valgrind`, which this host lacks; they have to be confirmed on buildbox3. The
wall-time evidence above is consistent with them: a failed sticky attempt is
O(1) in the length of the remaining subject (`sticky_lookahead`, 14 KB subject,
1500 attempts: 0.62 s to 0.009 s) and a long subject is converted once however
many short subjects interleave (`regex_input_cache_tests`,
`RegExp-subject-conversion-interleaved.js`).
