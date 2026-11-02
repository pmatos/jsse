# #1401: object blocks and AST nodes

Part of #1401 (epic #1214). After the shared property storage (`1401-shared-property-storage.md`) tsc
compiling its own sources on `jsse --node` still peaked at 1.8 to 3.3 times Node's RSS. That note named
the next levers: the 256-byte object block (60% of the heap), `primitive_value`, `id`, `class_name` and the
48-byte `ObjectKind`. This note records those and one it did not name, the size of the AST.

## Method

- Peak RSS (`ru_maxrss` through `wait4`) of one `compiler-dts` compile (`-p src/compiler`, as
  `scripts/run-tsc-selfcompile-oracle.py`) per binary, one process at a time; non-LTO release binaries
  (`CARGO_PROFILE_RELEASE_LTO=off`, 16 codegen units). Final table: the oracle itself, all four cases, output
  byte-identical to Node.
- Type sizes from `-Zprint-type-sizes` (`RUSTC_BOOTSTRAP=1`), which showed `ObjectAllocation` at 256 bytes
  (216 of `JsObjectData`, 8 of `RefCell` flag, 32 of header) and `ObjectKind` at 48 because `ArrayData` and
  `IterHelperData` are inline.
- A temporary counting allocator (not committed) printed live allocations by size class once 1000 MiB were
  live. After the object-block stages the AST (a `Statement` of 256 bytes, an `Expression` of 112) and the
  per-object value arrays of 113 to 128 bytes (one million of them, 123 MiB) were the next largest parts.
- The host was shared (load average 10 to 17), so wall time is not comparable across runs; time was judged
  by user CPU and by interleaved microbenchmarks (minimum of 5 to 11 runs).

## Changes (`compiler-dts`, MiB)

| stage | change | bytes per object block | RSS |
| --- | --- | ---: | ---: |
| base | main at 73f192da | 256 | 1485 |
| 1 | `ObjectKind` 48 to 16 bytes: `Array`, `RegExp`, `IterHelper`, `WrappedFunction`, `DisposableStack`, `WeakRef`, `PrimitiveWrapper` payloads boxed | 224 | 1406 |
| 2 | `JsObjectData.id` removed (the arena header keeps it, `ObjectHandle::id()`), header `strong` and `id` 32-bit; `primitive_value` moved into the lazily boxed `ObjectExtras`; `[[Prototype]]` a 32-bit slab index (`proto_id()`, `set_proto`) | 176 | 1250 |
| 3 | `class_name` a one-byte `ClassId` into a static table (`class_str()`, `set_class`, `is_class`; a name outside the table goes to a per-thread overflow list) | 160 | 1197 |
| 4 | function realm and generator realm share one slot | 152 | 1169 |
| 5 | `Expression` 112 to 56 bytes and `Statement` 256 to 72: function, arrow, class and template expressions and every statement variant above 60 bytes are boxed | 152 | 1108 |

An object that has `extras` (private fields, class field definitions, native roots) now pays 16 more bytes
for the primitive slot (72 bytes, allocator class 80 instead of 56); wrappers and Dates, which pay for a
box they did not have before, are rare in the tsc runs.

`JsObjectData` went from 216 to 120 bytes (cap in `js_object_data_size_cap`); loading the 6 MB `_tsc.js`
alone (`--version`) went from 201 to 147 MiB, 48 MiB of that from `Expression` alone.

## Final table

`scripts/run-tsc-selfcompile-oracle.py`, `--jobs 1`, output identical to Node in all four cases. Before is
the table of `1401-shared-property-storage.md` (same host class and configuration).

| case | Node MiB | jsse before | jsse after | ratio before | ratio after |
| --- | ---: | ---: | ---: | ---: | ---: |
| compiler-dts | 631 | 1486 | 1113 | 2.3 | 1.8 |
| compiler-js | 704 | 2287 | 1657 | 3.3 | 2.4 |
| src-js | 914 | 2739 | 1995 | 3.0 | 2.2 |
| generated-large | 159 | 292 | 211 | 1.8 | 1.3 |

The `max_rss_ratio` budget (6) is unchanged.

## Time

- `compiler-dts` user CPU: 226 s at stage 5 against 240 to 300 s for the earlier stages and 312 s for the
  base, all on a loaded host (some with a test run beside them), so only the direction is meaningful: smaller
  blocks, ASTs and records are not slower.
- `jsse -e 0`: 5.0 ms before and after; `jsse --node -e 0`: 35 ms before and after (minimum of 40).
- Microbenchmarks (user CPU, minimum of 5 to 11 interleaved runs, base then final): `bench_loop`
  -1.6%, `bench_opmix` -4.3%, `bench_fib` -1.5%, `bench_closures` -29%, `bench_object` -17%,
  `bench_promise_new` -11%, `bench_json` -15%. 2M small arrays created: -3.4%; reads and writes of arrays
  in a small live set: -3.5%.
- One measured cost: a loop over 1.5M live small arrays (the `[i, i+1, i+2]` shape) is 5% slower, 5.80 s
  to 6.08 s, because an element access now goes object block, boxed `ArrayData`, element buffer, one more
  dependent load on a cache-hostile working set; the same program peaks at 396 MiB instead of 493.

## Checks

Full test262 (`test262-pass.txt` unchanged), `test262-extra` normal, `--no-bytecode`, `JSSE_GC_STRESS=7`
and on a `release-checked` binary, `cargo test --profile release-nolto`, clippy and rustfmt. Under
`JSSE_GC_STRESS=7 --no-bytecode` the two scenarios of `gc-minor-collection-dirty-environment-list.js` time
out at 120 s, on the base binary as well (stress cost, not a rooting failure). New `test262-extra` tests
read back wrapper and Date slots, a FinalizationRegistry, exotic kinds and prototypes after collections that
recycle slots.

## What is left (`compiler-dts`, 1113 MiB)

- Object blocks are 152 bytes each (28 of header: a `Rc<ObjectStorage>` pointer, the `RefCell` flag, 32-bit
  strong count and id, four GC bytes; 120 of data: `PropertyMap` 64, `callable`, `extras`, `shape_id`, kind).
  The `storage` pointer could live once per chunk (a header at an aligned chunk base) for 8 bytes an object;
  `shape_id` is a 64-bit global counter and stays so.
- Per-object value arrays of shaped objects (8 bytes a property, allocator class rounding) are the next
  largest part after the blocks; every AST `Identifier` still owns a `String`.
- The arena keeps about 14% more blocks than are live at the last collection; pacing majors earlier bought
  2% to 3% for 1.5% to 15% of time in the previous note and was not retried.
- The issue stays open: the ratio is 1.3 to 2.4 against a budget of 6, but it has no acceptance criterion and
  the remaining levers are each 1% to 3%.
