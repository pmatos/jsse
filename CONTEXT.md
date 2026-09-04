# JSSE Engine Context

Domain language for the JSSE JavaScript engine: the modules, concepts, and seams that shape the interpreter and its interfaces.

## Language

**Body**:
A unit of executable ECMAScript syntax — a script, module, or function body — that owns its own IC site map and is the granularity at which inline-cache state is stored.
_Avoid_: function body, script body, code unit.

**IC Site**:
A specific call or property-access location in a Body that can be inline-cached at runtime.
_Avoid_: cache entry, IC slot (when referring to the location rather than the stored value).

**IC State**:
Where a property-access IC Site sits on the `Empty → Mono → Poly → Megamorphic` lattice. `Mono` caches one object; `Poly` caches up to `MAX_POLY_PROP` distinct objects (issue #71); `Megamorphic` is the terminal give-up state. Driven by `PropIcSlot::advance`.
_Avoid_: cache mode, IC level.

**CallSiteId**:
A dense identifier assigned to a call IC site within a single Body.
_Avoid_: call IC index, call cache id.

**PropSiteId**:
A dense identifier assigned to a property-access IC site within a single Body.
_Avoid_: prop IC index, property cache id.

**BodyIcInfo**:
Metadata describing the number and kinds of IC sites in a Body, used to size the runtime IC store without coupling the AST to the runtime slot types.
_Avoid_: cache header, IC metadata.

**BodyIcStore**:
The runtime cache of IC slot values for a Body, keyed by the Body's identity and shared by all closures of that Body.
_Avoid_: cache table, IC map.

**Module Key**:
The canonical host identity of a resolved ECMAScript module, whether it is backed by a file or supplied directly by the host.
_Avoid_: module path, registry path

**Seam**:
A place where one module's interface ends and another's begins. In JSSE, the seams between the AST, the inline-cache system, and the interpreter are intentionally narrow: the AST carries site identifiers, the runtime carries slot values, and the interpreter maps one to the other.
_Avoid_: boundary, layer.

**Divergence Tier**:
How the `differential` fuzz target (`fuzz/fuzz_targets/differential.rs`) classifies a jsse-vs-node run. Tier 1: jsse crashed (signal or the interpreter-panic exit code) while node didn't — an engine bug by definition. Tier 2: exactly one side rejects the source as a syntax error — a real coverage gap. Tier 3: both sides threw (possibly a different error class) or both timed out — expected noise (usually an unimplemented feature), recorded but not a fuzzer finding. See `docs/adr/0004-fuzz-lib-target-and-subprocess-differential.md`.
_Avoid_: divergence class, mismatch level.

## Memory

**Temp-Root Frame**:
A saved depth marker into the interpreter's `gc_temp_roots` stack — the set of `JsValue`s pinned as GC roots only for the duration of one native operation, so a GC safepoint reached while they exist solely as Rust locals cannot collect them. `gc_root_frame` captures the current depth; `gc_unroot_frame` bulk-truncates back to it. A native that roots temporaries opens a frame, roots values into it, and truncates on exit.
_Avoid_: root scope marker, gc stack pointer.

**GC Root Scope**:
The lexical scoping of a Temp-Root Frame behind the `with_gc_root_scope(|interp| …)` combinator: it captures the frame, runs the body, and truncates on every exit path (tail, early `return`, `?`) so the teardown cannot be forgotten on a branch. Prefer it to a hand-paired `gc_root_frame`/`gc_unroot_frame` for a whole-body, single-frame native; reach for the raw primitive only when frames nest or interleave across early exits.
_Avoid_: root guard, unroot epilogue.
