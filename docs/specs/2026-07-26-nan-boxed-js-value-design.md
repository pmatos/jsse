# NaN-Boxed JsValue

## Problem

`JsValue` (`src/types.rs`) is today an 8-variant Rust enum — `Undefined`,
`Null`, `Boolean(bool)`, `Number(f64)`, `String(JsString)`, `Symbol(JsSymbol)`,
`BigInt(JsBigInt)`, `Object(JsObject)` — sized by its largest inline payload,
`JsBigInt { value: num_bigint::BigInt }`, giving every instance roughly 32
bytes regardless of which variant is live. Every storage site — property
descriptors, environment bindings, `Vec<JsValue>` argument lists and stacks,
Map/Set slots — pays that full cost even for `Number`/`Boolean`/`Undefined`/
`Null`, which need only a tag and (for `Number`/`Boolean`) a scalar. Every
clone and drop runs the same enum-dispatch and drop glue regardless of which
variant is live. Issue #69 proposes shrinking this to a single 8-byte machine
word.

The fix must: preserve exact IEEE 754 double semantics for every finite value,
including signed zero; respect ECMAScript's NaN canonicalization (§6.1.6.1),
under which every JS-visible NaN collapses to one representative bit pattern;
keep the ownership model ratified in
[ADR 0003](../adr/0003-nan-boxed-jsvalue.md) — `Clone` plus hand-written
`Drop`, never `Copy`; keep the value type `Send` across the engine's four
`std::thread::spawn` sites; and never let any NaN bit pattern — whether from
ordinary hardware arithmetic (`sqrt`, division) or from JS-controlled bytes
(`DataView`/`TypedArray` reads) — alias a pointer or tag encoding.

## Approaches Considered

1. NaN-box using the negative (sign-bit-set) quiet-NaN subspace: a 3-bit tag
   plus 48-bit payload, `Arc`-owned pointers for heap variants. **Selected** —
   matches the ownership model ADR 0003 already ratified and needs no change
   to the GC's rooting scope.
2. Move `String`/`Symbol`/`BigInt` into the tracing GC so the whole value can
   be `Copy`. Rejected in ADR 0003: besides the `Copy`/`Drop` unsoundness,
   folding `String` — touched by nearly every property access, comparison,
   and template literal — into `gc_temp_roots`'s small, manually-curated
   allowlist reopens the unrooted-temporary bug class on the interpreter's
   most pervasively touched type.
3. A per-interpreter handle table: `JsValue` holds a small integer handle
   into a side table that owns the real payload, uniformly for every
   variant. Rejected: this adds an indirection and a bounds-checked lookup to
   `Number`/`Boolean` access — by far the most common variants — to buy
   representation uniformity that tag-check-and-return already gets for
   free.

## Design

### Tag layout

An IEEE 754 double has a 1-bit sign, an 11-bit exponent, and a 52-bit
mantissa. A bit pattern is NaN exactly when the exponent field is all-1 and
the mantissa is nonzero, and additionally "quiet" when the mantissa's top bit
is set. We reserve the *negative* quiet-NaN subspace — sign bit 1, exponent
all-1, quiet bit 1 — as the NaN-boxing signature.

Unlike some NaN-boxing designs, we cannot rely on the hardware to never
produce this bit pattern on its own: confirmed on this project's target
(x86-64), `(-1.0_f64).sqrt().to_bits()` is `0xfff8000000000000` — sign bit
set, exponent all-1, quiet bit set — which is exactly the reserved signature
with tag 0 and payload 0, i.e. it would decode as `Undefined` if stored
unmodified. `Math.sqrt(-1)` is an ordinary, frequent operation, not an edge
case, so this is not a theoretical risk. The safety argument therefore cannot
be "no legitimate double reaches this bit pattern"; it has to be "every f64
is canonicalized before it can reach a `NanBoxedValue` slot," which is a
property of the boxing constructor, not of hardware NaN generation. See
Canonicalization below.

The reserved signature is the full 13-bit prefix — sign bit 1, exponent
all-1, quiet bit 1 — checked as one mask-and-compare:
`(bits & 0xFFF8_0000_0000_0000) == 0xFFF8_0000_0000_0000`. All three
conditions matter: dropping the quiet-bit check would misclassify
`-Infinity` (`0xFFF0000000000000`: sign 1, exponent all-1, quiet bit **0**)
as boxed, decoding it as tag 0 / payload 0, i.e. `Undefined`, since it
differs from the reserved prefix only in that one bit. The quiet bit is
therefore load-bearing for *decoding*, not decorative — it is what keeps
`-Infinity` on the passthrough path. (A negative *signaling* NaN, quiet bit
0 and a nonzero mantissa, is excluded by the same 13-bit test and is not a
concern here regardless, since `JsValue::number`'s canonicalization already
maps every NaN — quiet or signaling, either sign — to the single canonical
bit pattern before boxing; the 13-bit test matters for decode, canonicalization
is a separate, unconditional step covered below.)

Below the 13-bit prefix, the remaining 51 bits split into a 3-bit tag at
bits 48–50 and a 48-bit payload at bits 0–47, so the payload occupies the
low bits untouched by the tag and pointer packing needs no shift, only a
mask:

| Tag (3 bits, bits 48–50) | Variant | Payload (bits 0–47) |
|---|---|---|
| 0 | `Undefined` | unused |
| 1 | `Null` | unused |
| 2 | `Boolean(false)` | unused |
| 3 | `Boolean(true)` | unused |
| 4 | `Object` | 48-bit object id |
| 5 | `String` | 48-bit `Arc<Vec<u16>>` pointer |
| 6 | `Symbol` | 48-bit `Arc` symbol-data pointer |
| 7 | `BigInt` | 48-bit `Arc<num_bigint::BigInt>` pointer |

Any bit pattern that fails the 13-bit prefix test passes through unmodified
as `Number(f64)` — a single mask-and-compare, so the common numeric path
costs at most one comparison more than a raw `f64` today.

48 bits is not an arbitrary budget: both x86-64 and AArch64 userspace virtual
addresses are canonical-form-limited to 48 bits today, so `Arc::into_raw`
pointers fit without truncation, and the object arena's id space (below)
fits comfortably inside it too.

### Object-id packing

`JsObject { id: u64 }` (`src/types.rs:365`) packs into the 48-bit payload
without any change to the arena. `ObjectArena::alloc`/`free`
(`src/interpreter/object_arena.rs:308`, `:349`) hand out ids from a monotonic
`next_slot` counter that advances only when `free_list` is empty — every
freed id is pushed back onto `free_list` and reused before `next_slot`
advances again. The id space therefore tracks the live-object high-water
mark, not cumulative allocations over a program's lifetime, so 48 bits is not
a realistic exhaustion risk.

### String/Symbol/BigInt packing

Heap-payload tags store `Arc::into_raw(payload) as u64` truncated to 48 bits.
On today's x86-64 and AArch64 userspace address spaces, a canonical pointer's
top 16 bits (48–63) are always zero, so truncating to 48 bits loses no
information and reconstruction is a zero-extend, not a sign-extend. This is
an explicit scope limit, not a universal guarantee: 5-level paging (57-bit
virtual addresses) and ARMv8.2 LVA (52-bit virtual addresses) can hand out
pointers outside 48 bits. Neither is default on this project's supported
targets today, so it is out of scope for this design, but the boxing
constructor should `debug_assert!` that a pointer's top 16 bits are zero
before packing it, so a future target that violates the assumption fails
loudly in development rather than silently truncating a pointer in release.
`Clone` and `Drop` branch on the tag first:

- `Number`/`Boolean`/`Undefined`/`Null`: no-op — the bit pattern is copied or
  discarded outright. This is the load-bearing invariant from ADR 0003: these
  paths must never fall through to an `Arc` increment or decrement.
- `Object`: no-op on the id itself; object liveness stays governed by the
  tracing GC exactly as it is today, untouched by this change.
- `String`/`Symbol`/`BigInt`: reconstruct the `Arc` via `Arc::from_raw` on the
  zero-extended pointer, then either bump the strong count and `mem::forget`
  the temporary (`Clone`) or let it drop normally (`Drop`).

### Prerequisite structural changes

Two existing types need a wrapper before they can be pointer-packed:

- `JsSymbol` (`src/types.rs:336-339`) currently inlines
  `description: Option<JsString>` with no `Arc`/`Rc` wrapper around the
  symbol itself. Issue #404 decides between an `Arc`-wrapped symbol-data
  struct (`id` plus `description`) and a side table keyed by `id` alone (the
  id already fits in 48 bits unwrapped, so a side table is viable if lookup
  indirection is cheaper than the extra allocation).
- `JsBigInt` (`src/types.rs:359-361`) owns `num_bigint::BigInt` by value with
  no wrapper. Issue #403 wraps it as `Arc<num_bigint::BigInt>`; an audit of
  `src/interpreter/builtins/bigint.rs` found no in-place mutation of a
  `BigInt` value, so sharing one allocation across clones is safe.

### Canonicalization at construction

Canonicalization cannot be scoped to a handful of call sites — as the tag
layout section shows, ordinary Rust f64 arithmetic (`sqrt`, division, and
other libm-backed operations) can produce a hardware NaN with the sign bit
set, and there is no practical way to enumerate every arithmetic path that
might do so across the interpreter. The only sound choke point is the single
constructor every `f64` must pass through to become a `JsValue`/
`NanBoxedValue`: `JsValue::number(n: f64)` (`src/types.rs:387`). That
function already carries a doc comment reserving exactly this job — *"NaN
canonicalisation lands here in Phase 3 (issue #69) — for now this is a thin
wrapper"* — anticipating this design. Phase 3 (#414) makes it unconditional:
`if n.is_nan() { CANONICAL_NAN } else { n }`, where `CANONICAL_NAN` is the
single positive-sign bit pattern (`f64::NAN`'s own bits, `0x7ff8…0`, which is
outside the reserved signature).

For this to be a real guarantee and not just a convention, `JsValue::number`
must become the *only* path that produces a `Number` value — today, dozens of
sites (67 in `eval.rs` alone) construct `JsValue::Number(x)` directly via the
enum's tuple constructor, bypassing `number()` entirely. Phase 2's call-site
conversion (issues #407–#413), which already replaces direct enum pattern
matches with the accessor API from PR #154, must also replace every direct
`JsValue::Number(...)` construction with `JsValue::number(...)` so that by
the time Phase 3 lands, the canonicalizing constructor has no bypass to
inherit.

Within that universal guarantee, three sites deserve explicit call-out
because they are the ones reachable from **fully** JS-controlled bit
patterns — not just "whatever a libm function happens to return," but any
64-bit pattern a script can construct byte-by-byte — so they are the highest-
value targets for the regression tests in #406:

- `typed_array_get_index` (`src/interpreter/types.rs:3439`) and
  `typed_array_get_index_shared` (`src/interpreter/types.rs:3331`);
- the `dv_get_method!` macro's `getFloat32`/`getFloat64` expansions
  (`src/interpreter/builtins/typedarray.rs:5333`, `:5341`) and `getFloat16`
  (`:5375`).

test262's own `built-ins/DataView/prototype/getFloat64/return-nan.js` builds
NaN byte-by-byte this way. Issue #405 audits these three sites (and any
others found reading raw bytes into an `f64`) to confirm they route through
`JsValue::number` rather than constructing the enum variant directly; issue
#406 adds bit-exact round-trip tests proving the constructor always produces
the canonical positive-sign NaN for every input, never the reserved sign=1
signature.

### Signed-zero guarantee

The encoding only claims bit patterns matching the full 13-bit prefix — sign
1, exponent all-1, *and* quiet bit 1. `+0.0` and `-0.0` differ only in the
sign bit with an all-zero exponent, so neither is ever NaN and both always
take the passthrough path unmodified. Every other finite float passes
through bit-exact as well, and so does `+Infinity` (exponent all-1, sign 0)
and `-Infinity` (exponent all-1, sign 1, mantissa 0 — i.e. quiet bit 0):
`-Infinity` is the near-miss that makes checking the quiet bit essential
rather than optional, since it shares the reserved signature's sign and
exponent bits and is distinguished only by that one bit (see Tag layout).

### Threading

`NanBoxedValue` must stay `Send`. Four `std::thread::spawn` sites compile
today only because every `JsValue` variant is `Send` —
`interpreter/mod.rs:619`, `:726`, `builtins/mod.rs:491`,
`builtins/atomics.rs:618` — and all of them rely on `Arc`, not `Rc`, as the
payload wrapper. The packed representation preserves that: non-atomic
refcounting beyond `Arc`'s existing atomic count is unnecessary because the
object arena and every payload `Arc` are only ever dereferenced by the single
owning interpreter thread; a cross-thread move carries only an inert id or
pointer bit pattern, never a live borrow.

### Rollout mechanism

No feature flag. This codebase has no `[features]` table in `Cargo.toml` and
no `#[cfg(feature = ...)]` anywhere in `src/` today, so introducing one would
add infrastructure this project doesn't otherwise use. `bytecode_enabled`
(`interpreter/mod.rs:160`) is not a precedent for this case: it is a runtime
`bool` that toggles a codepath without changing any type's memory layout,
whereas NaN-boxing changes `JsValue`'s layout itself and cannot be
runtime-switched between two representations. Phase 3 (issue #414) lands the
representation swap as a single PR after the exhaustive pre-merge validation
gate below; rollback is a plain `git revert`.

### Testing/validation strategy

See Validation below for the full gate; the bit-exact NaN test plan is
tracked as issue #406 and the Phase 3 pre-merge gate as issue #414.

## Specification Semantics

ECMAScript defines a single Number type backed by IEEE 754-2019 binary64,
with NaN canonicalization specified by §6.1.6.1: any operation that would
produce a NaN produces *the* NaN value, and distinct NaN bit patterns are not
observable to JS code. This licenses the engine to canonicalize freely — it
does not, by itself, guarantee that no operation's underlying hardware
implementation produces a negative-signed NaN (§6.1.6.1 constrains what JS
observes, not what libm or an FPU instruction returns bit-for-bit). The
encoding's soundness therefore rests on the engine enforcing §6.1.6.1 at the
single boxing constructor (`JsValue::number`, see Canonicalization above),
not on an assumption that the reserved bit pattern is otherwise unreachable.
With that constructor enforced, the encoding is a pure engine-internal
representation change: it alters no user-observable Number, Boolean,
Undefined, Null, Object, String, Symbol, or BigInt semantics.

## Validation

- Bit-exact round-trip unit tests (issue #406) against `JsValue::number`
  directly: every hardware-producible NaN bit pattern this project can
  generate (`sqrt` of a negative, `0.0/0.0`, transcendental-function edge
  cases, and the raw-byte constructions below) canonicalizes to the single
  positive-sign bit pattern; `+0.0`/`-0.0` and every other finite double
  survive a box/unbox round trip unchanged; each reserved tag value decodes
  to the correct variant and never aliases a passthrough double.
- A grep-based or Clippy-lint check (added alongside Phase 2, #407–#413) that
  no source file constructs `JsValue::Number(...)` via the enum's tuple
  constructor directly, so `JsValue::number` stays the sole entry point
  canonicalization can rely on.
- test262-extra regression coverage for the DataView/TypedArray
  byte-boundary sites (`getFloat32`, `getFloat64`, `getFloat16`, and the
  shared/non-shared typed-array element getters), confirming JS-constructed
  NaN payloads never leak a non-canonical bit pattern into a `JsValue`.
- Full `cargo test --release` plus the full test262 suite
  (`uv run python scripts/run-test262.py`) held at 100% before and after
  Phase 3 (issue #414) lands, per this project's forward-progress rule.
- `./scripts/lint.sh` (rustfmt + clippy) clean on every phase.
- Each Phase 2 issue (#407–#413) converts one file group onto the `JsValue`
  accessor API from PR #154 (`as_number`/`as_object_id`/`as_string`/
  `as_symbol`/`as_bigint`, `with_string`/`with_symbol`/`with_bigint`,
  `into_*`, `discriminant()`/`kind()`/`is_object()`) so no call site
  pattern-matches the enum directly. Phase 3 (#414) has a hard compile-order
  dependency on #403–#413 completing first: an unconverted file with a direct
  enum pattern fails to compile once the type stops being a plain enum. Phase
  4 (#415) removes any compatibility shims left over from the swap.
