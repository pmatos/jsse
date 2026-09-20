# Plan: issue #644 — `Intl.Locale` has a spurious static `supportedLocalesOf`

## 1. Problem restated

`Intl.Locale.supportedLocalesOf` is installed as an own data property of the `Intl.Locale` constructor
(`typeof` is `"function"`, and `Object.getOwnPropertyNames(Intl.Locale)` includes it). ECMA-402 defines
`supportedLocalesOf` only on the *service constructors* (Collator, DateTimeFormat, DisplayNames,
DurationFormat, ListFormat, NumberFormat, PluralRules, RelativeTimeFormat, Segmenter). `Intl.Locale` is not
a service constructor: its only own static property besides `length`/`name` is `prototype`. The property
was copy-pasted from the sibling constructors. Fix: delete the wiring in
`src/interpreter/builtins/intl/locale.rs` (lines ~1620-1639, the block under
`// supportedLocalesOf static method`).

## 2. Spec basis

`spec/` is ECMA-262 only; ECMA-402 is not vendored in the repo, so the clauses below are cited from
ECMA-402 by name (section numbers are per the current draft and should be double-checked against
https://tc39.es/ecma402/ when writing the PR body):

- **ECMA-402, Locale Objects → "Properties of the Intl.Locale Constructor"** (§14.2 in recent drafts):
  lists exactly one property, `Intl.Locale.prototype`. No `supportedLocalesOf`. `Intl.Locale` is
  defined with `length` 1 and `name` "Locale" like any built-in constructor.
- **ECMA-402, "Internal slots of Service Constructors" / per-constructor
  `Intl.<Service>.supportedLocalesOf ( locales [ , options ] )`** (Collator §10.2.2,
  DateTimeFormat §11.2.2, NumberFormat §15.2.2, PluralRules §16.2.2, ... and likewise DisplayNames,
  DurationFormat, ListFormat, RelativeTimeFormat, Segmenter): the static is specified per service
  constructor and each is defined in terms of `AvailableLocales` of that service. `Intl.Locale` has no
  `[[AvailableLocales]]` and is absent from that list.
- **ECMA-262, "ECMAScript Standard Built-in Objects"** (clause 18): built-in objects have only the
  properties the spec specifies; an unspecified own property is non-conformant.

Removing a non-specified own property is grounded in the absence of any such clause.

## 3. Files to touch

- `src/interpreter/builtins/intl/locale.rs` — delete the `supportedLocalesOf` block (the
  `let slof = self.create_function(...)` + `insert_builtin("supportedLocalesOf", slof)`, incl. its
  `// supportedLocalesOf static method` comment). Keep the surrounding `if let Some(ctor_id)` block that sets
  `prototype`.
- `test262-extra/Intl-Locale-no-supportedLocalesOf.js` — new regression test (see slice 1).

Not touched: `intl_supported_locales` / `intl_canonicalize_locale_list` (still used by the 9 service
constructors and by `string.rs` (`intl_canonicalize_locale_list`), so no dead-code fallout under
`clippy -D warnings`); `use` lines in `locale.rs` (`JsFunction`/`create_function` still used extensively).
No `docs/`, `CONTEXT.md`, or ADR changes: no new vocabulary or architectural decision. No other
references to `Intl.Locale.supportedLocalesOf` exist in `src/`, `tests/`, `test262-extra/`, `scripts/`,
`docs/`, `benchmarks/` (checked by grep).

## 4. TDD slices

Build first: `cargo build --release -j4` (explicit long timeout; keep parallelism capped for the shared
memory budget). Init the suite: `git submodule update --init --depth 1 test262`.

1. **Red — new test.** Add `test262-extra/Intl-Locale-no-supportedLocalesOf.js` (test262 frontmatter
   `/*--- description: ... features: [Intl.Locale] includes: [compareArray.js] ---*/`, header comment
   naming the ECMA-402 clause "Properties of the Intl.Locale Constructor", matching the style of
   `Intl-Locale-getCalendars-region-preference.js`). Assertions:
   - `Intl.Locale.supportedLocalesOf === undefined` and `typeof` is `"undefined"`.
   - `Object.prototype.hasOwnProperty.call(Intl.Locale, "supportedLocalesOf") === false`.
   - `Object.getOwnPropertyNames(Intl.Locale).sort()` deep-equals `["length", "name", "prototype"]`
     (sorted, since own-key order of built-ins is not spec-mandated).
   - A subclass `class L extends Intl.Locale {}` also has no `supportedLocalesOf`.
   - Guard against over-removal: each of `Collator, DateTimeFormat, DisplayNames, DurationFormat,
     ListFormat, NumberFormat, PluralRules, RelativeTimeFormat, Segmenter` still has
     `typeof Intl[name].supportedLocalesOf === "function"`.
   Run `uv run python scripts/run-test262.py test262-extra/Intl-Locale-no-supportedLocalesOf.js` (or the
   directory) and confirm it fails on the current tree (red).
2. **Green.** Delete the `supportedLocalesOf` block from `locale.rs`. Rebuild; rerun the new test → passes.
3. **Refactor / lint.** `./scripts/lint.sh` (rustfmt + clippy); confirm no unused-import/dead-code
   warnings. No refactor beyond the deletion is planned.

## 5. Test surface

- New: `test262-extra/Intl-Locale-no-supportedLocalesOf.js` (spec-correct behavior not covered by
  test262, as far as the baseline shows: `intl402/Locale/*` has no such test entry).
- Targeted test262 runs (separate commands, not `&&`-chained):
  - `uv run python scripts/run-test262.py test262/test/intl402/Locale/`
  - `uv run python scripts/run-test262.py test262/test/intl402/Intl/` (namespace-level checks)
  - `uv run python scripts/run-test262.py test262/test/intl402/Collator/supportedLocalesOf/` and the
    sibling `*/supportedLocalesOf/` directories as a sanity check that the service constructors are
    untouched.
  - `uv run python scripts/run-test262.py test262-extra/`
- Full gate before PR: `uv run python scripts/run-test262.py` (expect `regressions: 0`), then
  `cargo test --release`, `uv run python scripts/run-custom-tests.py`, `./scripts/lint.sh`.
  Never rebuild the binary while a full run is in flight.

## 6. Regression risk

Very low. The change is a pure deletion of one property on one constructor.
- `test262-pass.txt`: no `intl402/Locale/*supportedLocalesOf*` entries exist in the baseline (grep; the
  test262 checkout is empty in this workspace, so re-confirm after `git submodule update --init`),
  so no baseline entry can regress; the only conceivable movement is a test that asserts the constructor's
  own keys, which would move in the passing direction. Baseline is not to be rolled forward
  (`--update-baseline` must not be used on this branch).
- Shared machinery: not touching the tree-walker hot paths, `property.rs` MOP, GC rooting /
  `gc_safepoint()`, exhaustive `ObjectKind` matches, or the bytecode path. The removed function object
  was an unrooted-until-installed builtin; removing its creation only reduces allocations at realm setup.
- Library harnesses: Node lacks the property too, so no Node-cross-checked library can depend on it; no
  library run required.

## 7. Out of scope

- Deduplicating the `supportedLocalesOf` wiring across the nine service constructors (a possible
  `define_supported_locales_of` helper) — separate refactor.
- Any other `Intl.Locale` conformance gaps, and audit of other constructors' static surface.
- Formatting or unrelated cleanups in `locale.rs`; `CONTEXT.md`/ADR changes.

PR title (squash subject): `fix(intl): remove spurious Intl.Locale.supportedLocalesOf static`.
The implementation stage must `git rm PLAN.md` before opening the PR.
