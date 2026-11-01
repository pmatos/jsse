# #1401: smaller and shared property storage

Part of #1401 (epic #1214). After the tag-byte index (`docs/perf/2026-10-28/1401-property-tag-index.md`)
tsc compiling its own sources on `jsse --node` still peaked at 4.6 to 5.7 times Node's RSS. This note records
the next four reductions, each measured on its own, and what is left.

## Method

- `scripts/run-tsc-selfcompile-oracle.py` for the final table (Node and jsse, one process at a time, output
  byte-identical to Node in every case). For the per-stage rows, the same `compiler-dts` compile of one binary
  at a time (`ru_maxrss` through `wait4`).
- Non-LTO release binaries (`CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`), as in the
  issue; an LTO run does not move RSS.
- The host was loaded (load average 4 to 20, other agents), so wall time is not comparable across runs. Time was
  judged by user CPU of interleaved runs (minimum of 5 to 7) and by microbenchmarks; valgrind is not installed
  here, so there is no instruction count.
- Census: a temporary (uncommitted) allocator wrapper that counts live allocations by size, printed after each
  major collection, plus a walk of the object arena.

## Where the memory was (after the tag index, `compiler-dts`, 2.9M live objects, 2997 MiB)

| part | MiB |
| --- | ---: |
| object blocks (320 bytes of data plus 48 of header, 1024 per chunk) | 940 |
| entry lists of 1.3M maps of 9 to 20 properties: 48 bytes an entry, 25% spare capacity | 1183 |
| entry lists of 0.5M maps of 4 to 8 properties | 165 |
| everything else (AST of `_tsc.js`, strings, environments, bytecode, GC tables) | 700 |

The largest block of that was the same ordered key list, with its attributes, repeated in every AST node,
symbol and type.

## Changes (one commit each)

| stage | change | `compiler-dts` MiB |
| --- | --- | ---: |
| base | main after #1407 | 2997 |
| 1 | `PropertyRecord` 32 to 16 bytes (a value and a flag word; the accessor halves are boxed). Entry-list growth by a quarter rounded to a jemalloc size class (spill at 10 slots), not doubling | 2233 |
| 2 | Shapes: an object that starts empty keeps an `Rc<Shape>` (ordered keys, lookup tags, children by weak transition) and one 16-byte record per key | 1859 |
| 3 | `PropertyMap` 120 to 64 bytes: hashed index boxed, one entry inline per-object, two records inline shaped. `JsObjectData` 272 to 216 | 1697 |
| 4 | Shapes also record each key's attributes; a shaped object stores only the 8-byte value. Complete data properties only; `RecordRef`/`RecordMut` replace `&PropertyRecord` at call sites, `get_stored_mut` is the way to change attributes | 1485 |

Shapes are paused while a realm's builtins and the Node host floor are created (each is a family of one; a
shape per property slowed startup and spent the shape budget), and a map leaves shapes, for good, on a
deletion of anything but the last key, an accessor or partial descriptor, an attribute change, more than 48
keys, a child table of 512, or 50,000 live shapes on the thread.

## Final table

`scripts/run-tsc-selfcompile-oracle.py`, output identical to Node in all four cases. Before is the table of
#1407 (same host class, same configuration).

| case | Node MiB | jsse before | jsse after | ratio before | ratio after |
| --- | ---: | ---: | ---: | ---: | ---: |
| compiler-dts | 655 | 2986 | 1486 | 4.6 | 2.3 |
| compiler-js | 702 | 4044 | 2287 | 5.7 | 3.3 |
| src-js | 903 | 5065 | 2739 | 5.6 | 3.0 |
| generated-large | 159 | 359 | 292 | 2.3 | 1.8 |

The `max_rss_ratio` budget (6) is unchanged.

## Time

- `compiler-dts` user CPU, same host state, one run each: 207 s (stage 3), 193 s (stage 4); the base ran in
  251 s to 278 s wall on a loaded host. Fewer bytes per object also means less to trace and to cache-miss on.
- `jsse -e 0`: 4.1 ms before, 4.3 ms after (minimum of 40); `jsse --node -e 0`: 25.6 ms and 26.3 ms.
- Microbenchmark (7 interleaved runs, minimum, ms; before, after): 13-property constructor 187, 175;
  property reads 154, 137; object literal of 8 keys 63, 54; `{x, y}` 111, 80; 40-key dictionary 38, 39.
- Regression: deleting a key other than the last on a small shaped object (`{a,b,c,d,e}` then `delete o.c`,
  100k times) went from 52 ms to 107 ms. The object converts to the per-object form once; the cost is that
  conversion. Nothing in the tsc runs does this.
- `bench_object`, `bench_opmix`, `bench_closures`, `bench_array`, `bench_json`, `bench_fib`: within the noise of
  this host (user CPU, 5 interleaved runs).

## Checks

Full test262 (99,911 of 99,911 scenarios), `test262-extra` normal, `--no-bytecode`, `JSSE_GC_STRESS=7` and
on a `release-checked` binary, `cargo test --profile release-nolto` (1,851 tests), clippy and rustfmt. New
`test262-extra` tests cover property order, values and attributes across every capacity step, redefinition
between data and accessor, deletion and re-adding, and objects that diverge after the same construction.

## What is left (`compiler-dts`, 1486 MiB)

- Object blocks are 60%: 216 bytes of `JsObjectData` and 48 of header. `primitive_value` (16, only wrappers and
  dates), `id` (16, duplicated in the header), `class_name` (16) and the 48-byte `ObjectKind` are the
  candidates; each is about 2% and touches 50 to 400 call sites.
- The arena keeps 14% more blocks than are live at the last collection (its high-water mark); a GC pacing
  trial (major at 50% or 25% of the live set instead of 100%) bought 2% and 3% of RSS for 1.5% and 15% of time,
  so it was not adopted.
- Shrinking spare capacity of surviving maps at each major collection removed 58 MiB at the collection but
  not from the peak, so it was not adopted.
