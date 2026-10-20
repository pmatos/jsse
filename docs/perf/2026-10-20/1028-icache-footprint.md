# #1028: create-and-call loops (superadditive cost)

`docs/perf/2026-10-19/1028-closure-construction.md` left one gap: a loop that creates a closure and calls it in the same iteration
(`var f = function (x) { return x + i; }; s += f(1)`, function scope, x3M) ran 3.67 s against boa 2.96 s at equal Ir, while create-only
and call-only were each at parity or faster. It read that as instruction-cache pressure from alternating the two paths. That was a
symptom. The cause was a call-path miss.

## Cause

`call_function_general` installs a literal's `CompiledCall` record (the data `call_compiled_function` serves from) only if the
**callee's own** `bytecode_cache` is `Compiled` at call entry. A closure starts `Untried` and resolves the literal's shared memoised
verdict (#900) inside `dispatch_body`, after the install check. A closure that is called repeatedly installs the record on its second
call; a closure that is created and called once never does, and neither does any later closure of the same literal. Every call in the
create-and-call loop therefore took the general path: 479 Ir per call in `call_function_general` (cachegrind, 300k iterations),
`memoised_bytecode_verdict` and `dispatch_body` on top, and 18% of all simulated I1 misses. Call-only loops reach the fast path on the
second call, which is why they looked fine.

## Change

- `eval/compiled_call_record.rs`: `install_compiled_call` (the old inline block, moved out of `call_function_general`, so `eval.rs`
  shrinks by 1 KB) resolves the shared verdict for an `Untried` closure *before* the install check and hands the resolved state on to
  `dispatch_body`. The first call of any closure now publishes the record, and every later closure of the literal takes
  `call_compiled_function`.
- Function own properties are built from `PropertyRecord::data` / `getter_only`, which set the flag word directly, instead of
  assembling a `PropertyDescriptor` of `Option` fields that `From` packs again (about 70 Ir per property, five per sloppy closure, one
  more for `prototype.constructor`). `PropertyRecord`'s fields and flag constants stay private to `property_map`; the constructors live
  in `property_map/exact_capacity.rs`, and `exact_capacity.rs` has unit tests comparing both against the `From<PropertyDescriptor>`
  packing.
- `create_function_with_name` calls one `#[inline(never)]` routine, `push_function_props`, for `length`, `name`, `caller`/`arguments`
  and `prototype`, instead of carrying those arms inline.

No observable behaviour changes (same properties, same order, same attributes; the verdict is resolved earlier in the same call, not
differently), so no test262-extra test applies.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, jemalloc, nightly, `--locked`; base = `630a7c8f`)

Wall: pinned with `taskset -c 61`, interleaved runs under `.checkpoint.lock`, median; the null control is a byte-identical copy of the
base binary (`base2`). Host loadavg1 was 1.6-2.7 from other users, so `base` versus `base2` varies by up to 1% on a given row.

| row (function scope) | base | new | delta | null control |
|---|---:|---:|---:|---:|
| sloppy fn expression, create + call x3M | 3.592 s | 3.176 s | **-11.6%** | -0.0% |
| arrow, create + call x1.5M | 1.320 s | 1.159 s | -12.2% | -0.8% |
| strict fn, create + call x1.5M | 1.615 s | 1.422 s | -12.0% | -0.0% |
| sloppy fn + arrow x500k | 1.039 s | 0.934 s | -10.1% | +0.1% |
| makeAdder x1.5M | 2.713 s | 2.404 s | -11.4% | -0.8% |
| create only x3M | 2.014 s | 1.969 s | -2.3% | -0.2% |
| call only x3M | 1.297 s | 1.299 s | +0.2% | -0.1% |
| array literal `[i,i+1,i+2]` x3M | 1.425 s | 1.411 s | -1.0% | -0.1% |

The first row against boa 0.22 (2.96 s, measured in the #1028 comment): 1.24x slower before, 1.07x after. `cachegrind` of the
300k-iteration loop (I1 64 KB 4-way): Ir 1.743G -> 1.518G (-12.9%), I1 misses 5.30M -> 1.23M (-77%).

Control set (JetStream drivers and micros; whole-process Ir from cachegrind, wall from 11 pinned interleaved runs):

| benchmark | Ir delta | wall delta | null control |
|---|---:|---:|---:|
| fib | -0.001% | +0.9% (25 runs) | -0.1% |
| richards | -0.001% | -2.9% | -0.7% |
| delta-blue | +0.004% | -0.0% | +0.2% |
| Box2D | +0.41% | -1.8% | -0.6% |
| lazy-collections | -0.20% | +0.1% | +0.1% |
| bigint-noble-secp256k1 | +0.30% | -0.9% | -1.1% |
| opmix | -0.000% | +0.6% | -0.1% |
| gaussian-blur | -0.002% | -2.8% | +0.3% |
| splay | +0.005% | -2.1% | -0.5% |

Two of the Ir rows move by more than 0.3%, and both are run-to-run noise: the identifier hasher is randomised, so the same binary reads
5,309,706,394 and 5,328,439,109 on Box2D and 4,564,528,627 to 4,586,367,717 on secp256k1. Peak RSS is unchanged on every row
(`cc_*` loops 16-26 MB, `splay` 582 MB); `makeAdder` is bimodal at about 23.5 and 25.9 MB for base and new alike (GC timing).

## Code layout is a lottery here

The same source change built five ways gave the same Ir and the same or better simulated I1 misses and mispredictions, and very
different wall time on code the change does not touch:

| variant (n/m = not measured) | lazy-collections | array literal | call only | opmix |
|---|---:|---:|---:|---:|
| install fix only | +1.4% | +0.9% | +7.5% | n/m |
| + inlined property pushes | +4.7% | -1.8% | +0.8% | n/m |
| + pushes `#[inline(never)]` | +0.5% | **+5.9%** | -0.2..+1.9% | -0.4% |
| + only the getter push out of line | +1.7% | -0.3% | +0.8% | **+1.9%** |
| + one out-of-line `push_function_props` (shipped) | +0.1% | -1.0% | +0.2% | +0.6% |

`run_chunk_inner` has the same size in all five builds and starts at a different address in each; cachegrind sees none of this.
The shipped variant is the one that held every control and every closure and array row inside the noise; a later unrelated change can
move any of them by several percent in either direction.

## Not done

- Moving `JsObjectData` (312 bytes) into the arena without the by-value `memcpy` in `alloc_object` (`memcpy` is 61M Ir of 1.5G on this loop):
  #1138 measured the same idea as an Ir win with no wall gain and worse I1 misses.
- Anything in `run_chunk_inner`: it is still 24% of the remaining I1 misses, but its arms are layout-sensitive and this change needed
  no edit there.
