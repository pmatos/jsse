# RegExp Compliance Fixes Design

## Goal

Fix ~184 failing test262 scenarios across `built-ins/RegExp/`, `language/literals/regexp/`, and `annexB/built-ins/RegExp/`.

## Current State

- `built-ins/RegExp`: 3,616/3,756 pass (96.27%), 140 failing
- `language/literals/regexp`: 378/476 pass (79.41%), 98 failing
- `annexB/built-ins/RegExp`: 106/124 pass (85.48%), 18 failing
- Total: 4,100/4,356 pass, 256 failing
- Cross-realm failures (~30) deferred to separate task
- UTF-16 code unit mode (~12) deferred (fundamental regex model change)
- Fixable subset: ~184 scenarios

## Architecture

RegExp has four layers:
1. **Lexer** (`lexer.rs:910-953`): `lex_regex()` tokenizes `/pattern/flags`
2. **Validator** (`regexp.rs:2488-2940`): `validate_js_pattern()` static pattern validation
3. **Translator** (`regexp.rs:1067+`): `translate_js_pattern_ex()` converts JS→Rust regex syntax
4. **Execution** (`regexp.rs:3348+`): `regex_captures_at()` runs compiled regex via `fancy-regex`/`regex`

## Fixes by Priority

### Fix 1: `/u` flag pattern validation (~76 scenarios)

**Files**: `regexp.rs` (`validate_js_pattern`)

The `_unicode` flag is checked but many `/u`-mode rejections are missing:
- Bare `{`/`}` outside valid quantifiers
- Invalid `\c` escapes (only `\cA`-`\cZ`/`\ca`-`\cz` allowed)
- Bare `\p`/`\P` without `{...}`
- Quantifiers on lookahead/lookbehind assertions: `(?=x)?`, `(?<=x)*`
- Incomplete `\x`/`\u` escapes (e.g. `\x1`, `\u12`)

### Fix 2: LS/PS in regex literals (~38 scenarios)

**Files**: `lexer.rs` (`lex_regex`)

Line 915 only rejects `\n`/`\r` but U+2028/U+2029 are also line terminators per spec. Add them to the termination match. Also reject `\` followed by any line terminator.

### Fix 3: Quantified group captures not cleared (~16 scenarios)

**Files**: `regexp.rs` (post-processing in `regex_captures_at` or exec path)

When a capture group inside a quantified expression doesn't participate in the final iteration, its value must be `undefined`. The underlying regex crate may handle this, but the JS-level exec result assembly may not correctly propagate `undefined` for non-participating captures in quantified groups.

### Fix 4: Named group identifier characters (~12 scenarios)

**Files**: `regexp.rs` (`parse_regexp_group_name`)

Named group names must accept `ID_Continue` characters including U+200C (ZWNJ) and U+200D (ZWJ).

### Fix 5: Lookbehind quantification rejection (~8 scenarios)

**Files**: `regexp.rs` (`validate_js_pattern`)

After a lookbehind/lookahead assertion closing `)`, check for quantifiers `*`/`+`/`?`/`{n}` and reject as SyntaxError. Assertions are not QuantifiableAssertions per spec.

### Fix 6: Unicode case folding without `/u` (~6 scenarios)

**Files**: `regexp.rs` (`translate_js_pattern_ex`)

Without `/u`, case-insensitive matching should use simple ASCII/Latin uppercase only, not full Unicode case folding. The translated `(?i)` flag needs conditioning, or case-insensitive character ranges need manual expansion for non-unicode mode.

### Fix 7: Forward refs + undefined backrefs (~4 scenarios)

**Files**: `regexp.rs` (translation)

`\k<name>` or `\1` before the referenced group should match empty string. Translator must emit an empty-match alternative or equivalent for forward/undefined backreferences.

### Fix 8: Minor fixes (~8 scenarios)

- Annex B control escape in character class ranges
- Unicode property data gaps for edge-case properties

## Approach

Fix root causes in priority order (highest impact first). Each fix is independently testable against the relevant test262 subset. Run targeted tests after each fix, then full suite at the end.

## Out of Scope

- Cross-realm (`$262.createRealm()`) — 30 scenarios, deferred to engine-wide task
- UTF-16 code unit mode for non-`/u` regex — 12 scenarios, fundamental model change
- `intl402` and `staging` tests
