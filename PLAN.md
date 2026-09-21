# Plan: issue #313 — Date parser rejects toDateString() output and one-digit month strings

## 0. State of the world (read first — supersedes earlier plans on this branch)

This workspace was reused twice. The branch carries an old implementation
(`130fa8c`, `999a88c`, `96d5bc4`, `67006a0`) written against a `main` that is
now **71 commits ahead**. Two earlier plans called this "verify-and-ship";
that is no longer true, because `origin/main` gained #675
(`d3c785f1 fix(date): parse legacy date strings in Date.parse`) and #693.

Verified this run against a prebuilt post-#675 binary (the #674 workspace's
`target/release/jsse`, which contains `d3c785f1`):

| Input | post-#675 main | Verdict |
|---|---|---|
| `"Wed Jan 29 2026"`, `"Mon Sep 21 2026"`, `"Sat Jan 01 0099"`, `"Tue Jan 01 275760"` | finite | already fixed by #675 (`parse_legacy_written_month` eats weekday+month+day+year) |
| `Date.parse(d.toDateString()) === local midnight of d` (4 sample instants) | `true` | already fixed |
| `Date.parse(x.toString() / toUTCString() / toISOString()) === x` (4 instants) | all `true` | the triage comment's round-trip clause is already satisfied — no change needed |
| `"5"`, `"05"`, `"12"` | **NaN** | **still broken — this is what #313 still needs** |
| `"Fri Jan 01 -0001"` (negative-year `toDateString()`) | NaN | residual gap, see decision D1 |

The branch's own `test262-extra/date-parse-todatestring-roundtrip.js` **passes**
on post-#675 main (it no longer discriminates), and
`test262-extra/date-parse-bare-month-string.js` **fails** on it (expected).
So: half of the old branch is dead code with vacuous tests; the issue itself
is **not** resolved (`new Date("5")` is still Invalid Date).

## 1. Problem restated

`Date.parse` / `new Date(string)` return NaN for a bare one- or two-digit
decimal string (`"5"`), which Node (the issue's reference) reads as a month
number in reference year 2001 (`new Date("5")` → May 1 2001; Zod v4.4.3
`z.coerce.date()` hits this). The other half of the issue — `toDateString()`
output not parsing — is already fixed on `main` by #675 for every non-negative
year. The minimal remaining work is the bare-month fallback.

## 2. Spec basis

- **`sec-date`** (`Date ( ...values )`, one-argument branch, String case):
  the time value is the result of parsing the string "in exactly the same
  manner as for the `parse` method". Both entry points already share
  `parse_date_string`, so a single fix point suffices.
- **`sec-date.parse`** (`Date.parse ( string )`): after the Date Time String
  Format attempt, "the function may fall back to any implementation-specific
  heuristics or implementation-specific date formats. Strings that are
  unrecognizable or contain out-of-bounds format element values shall cause
  this function to return NaN." This clause *permits* the bare-month fallback;
  it does not mandate it, and does not mandate reference year 2001. Both are
  documented implementation choices (matching Node, per the issue), scoped so
  narrowly (1–2 ASCII digits, value 1–12) that no spec-mandated form can be
  affected — the shortest Date Time String Format year is 4 digits.
- The same clause's round-trip sentence (`x.valueOf()`,
  `Date.parse(x.toString())`, `Date.parse(x.toUTCString())`,
  `Date.parse(x.toISOString())` produce the same value when ms is zero) is
  already honored — see the table above. `toDateString()` is not in that
  list; its acceptance is an implementation-specific-format matter already
  handled by #675.

## 3. Decisions

- **D1 — drop the `parse_tostring_format` `< 5` → `< 4` relaxation
  (commit `130fa8c`); defer negative-year `toDateString()`.**
  For non-negative years it is dead code (main's legacy parser already
  accepts the string). Worse, `parse_tostring_format` runs *ahead* of
  `parse_legacy_date` and does not check the weekday token or bound the day,
  so it would preempt the legacy parser's stricter validation
  (`"foo Jan 99 2026"` → NaN on main, a rolled-over date with the relaxation;
  `sec-date.parse` says out-of-bounds values must be NaN). Its only unique
  value is negative years (`"Fri Jan 01 -0001"`, which
  `parse_legacy_digits` rejects because of the `-`); that is a BCE edge with no
  driver in the issue's reproduction, and the right fix lives in the legacy
  written-month parser's `LegacyNumber = (&str, u32)` model, not a parallel
  parser. Ship the minimal slice; file the follow-up (section 8).
- **D2 — bare-month tests go in `tests/`, not `test262-extra/`.** Main's
  precedent (#675): implementation-permitted formats are pinned in
  `tests/date-parse-legacy-formats.js`; `test262-extra/` is for
  spec-*mandated* invariants (already covered by
  `Date-parse-fallback-preserves-spec-formats.js`). Pinning `"0"`/`"13"` → NaN
  in `test262-extra/` would assert engine choices the spec does not require
  (Node parses `"0"` as year 2000). Hence the two old `test262-extra/date-parse-*`
  files are **not** carried over.
- **D3 — no Rust `#[cfg(test)]` unit tests.** #675 pinned its fallback through
  JS tests; a unit test on a private 1-line predicate adds nothing the JS test
  does not exercise through the public surface.

## 4. Files to touch (after slice 0)

- `src/interpreter/helpers.rs` — add `parse_bare_month_string` next to
  `parse_legacy_date` and call it from `parse_date_string` **after**
  `parse_legacy_date` (last fallback). Reuse `make_date_clipped(d, 0.0, true)`
  (as `make_legacy_local_date` does) for local-midnight; do not hand-roll
  `time_clip(utc_time(make_date(..)))`. Keep the existing sequential
  `if let Some(t) = … { return t }` style.
- `tests/date-parse-legacy-formats.js` — append a bare-month section (same
  `sameValue` helper style as the file).
- No `docs/adr/` or `CONTEXT.md` change: a leaf parser fallback, no new
  architecture or vocabulary.

## 5. TDD slices

0. **Rebase onto `origin/main`** (implementation stage; not done in planning).
   The branch was never pushed (`git ls-remote` shows no remote branch), so a
   reset is safe. Recommended non-interactive recipe:
   `git fetch origin main`; `git branch backup/313-old HEAD`;
   `git reset --hard origin/main`; `git checkout backup/313-old -- PLAN.md`
   (a single-file restore, no rebase in progress, so it keeps the
   "`git rm PLAN.md` before the PR" handoff working). Do **not** cherry-pick
   `130fa8c` (D1) or the two `test262-extra/date-parse-*` files (D2). A plain
   `git rebase origin/main` would also work but conflicts in
   `parse_date_string`'s fallback chain (main appended `parse_legacy_date` as
   the last arm exactly where the old commit appended its fallback) and drags
   in the vacuous Rust tests; the reset avoids both.
   Build first: `cargo build --release -j4` with an explicit long timeout
   (the 2-minute Bash default is too short); no `test262/` checkout exists in
   a fresh workspace — `git submodule update --init --depth 1 test262`.
1. **Red.** Append to `tests/date-parse-legacy-formats.js`: `Date.parse("5")`
   equals `new Date(2001, 4, 1).getTime()`; `"05"` equals `"5"`; `"1"` is Jan 1
   2001 and `"12"` is Dec 1 2001; `new Date("5")` agrees (constructor branch);
   `getFullYear() === 2001 && getMonth() === 4`. Run
   `uv run python scripts/run-custom-tests.py` — must fail on `"5"`.
   **Green.** Add `parse_bare_month_string` (1–2 ASCII digits, parsed value in
   1..=12, `make_day(2001.0, month - 1, 1.0)` at local midnight via
   `make_date_clipped`) and call it last in `parse_date_string`.
2. **Regression pins (should pass immediately; no production change).** In the
   same test file: NaN for `"0"`, `"13"`, `"32"`, `"100"` (3 digits stay
   unrecognized here), `"5.0"`, `"-5"`, `"+5"`, `"5a"`, `""`, `" "`; and the
   #313 reproduction itself:
   `Date.parse(new Date(2026, 8, 21).toDateString())` equals local midnight of
   that day, plus one instant on each side of a DST boundary
   (`new Date(2026, 0, 15)`, `new Date(2026, 6, 15)`). **Important:** do NOT
   assert `" 5 "` → NaN. `parse_date_string` trims before dispatch, so
   `Date.parse(" 5 ")` is accepted as May 2001 (consistent with main's
   `" 08/04/2011 "` "surrounding whitespace" test). Assert that instead, or
   omit it.
3. **Refactor/cleanup.** Update the header comment of
   `tests/date-parse-legacy-formats.js` (it enumerates accepted forms) to
   mention the bare-month form; nothing else. Run the quality gates as
   separate commands: `./scripts/lint.sh`, `cargo fmt --check`, `cargo test --release`.

## 6. Test surface

- Custom: `uv run python scripts/run-custom-tests.py` (includes
  `tests/date-parse-legacy-formats.js`).
- test262 targeted: `uv run python scripts/run-test262.py test262/test/built-ins/Date/`
  and `test262/test/annexB/built-ins/Date/`. No upstream test262 test uses a
  bare 1–2 digit Date string (grepped `built-ins/Date`, `language`, `annexB`,
  `intl402`), so no movement is expected.
- test262-extra: run `uv run python scripts/run-test262.py test262-extra/`
  — in particular `Date-parse-fallback-preserves-spec-formats.js`, which pins
  the toString/toUTCString/toISOString round trips and NaN for unrecognizable
  strings (none of its inputs is a bare 1–2 digit string; `""` and `" "` stay
  NaN because of the empty-after-trim early return).
- Zod harness (originating corpus, not a merge gate):
  `./scripts/run-library-tests.sh zod` — expected to drop the #313 failure in
  each mode (6 → 4 residual, the #314 cases remain).
- Full test262 run per CLAUDE.md; the baseline is read from `origin/main` and
  must not be rewritten (`--update-baseline` is out).

## 7. Regression risk

- Only `parse_date_string`'s tail changes: the new arm is reached only after
  ISO, toString, space-separated, toUTCString and legacy parsers all return
  `None`, and its input shape (1–2 ASCII digits) cannot overlap any of them
  (`parse_legacy_written_month("5")` bails at `month?`; numeric-slash needs a
  `/`; ISO needs ≥4-digit years). Regression surface = false accepts, which
  the NaN pins in slice 2 cover.
- Behavior change to call out in the PR body: strings that were NaN
  (`"1"`…`"12"`, incl. whitespace-padded and zero-padded) are now dates in
  2001. Any caller/test relying on `new Date("7")` being invalid changes.
- Does not touch `eval_expr`/`exec_statement`, `property.rs`, GC rooting,
  `ObjectKind`, or the bytecode fast path; blast radius is
  `src/interpreter/helpers.rs`. Library harnesses that parse dates
  (`moment` — jsse#311, `luxon`, `zod`) can only improve or stay equal; check
  `moment`'s pass count does not drop if run.
- `test262-pass.txt` baseline: not expected to move.

## 8. Out of scope / follow-ups

- **Follow-up issue to file (D1):** negative-year `toDateString()` output
  (`"Fri Jan 01 -0001"`) is NaN. Fix belongs in `parse_legacy_written_month`
  (accept a `-`-prefixed ≥4-digit year token *only* in that path — the
  numeric-slash path must keep rejecting `"1/1/-5"`, which main tests), with a
  test that fails on main first. Mention this in the PR description.
- Node's wider bare-number heuristics (`"0"` → 2000, `"32"` → 2032,
  `"50"` → 1950, 3+ digit bare years). Not required by the issue or the Zod
  corpus.
- Any change to `toString`/`toUTCString`/`toISOString`/`toDateString`
  formatting; table-driven refactor of the fallback chain; Locale/timezone-name
  extensions.
