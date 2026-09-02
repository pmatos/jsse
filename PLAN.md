# Plan: issue #534 — genuine U+F0000-U+F07FF code points collide with the lone-surrogate PUA encoding

## 1. Problem restated

`src/interpreter/builtins/regexp.rs` needs a Rust `&str`/`char`-based representation of
JS strings so it can hand text to the `regex` crate (and, separately, so `eval()` can
hand text to jsse's own `&str`-based lexer/parser). Since JS strings are arbitrary
UTF-16 code-unit sequences and Rust `char` cannot hold a surrogate value, both paths
remap unpaired surrogates D800-DFFF onto Plane-15 Private Use Area scalars
(U+F0000-U+F07FF, `SURROGATE_PUA_BASE`). That range is ordinary, application-assignable
Unicode scalar space — a JS string can genuinely contain a real U+F0000-U+F07FF
character (e.g. via `String.fromCodePoint`, a `\u{F0000}` escape, or a raw source
character) — so the remap is not injective: a real Plane-15 character and an encoded
lone surrogate become the identical `char`, and every place that later turns that
`char` back into UTF-16 (`regex_output_to_js_string`, `pua_code_units_to_surrogates`)
guesses wrong for the real character, corrupting it into a single lone surrogate and
losing a code unit. This reaches `RegExp` matching, `replace`/`split`/legacy statics,
and — through a second, RegExp-independent path — every string literal evaluated by
`eval()`-parsed code (and, prior to this fix, ordinary top-level literals too; see
§2/§4 Slice 1).

## 2. Spec basis

- **22.2.7.2 RegExpBuiltinExec** — captured substrings and `index` come from `S`
  (the retained UTF-16 code units), not from a derived text view.
- **22.2.6.11 RegExp.prototype `[ %Symbol.replace% ]`** and **GetSubstitution**
  (`spec/spec.html`, anchor `sec-getsubstitution` — defined under
  `String.prototype.replace`, §22.1.3.19.1, and invoked from `[ %Symbol.replace% ]`)
  — `$&`, `` $` ``/`$'`, `$1`-`$9`, `$<name>` are built from the original `S`'s code
  units and positions.
- **22.2.6.14 RegExp.prototype `[ %Symbol.split% ]`** — step "the substring of `S`
  from `p` to `q`" is a code-unit slice of `S`.
- **22.2.7.3 AdvanceStringIndex** — already implemented directly against
  `input.subject.code_units`, unaffected by this bug; kept as the model the rest of
  the fix follows.
- **22.2.3.4 Static Semantics: ParsePattern**, non-`u`/`v` branch — a pattern is
  interpreted as a sequence of individual code units, so a supplementary code point
  written into a non-Unicode pattern must be treated as two atoms, exactly as the
  Unicode-mode subject treats it as two units. The current bug (§4 Slice 4) violates
  this by treating a real Plane-15 scalar as already-split when it isn't.
- **12.9.4 String Literals, Static Semantics: SV** together with **11.1.1
  UTF16EncodeCodePoint** — a `\u{F0000}` escape or literal source character denotes
  the real code point and must produce its two-code-unit UTF-16 encoding, not a
  collapsed lone surrogate.
- **19.2.1.1 `eval` ( `x` )** / **PerformEval** — operates on the string value `x`;
  the code units of `x` (including any lone surrogates) are what gets parsed. jsse's
  existing PUA remap is how it survives the `&str`-based parser; that remap must be
  undone precisely at string-literal construction, not applied blindly to every
  literal regardless of origin.
- **Legacy RegExp features** (`lastMatch`, `lastParen`, `$1`-`$9`) — this is a
  separate stage-3 proposal, **not present in `spec/spec.html`**
  (`sec-additional-properties-of-the-regexp.prototype-object` there is the
  *prototype*'s `compile`, not the constructor statics). The existing code cites
  "B.2.4" by convention (see `src/interpreter/builtins/regexp.rs:8169` and the
  legacy-accessor setup ~line 10580); this plan follows that same convention rather
  than inventing a clause number. Same code-unit-fidelity requirement as
  RegExpBuiltinExec applies regardless of which document defines the property.

No new JavaScript syntax or semantics is introduced; this is entirely an internal
representation fix to already-specified behavior.

## 3. Files to touch

- `src/interpreter/builtins/regexp.rs` — the bulk of the fix (see slices 2-5 below).
- `src/lexer.rs` — lex-time surrogate-undo for eval-sourced text, scoped by a flag
  (Slice 1); make `pua_to_surrogate` `pub(crate)` for the lexer to call it.
- `src/parser/mod.rs` — add an eval-aware constructor that threads the flag into the
  `Lexer`.
- `src/interpreter/eval/literals.rs` — remove the blanket, unconditional
  `pua_code_units_to_surrogates` call on every `Literal::String` (Slice 1); revisit
  `Literal::RegExp`'s `regex_output_to_js_string(pattern)` once Slice 2's decode
  helper exists (Slice 3).
- `src/interpreter/eval.rs` (`perform_eval`) and `src/interpreter/mod.rs`
  (`$262.evalScript`, ~line 762) — switch to the eval-aware parser constructor.
- `test262-extra/` — new regression files (see §5).
- No `docs/adr/` or `CONTEXT.md` changes: this reuses the offset-table machinery
  #532 already built (no new architectural seam), and introduces no new domain
  vocabulary.

## 4. TDD slices

Each slice is independently green (`cargo build --bin jsse`, `cargo test --bin jsse`,
targeted test262 directories) before moving to the next. Order follows increasing
entanglement with the PUA-`String` accumulator pattern, per the advisor review of
this plan.

### Slice 1 — string literals (`eval_literal` / lexer)

**Red:** `test262-extra/string-literal-supplementary-pua.js` — asserts
`"\u{F0000}".length === 2` and `.codePointAt(0) === 0xF0000` for both a `\u{F0000}`
escape and a raw source character; currently jsse reports `length === 1`. This
exercises the *plain-parsing* half of the bug (`eval_literal`'s blanket undo). It
does **not** exercise the `pua_undo` lexer flag added below — `eval('"\\u{F0000}"')`
would look like a natural second case, but its eval source is 13 plain ASCII chars,
so `js_string_to_regex_input` (ASCII → identity) never invokes `surrogate_to_pua`,
and the case collapses to the exact same plain-literal bug. The case that actually
exercises the flag is `eval('"' + String.fromCharCode(0xD800) + '"')` — a *raw* lone
surrogate already present in the eval source's code units, which is the only input
that reaches `surrogate_to_pua` before lexing (`perform_eval`'s
`js_string_to_regex_input` conversion). Expected: `.length === 1`,
`.charCodeAt(0) === 0xD800`. Without `pua_undo`, step 5 below (deleting the blanket
`eval_literal` undo) would turn this into a *regression* — the raw 0xD800 would
survive `js_string_to_regex_input` as PUA char U+F0000, get lexed back into
`[0xDB80, 0xDC00]`, and nothing would undo it. Add both cases to the test file: the
plain-parsing case (proves the fix) and the eval-with-raw-surrogate case (proves the
flag prevents the regression the fix would otherwise introduce).

**Root cause:** `src/lexer.rs`'s `read_string`/`read_string_escape_into` already build
correct `Vec<u16>` code units for ordinary parsing (confirmed by reading; no PUA
anywhere in that path). The corruption is `eval_literal`'s
`Literal::String(s) => pua_code_units_to_surrogates(s)` in
`src/interpreter/eval/literals.rs:93-95`, which unconditionally reinterprets *every*
literal's already-correct code units as if they might be PUA round-trip artifacts —
collapsing a genuine `[0xDB80, 0xDC00]` (real U+F0000) into a lone `0xD800`. This call
is only meaningful for text that passed through `perform_eval`'s
`js_string_to_regex_input` source conversion (`src/interpreter/eval.rs:6203`), which
PUA-encodes lone surrogates so the `&str`-based parser can carry them — it must not
run on normally-parsed source.

**Green:**
1. Add `pua_undo: bool` to `Lexer` (`src/lexer.rs`), default `false` in `new()`; add
   `Lexer::new_for_eval(source: &'a str) -> Self` setting it `true`.
2. In `read_string`'s raw-character arm (the `Some(ch) => { ch.encode_utf16(...) }`
   catch-all, ~line 414-419) and nowhere else: when `pua_undo` is set, check
   `pua_to_surrogate(ch)` first and push the recovered surrogate code unit instead of
   `encode_utf16`. This must be per-character, not a post-pass over the finished
   `Vec<u16>` — a post-pass would also re-collapse a *correctly*-escaped
   `\u{F0000}` that `read_string_unicode_escape_into` already turned into
   `[0xDB80, 0xDC00]` via the ordinary (non-PUA) escape path (`src/lexer.rs:528-539`).
   Make `pua_to_surrogate` `pub(crate)` in `regexp.rs` for this call.
3. Add `Parser::new_for_eval(source: &'a str) -> Result<Self, ParseError>`
   (`src/parser/mod.rs`) constructing the lexer via `Lexer::new_for_eval`.
4. Switch `perform_eval` (`src/interpreter/eval.rs:6203` area) to call
   `Parser::new_for_eval`. Also switch `$262.evalScript`
   (`src/interpreter/mod.rs:762`) for consistency — it shares the same conversion
   call — but note it is test262-harness-only; no test262 test observes lone
   surrogates through `$262.evalScript` itself, so this is a consistency fix, not a
   red case, and the implementation stage should not invent one.
5. Delete the `pua_code_units_to_surrogates` call in `eval_literal`'s
   `Literal::String` arm (`src/interpreter/eval/literals.rs:92-96`) — just wrap `s`.
   This also fixes `src/interpreter/bytecode/compiler.rs:474`
   (`Literal::String(units) => ...`), which already skips the call and was silently
   diverging from the tree-walker for eval-sourced lone surrogates; after this slice
   both interpreters read the same, already-correct, AST-level code units.
6. Once nothing calls it, delete `pua_code_units_to_surrogates` from `regexp.rs`
   (clippy `-D warnings` will flag it as dead otherwise — confirm via `cargo build`
   after the deletion).

**Known residual gap (not fixed by this slice, follow-up):** `eval()` given a string
whose *raw* (non-escaped) code units already contain a genuine Plane-15 character
still collides with `js_string_to_regex_input`'s encode step, because encoding is
consistently non-injective regardless of direction. Document in the PR description;
no test262 coverage currently exercises it (checked: no existing eval+surrogate
test262-extra file).

### Slice 2 — `split()`

**Red:** extend `test262-extra/RegExp-replace-supplementary-pua-subject.js`-style
assertions (new file, see §5) — `pua.split(/x/)[0]` should be the 2-code-unit `pua`,
not `[0xD800]`.

**Green:** in the `[Symbol.split]` implementation (`src/interpreter/builtins/regexp.rs`,
~lines 9628-9687), `p` and `q` are already UTF-16 code-unit offsets *before* being
converted to byte offsets for slicing the PUA `&str` view (`p_byte`/`q_byte` via
`utf16_to_byte_offset`). Skip the byte round-trip entirely: push
`JsString::from_vec(regex_input.subject.code_units[p..q].to_vec())` (and the
`[p..]` tail case) directly. No offset-table lookup even needed here since `p`/`q`
are already in the right unit.

### Slice 3 — `exec`/`match`, indices, groups, Annex B legacy statics

**Red:** extend the same new test262-extra file — `pua.match(/./u)[0]`,
`pua.exec(pua)[0]`, `RegExp.lastMatch`/`RegExp.$1` after a match against `pua`.

**Green:**
1. Add a decode helper on `RegexInput` (`src/interpreter/builtins/regexp.rs`, near
   `byte_offset_to_utf16`):
   ```rust
   fn subject_slice(&self, view: RegexView, byte_start: usize, byte_end: usize) -> JsString {
       let start = self.byte_offset_to_utf16(view, byte_start);
       let end = self.byte_offset_to_utf16(view, byte_end);
       JsString::from_vec(self.subject.code_units[start..end].to_vec())
   }
   ```
   This is injective and view-agnostic (it already dispatches through
   `byte_offset_to_utf16`'s `Unicode`/`NonUnicode`/`Wtf8` match arms, all three built
   from the retained code units, never from a decoded view) — it supersedes
   `regex_output_to_js_string` for every call site that has `regex_input`/`view` and a
   byte-offset span, which is every remaining site except the replace accumulator
   (Slice 5).
2. Replace `regex_output_to_js_string(&full_match.text)` /
   `regex_output_to_js_string(&m.text)` at the match-array and `indices`/`groups`
   construction (~lines 8151-8300) with `regex_input.subject_slice(view, m.start, m.end)`.
3. Change `RegexpLegacyState.last_match`/`last_paren`/`parens` from `String` to
   `JsString`; write them via `subject_slice` (~lines 8175-8186) instead of
   `full_match.text.clone()`/`m.text.clone()`; simplify the three getters
   (~lines 10599-10600, 10680-10682, 10712-10714) to clone the stored `JsString`
   directly instead of `JsString::from_str(&legacy.last_match)` — this incidentally
   fixes a pre-existing, closely related bug where these getters use plain
   `JsString::from_str` (not even the old `regex_output_to_js_string`) and so were
   already wrong for *any* lone surrogate in a capture, not just Plane-15 collisions.
4. Fix the fast-path `@@replace` loop's `match_length_utf16` bug at line ~9021
   (`regex_output_to_js_string(matched).code_units.len()`, which returns 1 for a
   2-code-unit genuine match): replace with `match_end_utf16 - match_start_utf16`.
5. Fix `[Symbol.match]`'s global-flag fast path (~lines 8636-8700, the pristine-RegExp
   loop reached when `flags` contains `g`): `results.push(JsValue::string(
   regex_output_to_js_string(match_text)))` at line 8691 becomes
   `regex_input.subject_slice(view, full_match.start, full_match.end)`, and the
   `match_text.is_empty()` check at line 8693 becomes
   `full_match.start == full_match.end` (same pattern as the `matched.is_empty()` fix
   in Slice 5, but this site is a simple substring extraction with no accumulator
   entanglement, so it belongs here rather than waiting for Slice 5). Add
   `pua.match(/./g)` (global, non-sticky, no accumulator) to the Slice 3 red test to
   cover this — it was found only by grepping every `\.text\b` in `regexp.rs` and
   checking each hit against the slices above; do that grep again after
   implementing to confirm no other site was missed.
6. `matchAll`'s `%RegExpStringIteratorPrototype%.next` delegates to
   `regexp_exec_abstract` for the live path (confirmed by reading ~9919-10016) and
   inherits this fix for free; its `Some(mid)`-absent "legacy path" fallback
   (~10018-10113, uses plain `JsString::from_str(&full.text)`, no PUA-awareness at
   all) is pre-existing and separately broken — out of scope (§7).

### Slice 4 — pattern-side mode-aware encoding

**Red:** `new RegExp(pua).test(pua)` (non-`u`) must return `true`, matching Node —
add to the new test262-extra file.

**Root cause (confirmed by reading `src/interpreter/builtins/regexp.rs:3454` and the
constructor at ~8480-8523):** every pattern-source-to-Rust-`String` conversion site
(lines 7679, 7826, 8069, 8490, 8513, 8642, 8954, 10367, 10388, 10409) calls
`js_string_to_regex_input` — the *Unicode* encoder — unconditionally, regardless of
the regex's actual flags. For a non-`u`/`v` regex, this pre-combines a genuine
Plane-15 surrogate pair in the pattern source into one `char`. The subsequent
non-Unicode translation pass's astral-splitting guard,
`if !unicode && c as u32 >= 0x10000 && pua_to_surrogate(c).is_none()`, is supposed to
split any astral pattern atom into two PUA-encoded surrogate halves (matching how the
non-Unicode subject view represents it) — but `pua_to_surrogate(c).is_none()` treats
anything already in the PUA target range as "already a lone-surrogate encoding" and
skips the split, so a real U+F0000 pattern atom stays a single char while the subject
is two. This is the same non-injectivity as the subject side, now hitting the
*compiler* instead of the decoder.

**Green:** at each pattern-materialization call site, determine the effective `u`/`v`
flag before encoding the pattern (most sites already compute `flags_str`/`unicode`
adjacent to the pattern conversion — reorder where needed, e.g. constructor
~8490/8513 already has `r.flags`/`flags_arg` available), and call
`js_string_to_regex_input_non_unicode` instead of `js_string_to_regex_input` when the
regex is non-`u`/`v`. This pre-splits a genuine surrogate pair into two individually
PUA-encoded halves before the translation pass ever sees a combined astral `char`, so
line 3454's guard no longer needs to (and cannot correctly) distinguish "real Plane-15
atom" from "already-split lone surrogate" — both now arrive pre-split, consistent
with how `js_string_to_regex_input_non_unicode` already encodes the non-Unicode
subject view. Verify the `\u{...}` pattern-escape-syntax handling elsewhere in the
translation pass is unaffected (it operates on regex *source syntax* characters, a
different concern from raw literal astral atoms) — cover with the TDD red case plus
existing `built-ins/RegExp/` regression runs.

**Documented, not fixed, in this slice:** in `u`/`v` mode, a pattern atom that is
itself a lone-surrogate `CharacterValue` (e.g. `\uD800` written directly in a `/u`
pattern — valid per the regex grammar's `RegExpUnicodeEscapeSequence`) still encodes
to the same PUA `char` as a genuine Plane-15 subject scalar, producing a **false
positive** match (`/\uD800/u.test(String.fromCodePoint(0xF0000))` wrongly `true`).
This is a match-*semantics* collision, not a decode-offset one, and cannot be closed
by the offset-table technique used elsewhere in this plan — it needs the WTF-8/
bytes-mode route (#37's precedent) or a side table. Out of scope for this issue (§7);
the issue's own 44-row jsse-vs-Node comparison does not exercise this combination.

### Slice 5 — `replace()`: `get_substitution` and both accumulator loops

**Red:** the remaining rows of the new test262-extra file —
`pua.replace(/\u{F0000}/gu, "z")` behavior, plus `$&`/`` $` ``/`$'`/`$1` substitutions
and functional replacers exercised against a subject containing a genuine Plane-15
character interleaved with ordinary text.

**Root cause (confirmed by reading lines 7866-7998 and both call sites ~8944-9148 and
~9357-9447):** `get_substitution` and its two callers (the fast pristine-RegExp path
and the generic/functional path) deliberately keep the *entire* replacement assembly
— unmatched subject slices, `` $` ``/`$'` context, capture-group text, and even the
user-supplied literal replacement template — in "PUA-mapped regex-output space" as a
single Rust `String` (`accumulated_result`), decoding once at the very end via
`regex_output_to_js_string(&accumulated_result)`. Once pieces are concatenated, their
origin (real subject text vs. re-encoded lone surrogate) is unrecoverable — this is
the one place the offset-table trick from Slices 2-3 cannot apply, because the final
byte range no longer corresponds to a contiguous subject span.

**Green:** rewrite `get_substitution` to operate on `&[u16]`/`Vec<u16>` throughout
instead of `&str`/`String`:
- Signature becomes `(matched: &[u16], s: &[u16], position: usize, tail_pos: usize,
  captures: &[JsValue], named_captures: &JsValue, replacement: &[u16]) ->
  Result<Vec<u16>, JsValue>`. `position`/`tail_pos` are already UTF-16 offsets
  upstream (`position_utf16`, computed before the current byte-offset detour) —
  passing them straight through removes a layer of conversion rather than adding one.
- Scan `replacement` for the ASCII `$`-syntax using `u16` comparisons (`'$' as u16`,
  `.is_ascii_digit()` equivalent on `u16`) instead of `char`s; `$&` pushes
  `matched` directly; `` $` ``/`$'` push `s[..position]`/`s[tail_pos..]` directly;
  `$1`-`$9`/`$<name>` push the capture's `JsString.code_units` directly (no
  `js_string_to_regex_input_non_unicode` re-encoding — that round trip disappears
  entirely, lines 7920-7922, 7932-7934, 7975-7977).
- At both call sites, change `accumulated_result` from `String` to `Vec<u16>`;
  replace `s_slice[a..b]`-style `push_str` of unmatched subject spans with
  `regex_input.subject.code_units[a_utf16..b_utf16].to_vec()` extends (offsets already
  computed as UTF-16 in most spots; drop the `utf16_to_byte_offset(NonUnicode, ...)`
  detours and `.min(length_s)` byte clamps in favor of UTF-16-space clamps against
  `s_utf16_len`).
- The functional-replace return-value path (~9377-9379) drops its
  `js_string_to_regex_input_non_unicode(&s.code_units)` re-encoding and pushes
  `s.code_units` straight into the accumulator.
- Final result: `JsString::from_vec(accumulated_result)` — no
  `regex_output_to_js_string` call at all (lines 9145-9147, 9445-9447 deleted).
- `regex_output_to_js_string` survives this slice with exactly one remaining caller:
  `literals.rs:121` (`Literal::RegExp` source decode, for `RegExp.prototype.source`
  on a regex literal). One caller is not dead code, so **do not delete the function**
  in this PR — leave it, and file the `Literal::RegExp` migration (regex-literal
  `.source` for a pattern containing a raw Plane-15 char has the same latent bug
  Slice 1 fixed for string literals, needing the same kind of AST-level plumbing
  change) as a fast-follow rather than expanding this slice.
- `RegexMatch.text: String` (and its `wtf8_slice_to_pua_string` construction at
  ~line 7667 for the `Wtf8` view) becomes genuinely unread once Slices 2-5 land —
  confirmed by enumerating every `\.text\b` in `regexp.rs` against the slices above
  (Slice 3 step 5 above closes the one site — the `[Symbol.match]` global fast path
  at line 8687/8691/8693 — that an earlier pass of this plan missed). Grep
  `\.text\b` again after implementing Slice 5 to confirm nothing was missed before
  relying on `cargo build`'s dead-code warning as the safety net; then remove the
  field and its construction.

## 5. Test surface

**test262 (targeted, run via `uv run python scripts/run-test262.py <dir>`):**
- `test262/test/built-ins/RegExp/prototype/exec/`
- `test262/test/built-ins/RegExp/prototype/Symbol.replace/`
- `test262/test/built-ins/RegExp/prototype/Symbol.split/`
- `test262/test/built-ins/RegExp/prototype/Symbol.match/`
- `test262/test/built-ins/RegExp/prototype/Symbol.matchAll/`
- `test262/test/built-ins/String/prototype/replace/`
- `test262/test/built-ins/String/prototype/split/`
- `test262/test/built-ins/String/prototype/match/`
- `test262/test/annexB/built-ins/RegExp/legacy-accessors/` (if present under that
  path; confirm actual directory name — it may be under
  `annexB/built-ins/RegExp/` more broadly)
- `test262/test/built-ins/RegExp/property-escapes/generated/` — exercises the
  `Wtf8` view through the same `subject_slice` decode path (Slice 3), a regression
  check that the `\p{Cs}`/`\p{Co}` byte-level matching from #37 still agrees.
- `test262/test/language/literals/string/`, `test262/test/language/literals/regexp/`
- `test262/test/built-ins/eval/`, `test262/test/language/eval-code/`
- `test262/test/language/source-text/` (raw Plane-15 source characters)
- Full suite run before opening the PR, comparing against the `origin/main` baseline
  per project convention (no `--update-baseline`).

**test262-extra (new files, following `RegExp-replace-supplementary-pua-subject.js`'s
existing `esid`/`description`/`info` header convention and its
`codeUnits`/`assertSameUnits` helper pattern):**
- `test262-extra/string-literal-supplementary-pua.js` (Slice 1) — `\u{F0000}` escape
  and raw source character forms, plus an `eval('"\\u{F0000}"')` case exercising the
  lexer flag.
- `test262-extra/RegExp-match-supplementary-pua-subject.js` (Slices 2-3) —
  `match`/`exec`/`split`/`matchAll`/indices/groups/legacy statics
  (`lastMatch`/`$1`-`$9`) against a subject containing a genuine Plane-15 character,
  mirroring the existing offset-side file's structure but asserting the *text* of
  results rather than positions.
- `test262-extra/RegExp-pattern-supplementary-pua.js` (Slice 4) — a pattern
  containing the code point matches it, in both `u` and non-`u` modes.
- Extend `test262-extra/RegExp-replace-supplementary-pua-subject.js` in place, or add
  `test262-extra/RegExp-replace-supplementary-pua-text.js` (Slice 5) —
  `$&`/`` $` ``/`$'`/`$1`/`$<name>`/functional-replacer text correctness (the existing
  file only checks that *unmatched* portions survive; Slice 5 additionally needs the
  *matched/substituted* text itself checked, e.g. `pua.replace(/\u{F0000}/gu, "z")`).

**Non-engine gates:** none — this is entirely `src/` engine behavior;
`cargo test --bin jsse` covers unit-level regressions, `./scripts/lint.sh` covers the
clippy dead-code sequencing called out per slice.

## 6. Regression risk

- **`test262-pass.txt` baseline:** every slice is a strict correctness fix (wrong
  code units → right code units) with no observable behavior change for the
  overwhelmingly common case (subjects/patterns/literals without lone surrogates or
  Plane-15 scalars), so no currently-passing test should regress. The one place with
  real blast radius is Slice 5's `get_substitution` rewrite, since it touches every
  `String.prototype.replace`/`RegExp.prototype[@@replace]` call, including the
  "fast pristine-RegExp path" hot path — run the full replace/split/match test262
  directories plus `cargo test --bin jsse` after this slice specifically, before
  moving on.
- **Tree-walker vs. bytecode divergence:** Slice 1 fixes both interpreters at once by
  moving the fix to AST construction (`Literal::String`'s code units are correct
  before either interpreter sees them) — verify with a bytecode-enabled build
  (`--features bytecode_enabled` or however the project's existing bytecode test
  matrix is invoked) that eval-sourced lone-surrogate literals now agree between the
  two paths, where they silently diverged before.
- **Property MOP / GC:** none of these slices touch `property.rs`, object shapes, or
  GC rooting — `RegexInput`, `RegexMatch`, and the accumulator are all
  stack-local/short-lived, no new roots.
- **`RegexView::Wtf8` (the `\p{Cs}`/`\p{Co}` path from #37):** Slice 3's
  `subject_slice` helper is exercised by this view too (it already dispatches through
  `byte_offset_to_utf16`'s `Wtf8` arm) — run
  `built-ins/RegExp/property-escapes/generated/` to confirm no regression, since this
  is the one view the fix wasn't originally designed against.
- **Sequencing/dead-code:** `pua_code_units_to_surrogates` dies with Slice 1;
  `regex_output_to_js_string` and `RegexMatch.text` die with Slice 5 (modulo the
  `Literal::RegExp` follow-up noted there). The repo's Edit/Write hook runs
  `clippy -D warnings` — build after each slice, not just at the end, so a dead-code
  failure is attributed to the slice that caused it.
- **Regex cache correctness:** Slice 4 changes what Rust `String` a pattern encodes
  to for non-`u`/`v` regexes; `build_regex_cached`'s cache key is the encoded source
  string itself, so a changed encoding naturally produces a different (correct) cache
  entry rather than a stale hit — no separate cache-invalidation work needed, but
  worth a explicit look at the cache-key construction while implementing to confirm.

## 7. Out of scope

- **The u-mode lone-surrogate-vs-Plane-15-scalar false positive** described at the
  end of Slice 4 (`/\uD800/u.test(String.fromCodePoint(0xF0000))`) — a genuine
  match-semantics collision, not a decode-offset bug; needs the WTF-8/bytes-mode
  route (#37 precedent) or a side table for the *entire* Unicode/NonUnicode matching
  path, which the issue itself calls "a different and larger change." File as a
  follow-up issue referencing this plan's Slice 4 notes.
- **`eval()` with a raw (non-escaped) Plane-15 character already present in the
  source string's code units** (Slice 1's residual gap) — same class of gap as
  above, applied to the eval-source pipeline instead of the regex-subject pipeline.
  Follow-up.
- **`matchAll`'s legacy fallback iterator path** (`%RegExpStringIteratorPrototype%.next`
  when no `__matcher__` is present, ~lines 10018-10113) — pre-existing, already
  broken for lone surrogates independent of this issue (uses plain
  `JsString::from_str`, no PUA-awareness at all). Not touched.
- **`Literal::RegExp`'s `.source` decode** (`literals.rs:121`,
  `regex_output_to_js_string(pattern)`) for a regex literal whose pattern text
  contains a raw Plane-15 character — noted as a fast-follow in Slice 5, not bundled.
- **No refactor of `RegexInput`/`RegexView`/the boundary-sampling machinery itself**
  — Slices 2-3 are pure reuse of what #532 already built; no changes to
  `BOUNDARY_SAMPLE_INTERVAL`, `unicode_boundaries()`, or `non_unicode_offsets()`.
- **No `test262-pass.txt` baseline update** — left to `main`, per project convention.
