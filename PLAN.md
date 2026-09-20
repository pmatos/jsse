# Plan: issue #653 — JetStream `validatorjs` fails with "Assertion failure: 2010-07-02,[object Object]"

## 1. Problem restated

The issue says "needs minimizing"; that is done. Reproduced on this branch (release build, JetStream `c603c04`, `/tmp/JetStream`), then ran the validatorjs bundle with `assert` patched to *collect* failures instead of throwing. jsse reports `count=10114 fails=6`, node reports `fails=0`. All six failures are the `isBefore` cases with `comparisonDate: "08/04/2011"`:

```
2010-07-02,[object Object]      2010-07-02,08/04/2011      (new + legacy syntax)
2010-08-04,[object Object]      2010-08-04,08/04/2011
<Date(0).toString()>,[object Object]   <Date(0).toString()>,08/04/2011
```

Root cause: `Date.parse("08/04/2011")` returns `NaN` in jsse (node: local midnight, `1312408800000` in CEST). validator's `isBefore` does `toDate(comparisonDate)` -> `Date.parse` -> `NaN`, so every `d < NaN` is false and `valid` assertions fail. The first failing one is the `[object Object]` message in the issue. `isBefore` with `new Date(2011,7,4).toString()` already passes (the `toString` format is parsed). Nothing here is bytecode- or iterator-related (`--bytecode` fails identically; #52's "next called on non-array iterator" was a different, since-fixed bug).

`parse_date_string` (`src/interpreter/helpers.rs:2010`) tries ISO, `toString`, the SpiderMonkey-style `1997-3-8 1:1:1` relaxed form, and `toUTCString`, then gives up. There is no legacy fallback (`M/D/YYYY`, `Month D YYYY`, ...). The fix is to add one, last in the chain.

**Important discovered constraint (load-bearing, do not skip).** `test262/test/staging/sm/Date/two-digit-years.js` currently reports PASS *vacuously*: every assertion compares `new Date("m/d/YYYY")` with `new Date("m/d/yy")` / `new Date("may 1 yy")` via `assert.sameValue(x.getTime(), y.getTime())`, and on jsse today both sides are `NaN` (`SameValue(NaN, NaN)` is true). The file passes on node. A partial fix (only `M/D/YYYY`) makes `fullDate` a real number while `d1` stays `NaN`, turning that green staging test red. So the PR must also implement short-year mapping, `yy/m/d`, and written-month forms. (`staging/sm/Date/non-iso.js` is *not* vacuous — it asserts `NaN` for ISO-shaped strings such as `"1997-3-8T11:19:20"` and `"1997-03-08 11"`; the new fallback must not accept any of them.)

Decision: **option (a)** — one PR, three vertical slices (numeric `M/D/YYYY`; short years + `Y/M/D`; written months). Time-of-day / AM-PM / timezone designators in legacy strings are a follow-up (section 7). `staging/sm/Date/` (56 scenarios, currently 56/56) must stay 56/56 and is now a genuine check.

## 2. Spec basis

- **`sec-date.parse` — Date.parse ( string )** (ECMA-262 21.4.3.2, `spec/spec.html:34296`). Authorizes the change: *"The function first attempts to parse the String according to the format described in Date Time String Format ... If the String does not conform to that format the function may fall back to any implementation-specific heuristics or implementation-specific date formats. Strings that are unrecognizable or contain out-of-bounds format element values shall cause this function to return NaN."* Also: the result is *"implementation-defined when given any String value that does not conform to the Date Time String Format and that could not be produced ... by the `toString` or `toUTCString` method"* — so the accepted legacy set is ours to choose; the choice below is web-compat (node/SpiderMonkey agree on it) and deliberately conservative.
- **`sec-date-time-string-format`** (`spec.html:33944`): the ISO format must be attempted first and must win; ISO-shaped strings with out-of-bounds values must still yield `NaN`. The fallback therefore runs *last* and its accepted alphabet excludes `-`, `+`, `:`, `T`, so it can never accept an ISO-shaped string (which always contains `-`).
- **Round-trip requirement in `sec-date.parse`**: `Date.parse(x.toString())`, `Date.parse(x.toUTCString())`, `Date.parse(x.toISOString())` == `x.valueOf()`. Already satisfied by the earlier parsers; the fallback sits after them so it cannot perturb it.
- **`sec-utc-t`, `sec-localtime`**: a string with no offset in the legacy forms is interpreted as **local time** (node: `"08/04/2011"` = local midnight, unlike `"2011-08-04"` = UTC midnight), so the fallback ends in `UTC(MakeDate(...))`. Matches the local-time model in `test262/test/built-ins/Date/parse/without-utc-offset.js`.
- **`sec-makeday`, `sec-maketime`, `sec-makedate`, `sec-timeclip`**: composition of the resulting time value (`make_day`/`make_date`/`time_clip`, already used by sibling parsers; `make_date_clipped(day, time, is_local)` at `helpers.rs:1831` does UTC()+TimeClip).

This is a JavaScript behavior change (not `N/A`); the clause both permits it and bounds it (NaN for unrecognizable/out-of-bounds).

## 3. Files to touch

- `src/interpreter/helpers.rs` — add `parse_legacy_date(s: &str) -> Option<f64>` (+ small private helpers: month-name lookup, short-year mapping) next to the sibling parsers, and call it as the **last** step of `parse_date_string` (after `parse_utcstring_format`). Only this function's tail changes; `Date` constructor (`builtins/date.rs:1142`) and `Date.parse` (`:1261`) already funnel through `parse_date_string`, so no call-site edits.
- `tests/date-parse-legacy-formats.js` (new) — exact legacy-format values (host-compat, per CLAUDE.md `tests/` vs `test262-extra/` split).
- `test262-extra/Date-parse-fallback-preserves-spec-formats.js` (new) — spec-required invariants only (see section 5).
- No `spec/`, `test262/`, `test262-pass.txt`, docs/ADR, or `CONTEXT.md` changes (no new architecture or vocabulary). Note: `spec/` and `test262/` were initialised in this worktree only to read them (`git submodule update --init --depth 1`); do not commit gitlink changes.

The PostToolUse hook runs `rustfmt` + `clippy -D warnings` on every `.rs` edit and blocks on dead code: land `parse_legacy_date` and its call site in the *same* edit in slice 1 and grow the function in place afterwards; do not add helpers ahead of their first use.

## 4. Fallback grammar (target end state after slice 3)

`parse_legacy_date` is only reached when every earlier parser returned `None`. Input is already `trim()`med by `parse_date_string`. It rejects (`None` -> `NaN`) unless the *entire* string matches, and rejects up front any character outside: ASCII digits, ASCII letters, `/`, space, `,`. That single filter guarantees no `-`, `+`, `:`, `.`, `(`, `T`-joined ISO shape can enter.

Two shapes:

1. **Numeric slash** `A/B/C` — no whitespace/commas/letters; exactly three digit runs, each 1–6 digits, separated by single `/` (so `"1/1/"`, `"1//2000"`, `"/1/2000"`, `"1/1/2000/1"` -> NaN).
   - If `A` has >= 3 digits or value > 31: `Y/M/D` (`A`=year, `B`=month, `C`=day). Else `M/D/Y` (`A`=month, `B`=day, `C`=year).
2. **Written month** — tokens split on spaces/commas: exactly one month word (case-insensitive; 3-letter abbreviation or full English name), exactly two digit runs (1–6 digits), optionally one weekday word (3-letter or full, ignored; whole string otherwise unrecognized -> NaN). Numerics in order `n1, n2`: if `n1` has >= 3 digits or value > 31 then year=`n1`, day=`n2`; else day=`n1`, year=`n2`. Covers `may 1 2000`, `1 may 2000`, `1 2000 may`, `may 2000 1`, `2000 may 1`, `2000 1 may`, `Mon, May 1 2000`. Any other word (`invalid`, `foo`) -> NaN.

Common tail: **short year** — a year token with 1–2 digits maps `< 50 -> +2000`, `>= 50 -> +1900`; 3+ digits are literal (matches node and `two-digit-years.js`: `5/1/0` == `may 1 0` == year 2000; `5/1/100` == year 100). Range: month `1..=12`, day `1..=31` (like `parse_space_separated_date`; `make_day` rolls `2/31` over, same as node), else `None`. Result `Some(make_date_clipped(make_day(y, m-1, d), 0.0, true))` — local midnight.

Required `NaN`s (spec: out-of-bounds/unrecognizable): `13/13/13`, `0/10/0`, `99/1/99`, `1/32/2000`, `0/1/2000`, `1/0/2000`, `13/1/1`, `31/1/1`, `1/1/-5`, `1/1/+2000`, `may 1999 1999`, `may 0 0`, `invalid date`, `foo`, plus every `non-iso.js` NaN string.

## 5. TDD slices (each = red test -> minimal green -> refactor; commit per slice)

Run tests with `cargo build --release -j 6` first (cap parallelism; ~75 s incremental) then `uv run python scripts/run-custom-tests.py tests/date-parse-legacy-formats.js`. Build and gate commands run as separate commands, not `&&`-chained.

1. **Numeric `M/D/YYYY` (closes the issue).**
   - Red: `tests/date-parse-legacy-formats.js` (throw on failure, like other `tests/*.js`): `Date.parse("08/04/2011") === new Date(2011, 7, 4).getTime()` (TZ-independent: compares to the local-time constructor), `"8/4/2011"`, `"2/29/2000"`, `"12/31/1999"`, whitespace-trimmed `" 08/04/2011 "`; ordering check `Date.parse("2010-07-02") < Date.parse("08/04/2011")`; NaN set: `1/32/2000`, `0/1/2000`, `1/0/2000`, `13/13/13`, `1/1/`, `1//2000`, `1/1/-5`, `1/1/2000/1`.
   - Green: `parse_legacy_date` with the numeric-slash shape, 4-digit-year only for now (`M/D/Y`), wired last in `parse_date_string`.
2. **Short years and `Y/M/D`.**
   - Red (append): `"1/1/0"`==2000, `"1/1/49"`==2049, `"1/1/50"`==1950, `"1/1/99"`==1999, `"1/1/100"` = year 100, `"1/1/999"`; `"2011/08/04"` and `"2011/8/4"` == `new Date(2011,7,4)`; `"50/1/1"`==1950, `"32/1/1"`==2032 (yy>31 => year-first); NaN: `"13/1/1"`, `"31/1/1"`, `"99/1/99"`, `"0/10/0"`.
   - Green: year-first branch and short-year mapping.
3. **Written months.**
   - Red (append): `may 1 2000`, `1 may 2000`, `1 2000 may`, `may 2000 1`, `2000 may 1`, `2000 1 may`, `May 1, 2000`, `Mon, May 1 2000`, mixed case, full `September`; short years (`may 1 5` == `5/1/2005`-shaped); NaN: `may 1999 1999`, `may 0 0`, `invalid date`, `foo`, `may`, `may 1`, `5/1 may 2000` (mixed shapes rejected).
   - Green: month-word shape; month/weekday name tables (a shared const table is also reusable by the existing `parse_tostring_format`/`parse_utcstring_format` `match` blocks, but do **not** refactor them here).
4. **Spec-invariant test** `test262-extra/Date-parse-fallback-preserves-spec-formats.js` (esid `sec-date.parse`; copy the header pattern of `test262-extra/BigInt-string-to-bigint-abstract-operation.js`, quote the "may fall back ... shall return NaN" text in `info:`). Asserts what must hold regardless of the fallback: (i) for several `x` (epoch, a 2011 date, pre-1970, ms=0), `Date.parse(x.toString()) === Date.parse(x.toUTCString()) === Date.parse(x.toISOString()) === x.valueOf()`; (ii) ISO forms unchanged: `Date.parse("2011-08-04")` is UTC midnight, `"2011-08-04T00:00:00"` is local; (iii) out-of-bounds ISO-shaped strings stay `NaN` (`"2000-13-01"`, `"2000-01-32"`, `"2000-01-01T24:00:01Z"`, `"2000-01-01T25:00"`, `"1997-3-8T11:19:20"`, `"1997-03-08 11"`); (iv) unrecognizable -> `NaN`. This test is green on jsse today (a guard, not a red step): run it before slice 1 to confirm, and again after slices 1–3 to prove the fallback did not perturb spec-mandated behavior.
5. **End-to-end confirmation (no new file).** Re-run the JetStream workload and the collector: `uv run python scripts/run-jetstream.py --test validatorjs --iterations 1 --timeout 120 --engine target/release/jsse --jetstream /tmp/JetStream --no-idle-gate` must pass (expect `count=10114`, 0 failures); also with `--bytecode`. Collector recipe (used above): copy `validatorjs/dist/bundle.es6.js`, replace the single `if (!condition) throw new Error(\`Assertion failure: ${args}\`);` with a push into `globalThis.__fails`, append `ValidatorJSBenchmark.runTest()` + a print, run on jsse and on node; expect `fails=0` on both. If new failures appear behind these six (the bundle aborts at the first, so there could be hidden ones — the collector showed none, so none are expected), triage them as separate issues.

## 6. Test surface

- **Targeted test262** (run after slice 3, release build):
  - `uv run python scripts/run-test262.py test262/test/built-ins/Date/` — includes `parse/` (`without-utc-offset.js`, `zero.js`, `year-zero.js`, `time-value-maximum-range.js`), `15.9.1.15-1.js`, `S15.9.2.1_A2.js`, `value-to-primitive-result-string.js`, `prototype/toString|toUTCString|toDateString/`.
  - `uv run python scripts/run-test262.py test262/test/staging/sm/Date/` — **must remain 56/56** (baseline today: 28 files / 56 scenarios pass). `two-digit-years.js` is now a *real* check (was vacuous); `non-iso.js` guards the ISO-shaped NaN set. Staging is not in `test262-pass.txt`, so the runner will not flag a regression automatically — read the pass count yourself.
  - `test262/test/annexB/built-ins/Date/` and `test262/test/intl402/DateTimeFormat/` (spot-check).
- **Not covered by test262, needs a new test:** the exact accepted legacy set -> `tests/date-parse-legacy-formats.js` (implementation-defined per `sec-date.parse`, so `tests/`, not `test262-extra/`); spec invariants that the fallback must not violate -> `test262-extra/Date-parse-fallback-preserves-spec-formats.js`. Run both: `uv run python scripts/run-custom-tests.py` and `uv run python scripts/run-test262.py test262-extra/Date-parse-fallback-preserves-spec-formats.js`.
- **Full gates:** `cargo test --release` (separate command), `./scripts/lint.sh`, full `uv run python scripts/run-test262.py` (use a snapshot copy of the binary if rebuilding meanwhile).
- Optional cross-check: `./scripts/run-library-tests.sh moment` before and after (moment falls back to `new Date(string)` for non-ISO input; #311 currently records 198 failing assertions — expect unchanged or fewer, never more). Skip if it exceeds the time budget and say so in the PR.

## 7. Regression risk

- **`test262-pass.txt` baseline:** should not move; the change only affects strings that are `NaN` today and can only turn `NaN` into a value for the two shapes above. Any test that asserts `NaN` for a slash/space/comma/letter-only string would regress — the grep of `test262/test/{built-ins/Date,annexB,language,staging}` found only `non-iso.js` (ISO-shaped, protected by the alphabet filter) and `two-digit-years.js` (covered by slices 1–3) as consumers of `Date.parse`/`new Date(str)` with non-ISO strings; re-run the grep pattern `Date\.parse\(|new Date\(['"\`]` if test262 has moved.
- **Shared machinery:** pure string->f64 helper. No touch of `eval_expr`/`exec_statement`, the property MOP, GC rooting/`gc_safepoint`, `ObjectKind`, or the bytecode path. `make_day`/`make_date`/`utc_time`/`time_clip` are reused as-is; DST/time-zone behavior rides on `utc_time` (host time zone; `tests/host_time_zone.rs` exists — keep new tests TZ-independent by comparing against the local `new Date(y, m, d)` constructor, not literals).
- **Behavioral hazard:** strings now accepted that other code may have relied on being `NaN` (luxon/moment fallbacks). Mitigation: conservative grammar; optional moment run above.
- **JetStream:** validatorjs was the only workload failing on this cause; expected runtime ~0.4 s per iteration.

## 8. Out of scope (follow-up list — file separate issues; do not bundle)

- Legacy strings with **time of day** (`8/4/2011 10:20`, `8/4/2011, 10:20:30 AM`), AM/PM, and timezone designators (`GMT`, `UTC`, `Z`, `+0100`, US zone abbreviations `EST`/`PDT`...). Would need `:`/`+`/`-` in the alphabet — a separate, carefully-bounded slice.
- `Sept`, dotted (`1.2.2000`) or hyphenated (`1-2-2000`) numeric dates, parenthesized comments, and strings V8 accepts such as `"1/1/"` (deliberately NaN here).
- Refactoring `parse_tostring_format`/`parse_utcstring_format` to share a month table, splitting `helpers.rs` (3.1k lines) into a `date_parse` module.
- `Temporal` has its own ISO-only `parse_date_string` (`src/interpreter/builtins/temporal/plain_date.rs:1990`); it is **not** routed through the new fallback and must not be touched.
- `docs/perf/` reports and any JetStream driver/harness changes; `test262-pass.txt` updates (`--update-baseline` is a `main` operation).
- PR title (squash subject): `fix(date): parse legacy M/D/YYYY and written-month date strings in Date.parse`.
