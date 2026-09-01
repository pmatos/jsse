# Architecture review — jsse — 2026-09-02

**Scope**: `src/interpreter/builtins/` hot spots weighted by change frequency over the last 200 commits, plus the standing `.architecture/backlog.md` from the 2026-09-01 firing. `typedarray.rs` and `iterators.rs` were both touched on 2026-09-01 (hottest in the tree). One fresh sub-agent exploration pass over the built-in prototypes and shared prologues surfaced new candidates; the prior run's backlog was reconciled against `gh` (PR #543 `validate-typed-array` **merged** → `landed`).
**Picked**: `arraybuffer-receiver-guard` — see the PR and `.architecture/backlog.md`
**Branch**: `sym/jsse/routine/refactor-audit/01M1FK9R05` — **adopted** (all four conditions held: non-default, 0 commits ahead of `origin/main`, no upstream, unpublished on origin). Never renamed; the slug is recorded here and in the backlog instead.
**Degradations**: none — `gh` authenticated, sub-agents available, `codebase-design` vocabulary applied.

In the Mermaid diagrams: **solid edges are the interface** (what a caller wires), **dashed edges are inside the implementation** (hidden behind a seam).

## Candidates

### arraybuffer-receiver-guard — snapshot receiver-validation guards for the ArrayBuffer-family getters · Strong · score 24/25

- **Files** — `src/interpreter/builtins/typedarray.rs`. Seam belongs beside the proven sibling `validate_typed_array` (`typedarray.rs:5721`, landed as PR #543). Collapsible ArrayBuffer getters, each with its own inline `enum Probe`, at `typedarray.rs:42, 80, 116, 150, 184` (`byteLength`, `detached`, `resizable`, `immutable`, `maxByteLength`); SharedArrayBuffer getters at `typedarray.rs:1092, 1107, 1125` (`byteLength`, `maxByteLength`, `growable`). File-count estimate: **1 file** (plus an in-crate `#[cfg(test)]` module).
- **Score** — **24/25**
  - *Leverage 5* — 8 getters each shed a hand-rolled `as_object_id → get_object_cell → borrow → arraybuffer_data → is_shared` prologue; the 5 ArrayBuffer getters additionally delete a per-getter local `enum Probe` declaration that exists **only** to escape the `borrow()` before calling `create_type_error`. The brand+snapshot decision becomes independently unit-testable for the first time (today only test262 covers it). *(Sensitivity: scored conservatively as a two-seam bundle, leverage is 4 → total 22/25, which ties `complete-state-machine-generator-ctor`; the tie breaks on heat — `typedarray.rs` touched 2026-09-01 vs `generator_runtime.rs` 2026-08-25 — so the pick is deterministic either way. Recorded per the ranking rubric.)*
  - *Locality 4* — changing what "a valid, non-shared ArrayBuffer receiver" means becomes a one-function edit; today it is 5 divergent inline copies (they already differ: `maxByteLength` folds the detached branch into its read, others don't).
  - *Blast radius 1* (→ contributes 5) — one file, all sites are module-private native getter closures, no exported/public interface crossed.
  - *Heat 5* — `typedarray.rs` is among the hottest files in the tree, last changed 2026-09-01 (the day before this run).
- **Problem** — "Is `this` an ArrayBuffer (not shared, not detached), and what are its bytes?" is one conceptual step, but each getter re-expresses it as a ~20-line borrow-juggling probe. The five ArrayBuffer getters each declare a *local* `enum Probe { NotAB, Shared, Detached, … }` whose sole purpose is to carry the brand verdict out of the `cell.borrow()` closure so `create_type_error(interp)` can run after the borrow drops — the interface (read one field) is far simpler than the implementation the caller is forced to hand-roll, the definition of a shallow module. The copies have already drifted.
- **Deletion test** — **Concentrates.** One `require_array_buffer(interp, this) -> Result<ArrayBufferSnapshot, Completion>` absorbs the brand check (ArrayBuffer data present **and** not shared), the borrow scoping, and the field reads into an owned snapshot. Deleting it re-scatters that invariant and the five `enum Probe` decls across the getters; the callers do not grow — each shrinks to a field read on the snapshot.
- **Solution** — Add a snapshot struct and a receiver guard per brand (`require_array_buffer` returning `{ byte_length, is_detached, max_byte_length, is_immutable }`; `require_shared_array_buffer` returning `{ byte_length, max_byte_length }`). The guard **never throws on detached** — it returns `is_detached` in the snapshot, because `byteLength`/`maxByteLength` return `0` (not a throw) on a detached buffer and `detached` returns the flag itself. Migrate the 5 ArrayBuffer + 3 SharedArrayBuffer getters. The brand checks stay bidirectional (`require_array_buffer` rejects a SharedArrayBuffer; `require_shared_array_buffer` rejects a plain ArrayBuffer).
- **Benefits** — *Leverage*: 8 getters lose their prologue; a future spec change to the ArrayBuffer brand/detach rules is a one-line edit. *Locality*: the brand/detach decision lives in one place next to `validate_typed_array`. *Test surface*: the validation is exercisable directly through a narrow `Result` interface — valid AB → `Ok(snapshot)`, non-object / non-AB / shared → the exact `Err` — instead of only observably through 8 separate getters.
- **Deferred (recorded as a follow-up, not lost)** — DataView getters (`buffer`, `byteOffset`, `byteLength` at `:4801, 4817, 4846`) *throw* on IsViewOutOfBounds (which subsumes detached), compute a per-getter OOB condition rather than reading a field, and reach through to the **underlying buffer's** length — a cross-object read, not a single borrow. The ArrayBuffer/SharedArrayBuffer *methods* (`slice`, `resize`, `transfer`, `grow`) hold their object borrow across the body and re-probe detached **after** the species constructor runs user code. Both are left for a follow-up firing, exactly as PR #543 left `slice`/`sort`/`toSorted`. Tracked as `dataview-receiver-guard` in the backlog.

**Before** — every getter wires the prologue itself:

```mermaid
graph LR
  G1[byteLength] --> P1[as_object_id + borrow]
  G1 --> P2[arraybuffer_data + is_shared]
  G1 --> P3[local enum Probe escape]
  G1 --> P4[throw not-an-ArrayBuffer]
  G2[detached] --> P1
  G2 --> P2
  G2 --> P3
  G2 --> P4
  G3[...6 more] --> P1
```

**After** — one guard per brand hides the prologue:

```mermaid
graph LR
  G1[byteLength] --> R[require_array_buffer]
  G2[detached] --> R
  G3[resizable/immutable/maxByteLength] --> R
  S1[SAB byteLength] --> RS[require_shared_array_buffer]
  S2[SAB maxByteLength/growable] --> RS
  R -.-> D1[as_object_id + borrow]
  R -.-> D2[brand check: AB and not shared]
  R -.-> D3[snapshot: bytes, detached, max, immutable]
  R -.-> D4[throw not-an-ArrayBuffer]
```

### complete-state-machine-generator-ctor — collapse ~87 inlined "completed generator" struct literals · Strong · score 22/25

- **Files** — `src/interpreter/eval/generator_runtime.rs`; enum at `src/interpreter/types.rs`. Estimate: 1–2 files.
- **Score** — **22/25** — *Leverage 5* (87 byte-identical 10-field literals; a constructor adds a compiler check for the "cleared" invariant), *Locality 4*, *Blast radius 1* (→5), *Heat 3* (`generator_runtime.rs` last touched 2026-08-25; YAGNI docks cold code). Carried forward from the 2026-09-01 backlog; friction re-verified present.
- **Problem** — "This generator is finished; clear every pending field" is copy-pasted 87 times (27 sync + 60 async). Adding a field is an 87-site edit with no compiler check for a missed field.
- **Deletion test** — **Concentrates** into `completed_state_machine_generator` / `…_async_generator`.
- **Solution** — Two private constructors returning the cleared `IteratorState`.
- **Benefits** — *Leverage* across 87 sites; *Locality* on the completed-generator shape; *Test surface*: the cleared state becomes directly assertable.
- **Recommendation strength** — Strong. This is the runner-up **candidate** and the natural next firing.

```mermaid
graph LR
  S1[gen.return] --> L1[inline 10-field literal]
  S2[gen exhausted] --> L2[inline 10-field literal]
  S3[...85 more] --> L3[inline 10-field literal]
```

```mermaid
graph LR
  S1[gen.return] --> C[completed_state_machine_generator]
  S2[gen exhausted] --> C
  S3[...85 more] --> C
  C -.-> F[cleared 10-field IteratorState]
```

### completion-into-result — a `Completion::into_result()` adapter for the Result-returning iterator helpers · Worth exploring · score 21/25

- **Files** — `src/interpreter/builtins/iterators.rs` (37 `Completion::Normal(v) => v` adapter heads, 27 `Completion::Throw(e) => return Err(e)`), `src/interpreter/types.rs` (`impl Completion`). Estimate: 2 files.
- **Score** — **21/25** — *Leverage 4* (dozens of 4-line match wrappers collapse to `.into_result()?`, and the fabricated `_ =>` error arms become removable dead code), *Locality 3*, *Blast radius 1* (→5), *Heat 5* (`iterators.rs` touched 2026-09-01).
- **Problem** — The iterator abstract-operation helpers return `Result<_, JsValue>` but call MOP methods that return `Completion`, so every call is wrapped in a hand-rolled `match Completion { Normal(v)=>v, Throw(e)=>return Err(e), _=>… }`.
- **Deletion test** — **Concentrates** into one method on the existing `impl Completion`.
- **Solution** — `Completion::into_result(self) -> Result<JsValue, JsValue>`, mapping `Normal→Ok`, `Throw→Err`, other variants→`Ok(undefined)`.
- **Recommendation strength** — Worth exploring. Deferred: the two-seam ArrayBuffer guard scores higher on locality and is a cleaner single-family deepening.

```mermaid
graph LR
  H1[IteratorNext] --> W1[match Completion 4-line]
  H2[IteratorStep] --> W2[match Completion 4-line]
  H3[...many] --> W3[match Completion 4-line]
```

```mermaid
graph LR
  H1[IteratorNext] --> I[into_result]
  H2[IteratorStep] --> I
  H3[...many] --> I
  I -.-> M1[Normal to Ok]
  I -.-> M2[Throw to Err]
  I -.-> M3[other to Ok undefined]
```

### completion-unwrap-macro — a `try_completion!` macro for the Completion-returning natives · Worth exploring · score 21/25

- **Files** — `src/interpreter/types.rs` (macro) + `src/interpreter/builtins/typedarray.rs` as first adopter (27 sites). Estimate: 2 files for the contained first step.
- **Score** — **21/25** — *Leverage 4*, *Locality 3*, *Blast radius 1* (→5), *Heat 5*.
- **Problem** — The `match Completion { Normal(v)=>v, Throw(e)=>return Completion::Throw(e), _=>… }` idiom recurs in Completion-returning functions (typedarray 27, builtins/mod 22, eval 11, string 11, …).
- **Deletion test** — **Concentrates** into one macro; distinct from `completion-into-result` (that serves `Result`-returning helpers, this serves `Completion`-returning ones).
- **Recommendation strength** — Worth exploring. A macro adopted across many files has a larger eventual blast radius; scoped to one adopter it is a clean start, but it lost to the ArrayBuffer guard on locality.

```mermaid
graph LR
  F1[native A] --> M1[match Completion 4-line]
  F2[native B] --> M2[match Completion 4-line]
```

```mermaid
graph LR
  F1[native A] --> T[try_completion!]
  F2[native B] --> T
  T -.-> U1[bind Normal value]
  T -.-> U2[propagate abrupt Completion]
```

### object-this-coercion — a `require_this_object` ToObject prologue for Object.prototype · Worth exploring · score 20/25

- **Files** — `src/interpreter/builtins/mod.rs` (~10 `to_object(this_val)` prologues, ~4221–4770). Estimate: 1 file.
- **Score** — **20/25** — *Leverage 4*, *Locality 3*, *Blast radius 1* (→5), *Heat 4* (`builtins/mod.rs` last touched 2026-08-25).
- **Problem** — `hasOwnProperty`, `toString`, `isPrototypeOf`, `propertyIsEnumerable`, `toLocaleString`, `__proto__` each open-code `match to_object(this_val) { Normal(v)=>v, other=>return other }` then unwrap the object id. Distinct from the `object-id-of` round-trip: this is a *coercion* prologue (ToObject can run user code / throw).
- **Deletion test** — **Concentrates** into `require_this_object(this) -> Result<u64, Completion>`.
- **Recommendation strength** — Worth exploring.

```mermaid
graph LR
  O1[hasOwnProperty] --> C1[match to_object + unwrap id]
  O2[toString] --> C2[match to_object + unwrap id]
  O3[...8 more] --> C3[match to_object + unwrap id]
```

```mermaid
graph LR
  O1[hasOwnProperty] --> R[require_this_object]
  O2[toString] --> R
  O3[...8 more] --> R
  R -.-> D1[ToObject may run user code]
  R -.-> D2[unwrap object id]
```

### regexp-last-index-accessor — `get_last_index` / `set_last_index` for the RegExp lastIndex dance · Speculative · score 18/25

- **Files** — `src/interpreter/builtins/regexp.rs` (5 read+ToLength sites, 3 `spec_set(...,"lastIndex",...)` sites). Estimate: 1 file.
- **Score** — **18/25** — *Leverage 3* (8 sites, small dance), *Locality 3*, *Blast radius 1* (→5), *Heat 4* (`regexp.rs` last touched 2026-08-26).
- **Problem** — `Get(R,"lastIndex")`→`ToLength` / `Set(R,"lastIndex",v,true)` is re-spelled inline at each site.
- **Deletion test** — **Concentrates** into two small accessors.
- **Recommendation strength** — Speculative — small dance, lower payback than the getter-family guard.

```mermaid
graph LR
  R1[exec] --> A1[Get lastIndex + ToLength]
  R2[Symbol.match] --> A2[Get lastIndex + ToLength]
  R3[...] --> A3[Set lastIndex]
```

```mermaid
graph LR
  R1[exec] --> G[get_last_index / set_last_index]
  R2[Symbol.match] --> G
  R3[...] --> G
  G -.-> D1[Get + ToLength]
  G -.-> D2[Set true]
```

## Dropped

| Candidate | Dropped because |
|---|---|
| `typedarray-shared-equality` | Not a deepening — `/simplify`-class. `typedarray.rs` re-implements private `same_value_zero`/`strict_eq` that already exist in `helpers.rs`; deduping *moves* code rather than concentrating behaviour behind a new seam (leverage 2). Flagged caveat: the private `strict_eq` compares strings via `to_rust_string()` (allocating) — a semantic divergence must be confirmed before merging, and if real is a bug report, not a dedup. Recorded so a future run does not re-derive it as a deep-module candidate. |
| `object-id-of` | Leverage 2 — a `/simplify`-class round-trip cleanup, not a deepening (carried from the 2026-09-01 backlog). |

## Too large to automate

| Candidate | Blast radius |
|---|---|
| `unify-generator-async-drivers` — `generator_next_state_machine_impl` (~1580 lines) and `async_generator_next_state_machine_impl` (~3050 lines) are largely parallel state-machine interpreters. Unifying them is a deep structural refactor for a human to schedule; landing the generator-constructor and settle-tail candidates first shrinks both drivers. | 5 — human-scheduled |

## Pick

**`arraybuffer-receiver-guard` (24/25).** It outranks the runner-up **candidate** `complete-state-machine-generator-ctor` (22/25) on **heat**: both are single-file, module-private, blast-radius-1 collapses of a duplicated receiver-validation invariant, but `typedarray.rs` was touched the day before this run while `generator_runtime.rs` is over a week cold, and YAGNI weights deepening toward code that keeps changing. A proven deep sibling — `validate_typed_array`, landed the day before as PR #543 — already exists in the same file, so the interface shape is de-risked, and the drift already visible across the five `enum Probe` copies is evidence the friction is active, not hypothetical. The 2-point gap is **not** within 1 point on the leverage-5 read; on the conservative leverage-4 read the two tie at 22 and heat breaks it the same direction, so the pick is deterministic.

## Design

_Written in step 4 (design-it-twice + adjudication); appended after this report was first committed._
