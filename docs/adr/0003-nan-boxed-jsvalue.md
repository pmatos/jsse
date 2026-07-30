# NaN-boxed JsValue is Clone with hand-written Drop, never Copy

`JsValue` (`src/types.rs`) is an 8-variant enum — `Undefined`, `Null`,
`Boolean(bool)`, `Number(f64)`, `String(JsString)`, `Symbol(JsSymbol)`,
`BigInt(JsBigInt)`, `Object(JsObject)` — sized by its largest inline payload,
`JsBigInt { value: num_bigint::BigInt }`, giving every instance roughly 32
bytes regardless of which variant is live. Every clone and drop runs the same
enum-dispatch and drop glue even for the common `Number`/`Boolean`/`Object`
cases that own nothing. Issue #69 proposes NaN-boxing this into a single
64-bit word.

The issue's own original framing — "Numbers, booleans, nullish values, and
object IDs become one-word values" — reads as implying the whole type becomes
`Copy`. That is unsound in Rust: `Copy` and `Drop` are mutually exclusive on a
single type, and `String`/`Symbol`/`BigInt` carry refcounted heap payloads
(`JsString`'s `Arc<Vec<u16>>`, `JsSymbol`'s `Arc`-backed description, a future
`Arc<num_bigint::BigInt>` for `BigInt`) that need `Drop` to release. Forcing
`Copy` anyway would turn every existing `let y = x;` — already pervasive for
`JsValue` throughout the interpreter — into a silent bitwise duplication that
never bumps a refcount. Any pointer-tagged value would then either be
use-after-freed (one bitwise copy dropped while another remains live) or
leaked (the shared payload's refcount never reaches zero), with no compiler
diagnostic pointing at the mistake.

We ratify `NanBoxedValue`, the type that replaces today's `JsValue` enum, as
`Clone` with a hand-written `Drop`, never `Copy`. Both impls branch on the
3-bit tag first: `Number`, `Boolean`, `Undefined`, `Null`, and `Object` (an
inline object id) are a tag check and return — no refcount touched — while
`String`, `Symbol`, and `BigInt` retain (`Clone`) or release (`Drop`) the
`Arc` behind their pointer payload. This preserves today's ownership model
exactly — the same reference-counting discipline, the same call sites that
already call `.clone()` — while shrinking storage from ~32 bytes to 8
wherever a value is held (property descriptors, environment bindings,
`Vec<JsValue>` stacks and argument lists, Map/Set slots), and making the
common Number/Boolean/nullish/Object paths skip BigInt/String drop glue
entirely instead of paying for it on every drop regardless of which variant
is live.

We considered moving `String`/`Symbol`/`BigInt` into the interpreter's
tracing GC so every `JsValue` could be truly `Copy` (the payload becomes a
GC-managed id rather than an owned pointer). We rejected this: today's
`gc_temp_roots` (`interpreter/mod.rs:115`) is a small, manually-curated
allowlist, not an automatic rooting scheme — 14 production `.push` sites
spread across `mod.rs`, `eval.rs` (2), `exec.rs`, `builtins/array.rs`,
`builtins/regexp.rs`, and `builtins/iterators.rs` (8); `gc.rs` only pushes to
it from test code, not from any production rooting path. This project has
already found a broader version of this trade-off costly: an earlier attempt
to root automatically at every call-function return, rather than the current
curated set of loop backedges and explicit rooting sites, regressed hundreds
of test262 cases because callers routinely hold unrooted temporaries across a
call. Folding `String` — touched by nearly every property access, comparison,
and template literal — into the same tracing scheme as `Object` would put
exactly that class of temporary, now for the most pervasively used value type
in the interpreter, at risk on every one of those operations. Keeping
String/Symbol/BigInt on `Arc` and out of the tracing GC avoids reopening that
bug class.

A 2026-07-20 comment on #69 claimed a design doc already existed at
`docs/specs/2026-07-20-nan-boxed-js-value-design.md` (commit `49d36fc`) and
that no further design approval was needed. Neither the file nor the commit
exist anywhere in this repository, and the comment's own label history
(`sym:claimed` → `sym:running` → `sym:stale`) shows the run that posted it
died mid-flight. This ADR and its companion design doc,
`docs/specs/2026-07-26-nan-boxed-js-value-design.md`, are the real design
artifacts; the phantom comment carries no design authority and is superseded.

## Consequences

- `JsValue` shrinks from ~32 bytes (tag plus largest inline variant,
  `JsBigInt { num_bigint::BigInt }`) to 8 bytes everywhere it is stored. That
  ~32-byte figure was accurate when this ADR was written, before #403/#404
  wrapped `JsBigInt`/`JsSymbol` in `Arc`; by the time the swap (#414) actually
  landed, the pre-swap enum measured 16 bytes — see the design doc's
  [As Landed](../specs/2026-07-26-nan-boxed-js-value-design.md#as-landed-phase-4-issue-415)
  section for the reconciled numbers.
- `NanBoxedValue` is `Clone`, not `Copy`: every existing `.clone()` call site
  keeps working unchanged, but a stray `let y = x;` that implicitly relied on
  `Copy` now fails to compile instead of silently corrupting a refcount —
  this is the point of the design, not a gap in it.
- `String`, `Symbol`, and `BigInt` stay owned via `Arc`-backed heap payloads
  outside the tracing GC; `gc_temp_roots` keeps its current scope (object ids
  only) and does not grow to cover them.
- Hand-written `Clone`/`Drop` must branch on the tag before touching any
  refcount; the `Number`/`Boolean`/`Undefined`/`Null`/`Object` paths must be
  true no-op branches that never fall through to an `Arc` increment or
  decrement.
- Downstream phases (issues #403–#415) build call-site conversions and the
  representation swap on top of this ratified ownership model; none of them
  may reintroduce a `Copy` bound on the value type.
