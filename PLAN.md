# Plan: fix(gc) — destructuring rest-with-getter panics with "dead object id" under GC stress (#829)

## 1. Problem restated

`const {...x} = { get v() { return 2; } }` allocates the fresh rest object
(`rest_obj_id`) before copying any properties into it, then calls
`copy_data_properties`, which invokes the source's `v` getter. Nothing on the
GC's root set references `rest_obj_id` during that getter call — it exists
only as a Rust-local `u64` in `bind_object_rest_values`
(`src/interpreter/exec.rs:1377`) — so under `JSSE_GC_STRESS` a collection
triggered inside the getter (any statement-boundary/loop-back-edge safepoint
the getter's body crosses) sweeps the rest object's still-empty arena slot.
Accessing it afterwards via `get_object_cell_expect` hits the dead-id debug
assertion in `object_arena.rs:344` and panics. The hazard is already called
out, unfixed, in `bind_object_rest_values`'s own doc comment (added by #783).

Walking the function that hazard sits in (`copy_data_properties`, in
`src/interpreter/eval/literals.rs:1240`) and its two call-site families
surfaces two further, narrower instances of the same bug class in the same
code path:

- `copy_data_properties`'s own `result: Vec<(JsPropertyKey, JsValue)>`
  accumulator holds each already-fetched property value across every
  subsequent loop iteration, and a later key's getter (or proxy trap) can run
  arbitrary code that collects an earlier key's freshly-created,
  GC-root-less value before it ever reaches the rest object or a
  `JsObjectData`. This window exists for every caller of
  `copy_data_properties`, including the object-literal spread arm
  (`eval_object_literal`, `literals.rs:1388`), not just the rest-binding
  path — fixing it in `copy_data_properties` closes it everywhere at once.
- `destructure_object_assignment`'s rest arm (`src/interpreter/eval.rs:4404`,
  the `({...x} = obj)` assignment form, as opposed to `const`/`let`/`var`
  binding) builds `rest_val` via `bind_object_rest_values` and then passes it
  to `put_value_to_target(inner, rest_val, env)`. When `inner` is a member
  expression (`({...o[f()]} = src)`), `put_value_to_target` →
  `set_member_property` → `self.eval_expr(obj_expr, env)` evaluates the
  target's base/key expressions — arbitrary user code — while `rest_val` is
  once again only a Rust local. Unlike the binding form (where
  `ObjectPatternProperty::Rest`'s sub-pattern is grammar-restricted to a bare
  `BindingIdentifier`, so the recursive `bind_pattern` call that follows is
  just an environment write with no further user code), the assignment
  form's `DestructuringAssignmentTarget` can be an arbitrary
  `LeftHandSideExpression`, so this window is real and independent of the
  first two.

All three share one root cause: an object is built or captured before being
reachable from anything the GC walks, and user code runs before it becomes
reachable. All three are reachable from the `const {...x} = source-with-getter`
shape the issue names (two from the source having a getter, one from the
assignment target itself running code), so all three are fixed in this PR.

## 2. Spec basis

- `sec-destructuring-binding-patterns-runtime-semantics-restbindinginitialization`
  (RestBindingInitialization — `BindingRestProperty : ... BindingIdentifier`):
  step 2 creates `restObj` via `OrdinaryObjectCreate(%Object.prototype%)`
  *before* step 3's `CopyDataProperties(restObj, value, excludedNames)` runs,
  which is exactly the object-identity requirement our engine must preserve
  across any collection that happens mid-copy — `restObj` is one object for
  the whole operation, not a value reconstructed after the fact.
- `sec-copydataproperties` (CopyDataProperties, used by the rest-binding path
  above, by `RestDestructuringAssignmentEvaluation` below, and by
  object-literal spread via `sec-runtime-semantics-propertydefinitionevaluation`):
  step 3 fetches `[[OwnPropertyKeys]]` once, then for each key conditionally
  calls `[[GetOwnProperty]]` and `Get` — both explicitly marked
  `effects="user-code"` in the spec, i.e. each key's processing can run
  arbitrary script (getters, proxy traps) that must not disturb property
  values already produced by an earlier key in the same loop. The abstract
  operation's own note — "The target passed in here is always a newly
  created object which is not directly accessible in case of an error being
  thrown" — states the invariant this bug violates: the in-progress target
  (and, by the same reasoning, each value already read off `source` before
  it is written into that target) must survive to the point an error (or the
  final result) is produced, not disappear into a GC sweep along the way.
- `sec-runtime-semantics-propertydestructuringassignmentevaluation`
  (`AssignmentProperty : ... AssignmentRestProperty`) dispatches to
  `sec-runtime-semantics-restdestructuringassignmentevaluation`
  (`AssignmentRestProperty : ... DestructuringAssignmentTarget`): step 1
  evaluates `lRef` (the `DestructuringAssignmentTarget` reference), step 2
  creates `restObj`, step 3 runs `CopyDataProperties`, step 4 is
  `PutValue(lRef, restObj)`. `PutValue` on a member reference can itself run
  a setter — ordinary, expected user code — but `restObj` must still be the
  same object through that call; it governs the third hazard above (our
  engine's `put_value_to_target` call for the rest arm, unlike this
  `pre-resolved-lRef` shape, re-evaluates the target's base/key expressions
  rather than using an already-resolved reference — an existing,
  pre-existing evaluation-order deviation from this clause that is not part
  of this fix; see §7).
- No syntax or observable-semantics change is planned: all three clauses
  already mandate the identity/ordering guarantees above; the fix makes the
  engine's internal object lifetime honor them instead of changing what the
  spec requires.

## 3. Files to touch

- `src/interpreter/eval/literals.rs` — `copy_data_properties` (~line 1240):
  root each fetched property value for the remaining lifetime of the loop
  that accumulates `result`.
- `src/interpreter/exec.rs` — `bind_object_rest_values` (~line 1377): root
  `rest_obj_id` across the `copy_data_properties` call; drop the doc
  comment's now-stale "share no explicit rooting" sentence.
- `src/interpreter/eval.rs` — `destructure_object_assignment`'s rest arm
  (~line 4404): root `rest_val` across the `put_value_to_target` call.
- `test262-extra/object-pattern-rest-getter-gc-rooting.js` — new regression
  for the reported panic (slice 1).
- `test262-extra/copy-data-properties-accumulated-values-gc-rooting.js` — new
  regression for the internal accumulator hazard, exercised through both the
  object-rest-destructuring path and the object-literal-spread path so the
  shared fix in `copy_data_properties` is proven at both call sites (slice 2).
- `test262-extra/object-pattern-rest-assignment-target-gc-rooting.js` — new
  regression for the assignment-target hazard (slice 3).
- No `docs/adr/` entry: this is a bug fix inside an already-established GC
  rooting discipline (see `CLAUDE.md`'s "GC Root-Stack Discipline" section),
  not a new architectural decision.

## 4. TDD slices

1. **Red:** add `test262-extra/object-pattern-rest-getter-gc-rooting.js`:
   `const {...x} = { get v() { <churn + $262.gc()>; return 2; } }`,
   `features: [host-gc-required, object-rest, destructuring-binding]`.
   Assert more than `x.v === 2` — also assert the *shape* is exactly right
   (`Object.getOwnPropertyNames(x)` is exactly `["v"]`, no stray `churn` key,
   `Object.getPrototypeOf(x) === Object.prototype`). This matters because a
   freed arena slot is typically reused by the churn allocator's own
   `{churn: i}` objects; without the shape check, `insert_value` silently
   writing `"v"` into a *reused, wrong* object can still make `x.v === 2`
   pass while `x` is corrupted. Confirm this test is red on the unreleased
   binary two ways before trusting it: plain `$262.gc()` via
   `uv run python scripts/run-test262.py test262-extra/object-pattern-rest-getter-gc-rooting.js`,
   and the issue's own repro recipe via
   `JSSE_GC_STRESS=1 ./target/release/jsse <repro.js>`. If plain `$262.gc()`
   doesn't reproduce the panic/corruption, keep shaping the churn helper
   (allocation count/shape) until it does, rather than shipping a green
   placeholder.
   **Green:** in `bind_object_rest_values`, root `rest_obj_id` — e.g.
   `let pairs = propagate!(self.with_gc_root_scope(|interp| { interp.gc_root_id(rest_obj_id); interp.copy_data_properties(o.id, source_val, excluded) }));`
   — so the rest object is reachable for the whole `copy_data_properties`
   call, then unrooted (by `with_gc_root_scope`'s own exit) before the
   `insert_value` loop that follows, which does not run user code and so
   cannot itself trigger a collection.
2. **Red:** add `test262-extra/copy-data-properties-accumulated-values-gc-rooting.js`
   — an object with two enumerable getters, the first returning a fresh
   object (e.g. `{ tag: "first" }`), the second calling the churn+`$262.gc()`
   helper before returning a primitive; destructure it with
   `const {...x} = source` and assert `x.a.tag === "first"` *and*
   `!("churn" in x.a)` (same reused-slot concern as slice 1). Add a second,
   independent assertion block doing the equivalent through object-literal
   spread (`const y = { ...source2 }` against a second, freshly-constructed
   source) to prove the shared fix covers that call site too. Confirm red
   the same two ways as slice 1 (this hazard is independent of
   `rest_obj_id` rooting, so it must still reproduce with slice 1's fix
   alone in place).
   **Green:** in `copy_data_properties`, wrap the existing body in
   `self.with_gc_root_scope(|interp| { ... })` (renaming the body's `self.`
   receivers to `interp.`), and add `interp.gc_root_value(&val);`
   immediately after `let val = match interp.get_object_property(...)` and
   before `result.push((key_str, val))`. Every early return (`?` on
   `proxy_own_keys`/`to_property_key`, the `Err(e) => return Err(e)` arms)
   now unroots via the scope's own exit instead of leaking or
   under-unrooting.
3. **Red:** add `test262-extra/object-pattern-rest-assignment-target-gc-rooting.js`
   — `var o = {}; var calls = 0; ({ ...o[(calls++, churnAndGc(), "k")] } = { a: 1, b: 2 });`
   (a computed member target whose key expression triggers the churn+`$262.gc()`
   helper, with a getter-free source so slices 1–2's fixes don't mask this
   one), then assert `o.k.a === 1 && o.k.b === 2` and the shape checks from
   slice 1 applied to `o.k`. Confirm red the same two ways.
   **Green:** in `destructure_object_assignment`'s
   `Expression::Spread(inner)` arm, root `rest_val` across
   `put_value_to_target`:
   ```rust
   let rest_val = propagate!(self.bind_object_rest_values(&obj_val, &excluded_keys));
   let put_result = self.with_gc_root_scope(|interp| {
       interp.gc_root_value(&rest_val);
       interp.put_value_to_target(inner, rest_val, env)
   });
   match put_result {
       Completion::Normal(_) | Completion::Empty => {}
       other => return other,
   }
   ```
4. **Refactor:** none planned — all three changes are additive rooting calls
   around existing control flow; no structural cleanup is in scope for a bug
   fix PR.

## 5. Test surface

- Targeted test262 run (must stay green, both before and after — these are
  the tests the issue names and their siblings):
  `uv run python scripts/run-test262.py --jsse ./target/release/jsse --test262 ./test262 test262/test/language/statements/const/dstr/`
  (and the `let`/`var` siblings), plus
  `test262/test/language/expressions/object/` (object-literal spread) and
  `test262/test/language/expressions/assignment/destructuring/` (assignment
  rest, which funnels through `destructure_object_assignment`).
- GC-stress confirmation of the exact issue repro (required before calling
  this fixed, run against the plain release binary with the issue's own
  recipe): `JSSE_GC_STRESS=2 ./target/release/jsse <repro.js>` — clean, no
  panic, after the fix.
- New regressions, run directly the same two ways as the targeted suite
  above, by path instead of directory.
- Match the actual CI gates (`.github/workflows/ci.yml`) rather than
  approximating them, since this fix's correctness is specifically about the
  debug-assert-guarded rooting invariants those gates exercise:
  - `cargo build --release` then
    `uv run python scripts/run-test262.py --jsse ./target/release/jsse --test262 ./test262 test262-extra/ --fail-on-failures`
    and the same with `--bytecode` appended (object-rest patterns are not
    compiled — see §6 — so `--bytecode` runs them on the tree-walker fallback
    exactly as the non-bytecode invocation does; running it anyway matches
    CI and guards against that bail coverage changing later).
  - `JSSE_GC_STRESS=7 uv run python scripts/run-test262.py --jsse ./target/release/jsse --test262 ./test262 test262-extra/ --timeout 300 --fail-on-failures`
    and the same with `--bytecode` appended — this is the blocking CI gate
    from #831 and the most direct re-run of the issue's own discovery
    conditions (`JSSE_GC_STRESS=2 --sample 0.003`) at higher intensity.
  - `cargo build --profile release-checked` then
    `uv run python scripts/run-test262.py --jsse ./target/release-checked/jsse --test262 ./test262 test262-extra/ --fail-on-failures`
    (and `--bytecode`) — this binary has `debug_assert!`s compiled in,
    including `gc_assert_root_depth` and `with_gc_root_scope`'s
    truncate-on-every-exit behavior, which is the actual mechanism slices
    1–3 lean on; a missed exit path fails loudly here even without
    `JSSE_GC_STRESS`.
  - `cargo test` (plain, debug profile — not `--release`) for the
    in-crate unit tests, which also exercise these debug assertions.
- test262 does not and cannot cover any of these three hazards (test262 has
  no `$262.gc()` dependency in its binding-rest, CopyDataProperties, or
  assignment-destructuring tests), so the three new `test262-extra/` tests
  are the only coverage for this fix; no `tests/` entry is needed since this
  is an observable-value/throw regression (spec-correct behavior under GC
  pressure), not a host-compatibility or resource-limit diagnostic.

## 6. Regression risk

- All three changes are purely additive GC-rooting calls (`gc_root_id`,
  `gc_root_value`, `with_gc_root_scope`) around existing control flow; they
  do not change any value produced, any order of operations, or any
  thrown/caught error — so no `test262-pass.txt` movement is expected in
  either direction.
- Shared machinery touched: `copy_data_properties` backs object-rest
  destructuring (binding and assignment), the state-machine `ObjectRestCopy`
  terminator (generators/async functions — confirmed its own call sites
  write straight to an environment binding with no further user code, so it
  needs no slice-3-style fix of its own), and object-literal spread. The
  targeted test262 directories in §5 cover all of these consumers directly.
  No bytecode-specific fix is needed because object-rest patterns are not
  yet compiled — confirmed by grepping `src/interpreter/bytecode/compiler.rs`
  for rest/object-pattern handling (none found) — so they already execute
  entirely on the tree-walker via the `CompileError::Unsupported`
  bail-to-tree-walker path; the `--bytecode` test262 runs in §5 are there to
  catch it if that ever changes, not because a bytecode-specific fix is
  expected now.
- `with_gc_root_scope`'s truncate-on-every-exit behavior (not just the tail)
  is exactly what all three fixes rely on given `copy_data_properties`'s
  multiple `?`/`return Err(...)` early exits and `put_value_to_target`'s
  multiple `Completion` variants; `gc_assert_root_depth` and the
  `release-checked` test262 run in §5 are the backstop if an exit path were
  missed.
- `bind_object_rest_values`'s doc comment currently documents the exact
  hazard being fixed; it must be updated (not just left stale) once the
  rooting is in place, since the next reader should not re-discover and
  re-flag an already-closed gap.

## 7. Out of scope

- The pre-existing evaluation-order deviation in `destructure_object_assignment`'s
  rest arm, where `put_value_to_target` re-evaluates the target's base/key
  expressions instead of using a `lRef` resolved *before* `bind_object_rest_values`
  runs (as `sec-runtime-semantics-restdestructuringassignmentevaluation`
  steps 1–4 require, and as the sibling `PropertyKind::Init` arm in the same
  function already does via `eval_member_lhs_ref`/`with_destruct_lref`). This
  is an evaluation-order correctness question, not a GC-rooting one, and
  slice 3's fix is correct regardless of which order these two things
  happen in. Worth its own issue if a test262 case demonstrates
  observable-order breakage.
- Auditing every other `create_object_id`/`create_array`-then-populate site
  in the interpreter for the same "allocate before populate, user code runs
  in between" shape. This PR fixes the three instances the issue's own
  repro and the immediate code walk around it surfaced; a broader sweep
  belongs to the `pm-deepen` GC-rooting backlog, not this bug-fix PR.
- Removing or simplifying the call sites in `literals.rs:1390-1393` that
  already do a (now partially redundant but harmless) post-return
  `self.gc_root_value(&v)` on pairs pulled out of `copy_data_properties` —
  those are cheap, correctly balanced under their own enclosing frame, and
  not the bug; touching them is unrelated cleanup.
- Any change to `object_arena.rs`'s dead-object-id debug assertion itself —
  it did exactly its job here (caught a real use-after-free instead of
  silently returning a wrong value) and should not be loosened.
- Rolling `test262-pass.txt` forward — not applicable on a feature branch
  regardless, and this fix is not expected to move it either direction.
