# Plan: issue #704 — Date parser rejects negative-year `toDateString()` output

## 1. Problem restated

`Date.parse` accepts an implementation-defined legacy fallback format for written-month
date strings (e.g. `"May 1, 2000"`), added in #675/#313 so that jsse's own
`Date.prototype.toDateString()`/`toString()` output round-trips through `Date.parse`.
That fallback tokenizes on whitespace/comma and parses each numeric token with
`parse_legacy_digits`, which rejects any token containing a non-digit byte — including a
leading `-`. `Date.prototype.toDateString()` (via the shared `DateString` formatting,
`format_year_string`) renders years before 0 as a `-`-prefixed, zero-padded-to-4-digits
token (e.g. `"-0001"`), so `Date.parse(new Date(-62198755200000).toDateString())` —
`Date.parse("Fri Jan 01 -0001")` — falls all the way through every format branch and
returns `NaN` instead of round-tripping. The numeric-slash branch of the same fallback
(`parse_legacy_numeric_slash`, e.g. `"1/1/2000"`) must keep rejecting a `-`-prefixed year
token (`"1/1/-5"` is pinned `NaN` in `tests/date-parse-legacy-formats.js`), so the fix is
scoped to the written-month branch only, exactly as the issue's suggested fix says.

## 2. Spec basis

- **`Date.parse ( string )`** (`spec/spec.html`, clause id `sec-date.parse`): "The function
  first attempts to parse the String according to [Date Time String Format]... If the
  String does not conform to that format the function may fall back to any
  implementation-specific heuristics or implementation-specific date formats." This is
  *permission*, not a mandate — jsse's written-month legacy fallback exists entirely under
  this license (established by #675/#313), and this fix stays inside it.
- **`DateString ( tv )`** (clause id `sec-datestring`, the abstract operation `toDateString`
  and `toString` build on): step 5 — "If `_yv_` is `+0` or `_yv_` > `+0`, let `_yearSign_`
  be the empty String; otherwise let `_yearSign_` be `"-"`" — and step 6, `paddedYear` is
  `ToZeroPaddedDecimalString(abs(yv), 4)`. This is the source of the exact string shape
  (`-` + zero-padded-≥4-digit magnitude) the parser must accept. `Date.prototype.toUTCString`
  (clause id `sec-date.prototype.toutcstring`) specifies the identical yearSign/paddedYear
  rule inline, confirming `-`-prefixed, no-`+`, ≥4-digit-padded is the one shape both
  formatters ever produce for out-of-range years.
- **No mandated round trip for `toDateString`.** The round-trip note under
  `Date.prototype.toString` (clause id `sec-date.prototype.tostring`) only promises
  `x.valueOf() === Date.parse(x.toString())` (and `toUTCString`/`toISOString`) for
  zero-millisecond Dates — it does not mention `toDateString`. Confirmed empirically:
  `node -e '...'` also returns `NaN` for `Date.parse(new Date(-62198755200000).toDateString())`.
  So this fix is motivated by *internal consistency* with the fallback #675/#313 already
  built (which accepts non-negative `toDateString()` output), not by a spec or `node`
  requirement. Because it's not a spec-pinned invariant, its test belongs in
  `tests/date-parse-legacy-formats.js` (implementation-defined behavior), not
  `test262-extra/`.

## 3. Files to touch

- `src/interpreter/helpers.rs`
  - `parse_legacy_written_month` (~line 2104): add a negative-year-token branch to the
    tokenizing loop.
  - New small helper(s) near it, used *only* from `parse_legacy_written_month`:
    a token matcher for a `-`-prefixed year, and a small date-construction helper that
    takes the year as a signed value (see §4). `parse_legacy_digits`,
    `parse_legacy_numeric_slash`, `is_legacy_year_first`, `expand_legacy_year`, and
    `make_legacy_local_date` are **not modified** — this is what keeps the numeric-slash
    branch's rejection of `"1/1/-5"` untouched-by-construction rather than by a new
    conditional.
- `tests/date-parse-legacy-formats.js` — add accept-case and reject-case assertions (§5).
  Update the file's top-of-file comment block to note that the written-month form also
  accepts a `-`-prefixed ≥4-digit year (round-tripping negative-year `toDateString()`
  output), since that comment currently documents the accepted-format surface for future
  readers.
- No `docs/adr/` entry: this is a bug-fix-sized extension of an existing
  implementation-defined heuristic (#675/#313), not a new architectural decision.

## 4. TDD slices

1. **Red — accept case.** In `tests/date-parse-legacy-formats.js`, add:
   ```js
   var negYear = new Date(-62198755200000); // toDateString(): "Fri Jan 01 -0001"
   sameValue(
     Date.parse(negYear.toDateString()),
     local(negYear.getFullYear(), negYear.getMonth(), negYear.getDate()),
     "toDateString round-trip for year -1"
   );
   ```
   Deriving both the input (via `.toDateString()`) and the oracle (via the existing
   `local()` helper, fed from the same Date's own `getFullYear`/`getMonth`/`getDate`)
   keeps the assertion TZ-independent and avoids hand-computing an expected timestamp.
   Do **not** use `Date.parse(negYear.toString())` as a cross-check in this test: the
   full `toString()` output includes a `GMT±hh:mm` offset, and for pre-1970 instants in
   this host's zone that offset has a non-zero seconds/sub-minute component (confirmed:
   `new Date(-62198755200000).toString()` → `"...GMT+0053 (LMT)"`, and
   `Date.parse` of that round-trips to a time 28s off `negYear.getTime()`); that gap is
   pre-existing, unrelated to this issue, and would make the test flaky-by-design.
   Add 2-3 order-permutation variants for confidence (mirroring the existing
   `writtenMay2000` array's style), e.g. `"-0001 may 1"`, `"may 1 -0001"`,
   `"Mon may 1 -0001"`, each compared against `local(-1, 4, 1)`.
   Run `./target/release/jsse tests/date-parse-legacy-formats.js` — confirm it now throws
   (red) on the new assertions before any production change.
2. **Green — implement.** In `src/interpreter/helpers.rs`, add a token matcher (e.g.
   `parse_legacy_negative_year(token: &str) -> Option<i64>`) that requires: the token
   starts with `-`, the remainder is 4 to 6 ASCII digits (mirrors `parse_legacy_digits`'s
   existing magnitude cap, budgeted for sign + digits rather than reused verbatim —
   `new Date(-8.64e15)`'s `toDateString()` year token is `-271821`, 6 digits after the
   sign, 7 chars total), and only then parses to `i64` and negates. Requiring **≥4**
   digits after the sign is load-bearing: it is what keeps a negative value from ever
   reaching `expand_legacy_year`'s 1–2-digit two-digit-year arms (which this new path
   bypasses entirely, since it never calls `expand_legacy_year`) — so `"may 1 -5"` must
   stay rejected (§5), not become some remapped year.
   In `parse_legacy_written_month`'s tokenizing loop, check each token against this
   matcher *before* `parse_legacy_digits`; on a match, store it in a new
   `Option<i64>` local (call it `neg_year`), returning `None` immediately if it was
   already `Some` (mirrors the existing `month.replace(...).is_some()` double-set guard).
   A matched token is **not** pushed into the existing `numbers` vec.
   After the loop: if `neg_year` is `Some(y)`, require `numbers` to contain **exactly
   one** entry (the day — a negative-year token can only ever occupy the year role,
   never day-of-month, so this doesn't need `is_legacy_year_first`'s order-inference);
   anything else (0, 2+ remaining numbers) is `None`. Validate `month`/`day` with the
   same `1..=12`/`1..=31` range checks `make_legacy_local_date` already applies (add a
   small sibling, e.g. `make_legacy_local_date_signed(year: i64, month: u32, day: u32)`,
   doing the same range checks plus `make_day`/`make_date_clipped`, skipping
   `expand_legacy_year` since the ≥4-digit requirement already makes the value literal).
   If `neg_year` is `None`, fall through to the existing two-number logic unchanged.
   Rebuild and re-run step 1's test — confirm green.
3. **Red/green — reject cases (regression guards).** Add to the `invalidWritten` array in
   the same test file (these already return `NaN` today via the blanket
   `parse_legacy_digits` rejection of any `-`; the point is to pin the *reason* they stay
   `NaN` doesn't get accidentally loosened by this change):
   - `"may 1 -5"` — negative year token present but only 3 digits (must stay `NaN`, not
     become some 4-digit-heuristic year).
   - `"-1 -2 may"` — two negative-year-shaped tokens (mirrors the existing double-month
     rejection path).
   - `"may 1 2000 -0001"` — a valid negative-year token plus two already-valid numbers
     (more than one remaining `numbers` entry once `neg_year` is set).
   - `"2000-13-01"` — a single non-space-separated token containing an internal `-`
     that isn't a leading sign; confirms the new matcher's "starts with `-`" requirement
     doesn't let ISO-shaped or otherwise malformed strings newly reach the written-month
     fallback.
   Run the full test file again after step 2's implementation — all of these should
   already pass (they're guards, not new red tests), which itself confirms the new
   matcher is correctly scoped.
4. **No change needed, verify only.** Confirm the existing `invalid` array's
   `"1/1/-5"` (numeric-slash path) still returns `NaN` — it must, since
   `parse_legacy_numeric_slash`/`parse_legacy_digits` are untouched by this change.

## 5. Test surface

- **Primary test:** `tests/date-parse-legacy-formats.js` (implementation-defined legacy
  fallback; TZ-independent via the local `Date` constructor, per its own header). All new
  assertions from §4 land here. Run: `./target/release/jsse tests/date-parse-legacy-formats.js`
  and `uv run python scripts/run-custom-tests.py`.
- **Not test262-covered:** confirmed via `test262/test/built-ins/Date/parse/` (only
  `length.js`, `name.js`, `not-a-constructor.js`, `prop-desc.js`,
  `time-value-maximum-range.js`, `without-utc-offset.js`, `year-zero.js`, `zero.js` —
  none exercise the implementation-specific fallback), consistent with this being
  implementation-defined behavior outside the Date Time String Format. No new
  `test262-extra/` file: per §2, there is no spec-pinned invariant here to encode (the
  round-trip note doesn't cover `toDateString`), so `test262-extra/` is not the right
  home — `tests/` already is.
- **Targeted test262 run for regression-only confirmation** (no behavior there should
  move, but the change touches a shared file):
  `uv run python scripts/run-test262.py test262/test/built-ins/Date/`
- **Full gates**, run separately (never `&&`-chained):
  - `./scripts/lint.sh`
  - `cargo test --release`
  - `uv run python scripts/run-custom-tests.py`
  - `uv run python scripts/run-test262.py` (full suite; compare against baseline from
    `origin/main:test262-pass.txt`, expect zero regressions and no forward movement
    needed since this doesn't add new spec-covered passes)

## 6. Regression risk

- The change is additively scoped to `parse_legacy_written_month`'s token loop plus two
  new small functions; `parse_legacy_digits`, `parse_legacy_numeric_slash`,
  `is_legacy_year_first`, `expand_legacy_year`, and `make_legacy_local_date` are
  untouched, so the numeric-slash branch and all previously-accepted non-negative
  written-month forms (`writtenMay2000`, `September 3 2001`, `dec 31 1999`, etc.) keep
  their existing code paths byte-for-byte.
- `parse_date_string`'s dispatch order (ISO → `toString`-format → space-separated →
  `toUTCString`-format → legacy fallback) is unchanged; only the legacy written-month
  branch's internal acceptance set grows.
- Not on the tree-walker hot path (`eval_expr`/`exec_statement`), not touching the
  property MOP (`property.rs`), GC rooting, `ObjectKind`, or the bytecode fast path — this
  is pure string-parsing helper code reached only from `Date.parse`/`new Date(string)`.
- Node-compat library harnesses: `moment`/`luxon`/`zod` all exercise date parsing; a
  behavior change here is exactly the kind of thing that could shift their pass counts.
  None of their fixtures are expected to contain negative-year strings in written-month
  form, but re-run `./scripts/run-library-tests.sh moment`, `luxon`, and `zod` (or at
  least `moment`, which has the most date-string-heavy coverage) after the change to
  confirm their tracked pass/fail counts (jsse#311, jsse#262–#265, jsse#313/#314) don't
  move.

## 7. Out of scope

- **`+`-prefixed years (> 9999) are not handled by this fix, and must not be.** Verified:
  `Date.parse(new Date(8.64e15).toDateString())` — `Date.parse("Sat Sep 13 +275760")` —
  is also `NaN` today, symmetric-looking to this issue, but it is **not** the same class
  of bug. `format_year_string` (`src/interpreter/helpers.rs:1896`, shared by
  `format_date_string` for `toString`, the `toUTCString` formatter, and
  `format_date_only_string` for `toDateString`) emits `format!("+{}", yi)` for `yi > 9999`.
  Per `DateString` (`sec-datestring`) step 5 and `Date.prototype.toUTCString`
  (`sec-date.prototype.toutcstring`)'s identical inline rule, `yearSign` is only ever the
  empty string or `"-"` — **never** `"+"` — so `"Sat Sep 13 +275760"` is itself spec-wrong
  output; the correct `toDateString()` result is `"Sat Sep 13 275760"`. The right fix is
  in the formatter (`format_year_string`), not a parser change to accept a `+`-prefixed
  token. This is a separate, pre-existing bug (affecting `toString`/`toUTCString`/
  `toDateString` alike) — file a follow-up issue rather than bundling a formatter fix into
  this parser-focused PR.
- No refactor of `LegacyNumber`/`parse_legacy_digits` to a shared signed type across both
  the slash and written-month branches, even though that would be more "DRY" — it would
  require changing `is_legacy_year_first`, `expand_legacy_year`, and
  `make_legacy_local_date`'s shared signatures, increasing blast radius on the
  numeric-slash branch for no behavioral gain. The two new small functions scoped to
  `parse_legacy_written_month` are the minimal change.
- No change to `parse_tostring_format` (the full `toString()`-format branch) — it already
  accepts negative years today (confirmed:
  `Date.parse(new Date(-62198755200000).toString())` parses successfully, modulo the
  pre-existing sub-minute historical-offset rounding noted in §4, which is out of scope).
- No baseline update (`test262-pass.txt` via `--update-baseline`) — not applicable on a
  feature branch per project convention.
