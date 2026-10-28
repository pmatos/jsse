# #1401: tag-byte index for spilled property maps

Part of #1401 (epic #1214). tsc 6.0.3 compiling its own sources on `jsse --node` peaked at
3.7-6.0 GiB, 5.7-6.6x Node. This note records where the memory was and what the first reduction
bought.

## Where the memory goes

A census taken after a major collection (temporary instrumentation, not committed) of
`compiler-js` at 2.68M live objects and 3.4 GiB RSS: 2.1M plain objects (AST nodes, symbols,
types) and 0.44M arrays; 21.8M property entries in 1.26M spilled maps (nine or more properties,
typically 11-14 for a node) and 0.5M heap-backed linear maps. Garbage is not the problem: RSS
right after a collection equals the high-water mark, so the live set is the peak.

| part | share of RSS |
| --- | ---: |
| `HashMap<JsPropertyKey, u32>` index of spilled maps (25+ bytes per entry) | 19% |
| entry lists (`Vec<Option<(JsPropertyKey, PropertyRecord)>>`, 48 bytes per entry, doubling slack 18%) | 39% |
| `JsObjectData` blocks (384 bytes each) | 28% |
| everything else (AST of `_tsc.js` ~160 MiB, strings, environments) | 14% |

## Change

`SlotIndex` (`src/interpreter/property_map/slot_index.rs`): up to 64 entry slots the index is one
tag byte per slot (eight per `u64`), scanned a word at a time; only slots whose tag matches are
verified against the entry's key. Beyond 64 slots the randomised `HashMap` is kept. The tag is a
cheap unseeded function of the key; a crafted collision costs at most 64 key compares per lookup.
Insertion no longer clones the key `Arc`.

## Result

Same non-LTO release configuration as the issue, `scripts/run-tsc-selfcompile-oracle.py`, output
byte-identical to Node in every case (RSS is `ru_maxrss`; the three jsse runs overlapped, so only
RSS is comparable):

| case | before MiB | after MiB | change |
| --- | ---: | ---: | ---: |
| compiler-dts | 3690 | 2986 | -19% |
| compiler-js | 4742 | 4044 | -15% |
| src-js | 6030 | 5065 | -16% |
| generated-large | 384 (60 modules) | 359 | -7% |

Ratio to Node: 5.7/6.4/6.6 became 4.6/5.7/5.6.

## Time

buildbox3 (quiet, pinned core, fat-LTO release, base = `a979b9ca`):

- Whole tsc compile of 200 generated modules, 5 interleaved runs: wall median 52.00 s before,
  51.92 s after; RSS 624 -> 558 MiB. A first set of five read +2% and a second -0.2%, so the
  difference is noise.
- cachegrind Ir, tsc over 100 generated modules: 84,230,246,068 before, 84,404,535,050 after
  (+0.2%).
- Lookup microbenchmark (12- and 40-property objects, dynamic hits, misses, `in`) Ir: 1,247.7M
  before, 1,270.7M after (+1.8%); wall median 2.800 s / 2.783 s.
- `bench_object` 0.374/0.381 s, `bench_opmix` 7.648/7.501 s, `bench_json` 0.088/0.088 s,
  `bench_closures` 1.433/1.424 s, `bench_array` 0.168/0.176 s, `bench_string` 0.083/0.084 s,
  `bench_fib` 1.034/1.072 s (no spilled maps there: layout lottery, see #1100).

## Next candidates (not in this change)

- Entry-list growth by a quarter instead of doubling: about -3% more on the generated program
  (555 -> 539 MiB); needs a benchmark with objects of tens of thousands of keys first.
- `PropertyRecord` 32 -> 24 bytes (getter and value share a slot): 8 of every 48 entry bytes,
  about -5%; every writer of an accessor/data pair must keep them exclusive.
- Caching the tag in the key allocation so a lookup does not hash.
