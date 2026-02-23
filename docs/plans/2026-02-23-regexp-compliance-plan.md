# RegExp Compliance Fixes Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix ~184 failing test262 RegExp scenarios across `built-ins/RegExp/`, `language/literals/regexp/`, and `annexB/built-ins/RegExp/`.

**Architecture:** The RegExp subsystem has four layers — lexer (tokenize `/pattern/flags`), validator (`validate_js_pattern` for static checks), translator (`translate_js_pattern_ex` to convert JS→Rust regex syntax), and executor (`regex_captures_at` via `fancy-regex`/`regex` crate). Fixes touch all four layers.

**Tech Stack:** Rust, `fancy-regex`/`regex` crates, test262 suite, Python test runner (`scripts/run-test262.py`)

**Testing:** Run `uv run python scripts/run-test262.py <path> -j $(nproc)` for targeted tests. Always build release first: `cargo build --release`. Always use `-j $(nproc)` for max parallelism.

---

### Task 1: LS/PS rejection in regex literals (~38 scenarios)

**Files:**
- Modify: `src/lexer.rs:910-953` (`lex_regex`)

**Step 1: Verify baseline**

Run: `uv run python scripts/run-test262.py test262/test/language/literals/regexp/ -j $(nproc)`
Record the pass count (expect ~378/476).

**Step 2: Fix the lexer**

In `lex_regex()` at line 915, the match arm for line terminators only includes `\n` and `\r`. U+2028 (Line Separator) and U+2029 (Paragraph Separator) are also line terminators per §12.3. They must terminate regex scanning with "Unterminated regular expression".

Change:
```rust
None | Some('\n') | Some('\r') => {
```
To:
```rust
None | Some('\n') | Some('\r') | Some('\u{2028}') | Some('\u{2029}') => {
```

Also, the backslash escape handler (line 933-937) accepts any character after `\`, including line terminators. After consuming `\`, check if the next char is a line terminator and reject:

Change:
```rust
Some('\\') => {
    pattern.push(self.advance().unwrap());
    if let Some(_c) = self.peek() {
        pattern.push(self.advance().unwrap());
    }
}
```
To:
```rust
Some('\\') => {
    pattern.push(self.advance().unwrap());
    match self.peek() {
        None | Some('\n') | Some('\r') | Some('\u{2028}') | Some('\u{2029}') => {
            return Err(LexError {
                message: "Unterminated regular expression".to_string(),
                location: self.location(),
            });
        }
        Some(_) => {
            pattern.push(self.advance().unwrap());
        }
    }
}
```

**Step 3: Build and test**

Run: `cargo build --release && uv run python scripts/run-test262.py test262/test/language/literals/regexp/ -j $(nproc)`
Expected: ~38 new passes (most `S7.8.5_A1.5_T*`, `S7.8.5_A2.5_T*`, `regexp-*-no-*-separator`, `7.8.5-1` tests).

**Step 4: Run full RegExp suite to check for regressions**

Run: `uv run python scripts/run-test262.py test262/test/built-ins/RegExp/ -j $(nproc)`
Expected: No regressions from baseline (3616 pass).

**Step 5: Commit**

```bash
git add src/lexer.rs
git commit -m "Reject LS/PS line terminators in regex literals per §12.3"
```

---

### Task 2: `/u` flag pattern validation — escape sequences (~76 scenarios, part 1)

**Files:**
- Modify: `src/interpreter/builtins/regexp.rs:2488-2940` (`validate_js_pattern`)

This is the largest fix. Split into sub-steps.

**Step 1: Verify baseline**

Run: `uv run python scripts/run-test262.py test262/test/language/literals/regexp/ test262/test/built-ins/RegExp/property-escapes/ test262/test/built-ins/RegExp/ -j $(nproc)`
Record totals.

**Step 2: Add `/u` mode validation for incomplete `\x` escapes**

In `validate_js_pattern`, after the `\x` handling at line 2552-2556, the code accepts `\x` followed by 0 or 1 hex digits without error. In `/u` mode, `\x` must be followed by exactly 2 hex digits.

After the existing `\x` parsing block (lines 2552-2556), add a unicode-mode check:

```rust
if after_escape == 'x' && i < len && chars[i].is_ascii_hexdigit() {
    let hex_start = i;
    i += 1;
    if i < len && chars[i].is_ascii_hexdigit() {
        i += 1;
    } else if _unicode {
        return Err(format!(
            "Invalid regular expression: /{}/: Invalid escape",
            source
        ));
    }
} else if after_escape == 'x' && _unicode {
    // \x without any hex digits in unicode mode
    return Err(format!(
        "Invalid regular expression: /{}/: Invalid escape",
        source
    ));
}
```

**Step 3: Add `/u` mode validation for incomplete `\u` escapes**

The `\u` handler at lines 2557-2572 accepts partial sequences. In `/u` mode, `\u` must be followed by either exactly 4 hex digits or `{hex+}`.

After the `\u` block, add validation:
```rust
} else if after_escape == 'u' {
    if i < len && chars[i] == '{' {
        let brace_start = i;
        i += 1;
        let hex_start = i;
        while i < len && chars[i].is_ascii_hexdigit() {
            i += 1;
        }
        if i < len && chars[i] == '}' {
            if i == hex_start && _unicode {
                return Err(format!(
                    "Invalid regular expression: /{}/: Invalid escape",
                    source
                ));
            }
            // Validate code point value in unicode mode
            if _unicode {
                let hex: String = chars[hex_start..i].iter().collect();
                if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                    if cp > 0x10FFFF {
                        return Err(format!(
                            "Invalid regular expression: /{}/: Invalid escape",
                            source
                        ));
                    }
                }
            }
            i += 1;
        } else if _unicode {
            return Err(format!(
                "Invalid regular expression: /{}/: Invalid escape",
                source
            ));
        }
    } else {
        let mut count = 0;
        while count < 4 && i < len && chars[i].is_ascii_hexdigit() {
            i += 1;
            count += 1;
        }
        if _unicode && count < 4 {
            return Err(format!(
                "Invalid regular expression: /{}/: Invalid escape",
                source
            ));
        }
    }
}
```

**Step 4: Add `/u` mode validation for `\c` escapes**

The `\c` handler at line 2573 accepts any alphabetic char. In `/u` mode, `\c` without a valid control letter, or `\c` at end of pattern, must be a SyntaxError. The existing check is already correct (`is_ascii_alphabetic`), but we need to also reject `\c` NOT followed by an alpha in `/u` mode:

After line 2573-2574:
```rust
} else if after_escape == 'c' {
    if i < len && chars[i].is_ascii_alphabetic() {
        i += 1;
    } else if _unicode {
        return Err(format!(
            "Invalid regular expression: /{}/: Invalid escape",
            source
        ));
    }
}
```

**Step 5: Add `/u` mode validation for bare `{`/`}` (not part of valid quantifier)**

In `/u` mode, bare `{` and `}` that are not valid quantifiers must be rejected. In the main loop's atom handling, after quantifier parsing, add:

In the main character processing (near the end of the loop body, around line 2919):
```rust
if _unicode && (c == '{' || c == '}') {
    // Check if '{' is a valid quantifier start (already handled above)
    // If we reach here with '{', it wasn't consumed as a quantifier
    if c == '}' {
        return Err(format!(
            "Invalid regular expression: /{}/: Lone quantifier bracket",
            source
        ));
    }
    // '{' that wasn't a valid quantifier
    // ... check if it looks like {n} or {n,m}
    let mut j = i + 1;
    let mut is_quant = false;
    while j < len {
        if chars[j] == '}' { is_quant = true; break; }
        if !chars[j].is_ascii_digit() && chars[j] != ',' { break; }
        j += 1;
    }
    if !is_quant {
        return Err(format!(
            "Invalid regular expression: /{}/: Lone quantifier bracket",
            source
        ));
    }
}
```

**Step 6: Build and test**

Run: `cargo build --release && uv run python scripts/run-test262.py test262/test/language/literals/regexp/ test262/test/built-ins/RegExp/ -j $(nproc)`
Expected: Progress on `u-invalid-*` tests in `language/literals/regexp/` and `property-escapes/` validation tests.

**Step 7: Commit**

```bash
git add src/interpreter/builtins/regexp.rs
git commit -m "Add /u flag validation for escape sequences and bare brackets"
```

---

### Task 3: `/u` flag — quantifiers on assertions (~8 scenarios, shared with Fix 5)

**Files:**
- Modify: `src/interpreter/builtins/regexp.rs:2488-2940` (`validate_js_pattern`)

**Step 1: Track assertion groups in the validator**

Add a stack to track what kind of group we're in. When we see `(?=`, `(?!`, `(?<=`, `(?<!`, push an "assertion" marker. On matching `)`, pop it. After assertion closing `)`, if a quantifier follows (`*`, `+`, `?`, `{`), reject with SyntaxError.

In the group handling (around line 2727), after detecting assertion groups, track them:

```rust
// Add at top of function:
let mut assertion_close_positions: Vec<usize> = Vec::new();  // positions of ')' closing assertions
let mut group_stack: Vec<bool> = Vec::new(); // true = assertion group

// In the '(' handler, when detecting (?= (?! (?<= (?<!:
// push true to group_stack

// In the ')' handler:
// pop from group_stack, if true => record position

// In quantifier handling, check if we're right after an assertion close
```

Specifically, modify the `(` handler starting at line 2727:
- `(?:` → push `false`
- `(?=`, `(?!` → push `true`
- `(?<=`, `(?<!` → push `true`
- `(?<name>` → push `false`
- bare `(` → push `false`

Modify the `)` handler at line 2773:
```rust
if c == ')' {
    i += 1;
    group_depth = group_depth.saturating_sub(1);
    let was_assertion = group_stack.pop().unwrap_or(false);
    if was_assertion {
        // Don't set has_atom — assertions are not quantifiable
        // But also explicitly mark that the NEXT quantifier is invalid
        // Check if next char is a quantifier
        if i < len && (chars[i] == '*' || chars[i] == '+' || chars[i] == '?') {
            return Err(format!(
                "Invalid regular expression: /{}/: Nothing to repeat",
                source
            ));
        }
        if i < len && chars[i] == '{' {
            // Check if it's a valid quantifier
            let mut j = i + 1;
            while j < len && (chars[j].is_ascii_digit() || chars[j] == ',') { j += 1; }
            if j < len && chars[j] == '}' {
                return Err(format!(
                    "Invalid regular expression: /{}/: Nothing to repeat",
                    source
                ));
            }
        }
        has_atom = false;
        continue;
    }
    has_atom = true;
    continue;
}
```

**Note:** Per spec, `(?=...)` and `(?!...)` are NOT QuantifiableAssertions in non-unicode mode either (they are Assertions), but many engines allow it as a non-standard extension. In `/u` mode, the spec is strict. The fix should reject quantifiers on ALL assertion types in `/u` mode, and on lookbehinds always (since lookbehinds are newer and engines don't have legacy compat for them). Investigate test expectations to determine the correct behavior for `(?=...)?` in non-`/u` mode — some tests may expect it to pass in sloppy mode.

**Step 2: Build and test**

Run: `cargo build --release && uv run python scripts/run-test262.py test262/test/language/literals/regexp/ -j $(nproc)`
Expected: ~8 new passes for `invalid-*-lookbehind` tests and `/u` assertion quantifier tests.

**Step 3: Commit**

```bash
git add src/interpreter/builtins/regexp.rs
git commit -m "Reject quantifiers on assertion groups per spec"
```

---

### Task 4: Named group identifier characters — ZWNJ/ZWJ in regex crate (~12 scenarios)

**Files:**
- Modify: `src/interpreter/builtins/regexp.rs` (translation in `translate_js_pattern_ex` around line 1559-1607)

**Step 1: Understand the problem**

The Rust `regex`/`fancy-regex` crates only allow ASCII alphanumeric + `_` in `(?P<name>...)` group names. JS allows full Unicode `ID_Start`/`ID_Continue` including ZWNJ (U+200C) and ZWJ (U+200D). When the translator emits `(?P<π>...)`, the regex crate rejects it.

**Step 2: Implement name sanitization**

Create a name mapping: replace non-ASCII group name characters with ASCII-safe placeholders. Keep a bidirectional map from sanitized names back to original JS names.

In `translate_js_pattern_ex`, add a helper:
```rust
fn sanitize_group_name(name: &str) -> String {
    let mut result = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_alphanumeric() || c == '_' {
            result.push(c);
        } else {
            // Encode as _uXXXX_
            result.push_str(&format!("_u{:04X}_", c as u32));
        }
    }
    // Ensure it doesn't start with a digit
    if result.starts_with(|c: char| c.is_ascii_digit()) {
        result.insert(0, '_');
    }
    result
}
```

At the named group emission site (line 1605), use the sanitized name for the regex but store the original in `group_name_order`:
```rust
let sanitized = sanitize_group_name(&name);
result.push_str(&format!("(?P<{}>", sanitized));
```

In the `\k<name>` backreference handler, also sanitize the name before emitting `(?P=name)`.

In the pre-scan for duplicated names, use original JS names for dedup logic but sanitized names for regex emission.

**Step 3: Fix capture name mapping in exec results**

In `regex_captures_at` (line 3348+), when building the result, map sanitized names back to the original JS names. The `group_name_order` vector already stores original JS names, so the name→index mapping should use the original names. Check that the `names` vector from `r.capture_names()` contains sanitized names and convert them back.

Actually, a simpler approach: Don't rely on the regex crate's capture names at all for the JS `groups` object. Instead, use `group_name_order` (which stores original JS names) and the numeric capture index to build the groups object. This is already partially done — check how groups are assembled in `regexp_exec`.

**Step 4: Build and test**

Run:
```bash
cargo build --release
uv run python scripts/run-test262.py test262/test/built-ins/RegExp/named-groups/ -j $(nproc)
```
Expected: ~12 new passes (non-unicode-property-names, unicode-property-names, and match-indices variants).

**Step 5: Commit**

```bash
git add src/interpreter/builtins/regexp.rs
git commit -m "Sanitize named group identifiers for regex crate compatibility"
```

---

### Task 5: Quantified group captures not cleared per iteration (~16 scenarios)

**Files:**
- Modify: `src/interpreter/builtins/regexp.rs` (exec result assembly)

**Step 1: Understand the problem**

Test: `/(a)*/exec("aaa")` should return `["aaa", "a"]` — the last capture of group 1.
Test: `/((a+)?(b+)?(c))*/exec("aacbbbcac")` — groups inside the quantified outer group that didn't participate in the LAST iteration should be `undefined`.

The Rust regex crate returns the last match for each capture group (correct), but when a group doesn't participate in the final iteration, it may still hold its value from a previous iteration. This is a regex crate behavior difference — Rust `regex` returns `None` for groups that didn't match in the final iteration, but `fancy-regex` might differ.

Verified failure: `duplicate-names-exec.js` returns `[aabb, a, b]` instead of `[aabb, undefined, b]` — the `a` capture from group 1 persists when it shouldn't.

**Step 2: Investigate the specific pattern**

The failing pattern is likely `/(?<x>a)|(?<x>b)/` with duplicate named groups. The `a` in the result is from the first alternative's group, which should be `undefined` when the second alternative matches. Check how `dup_group_map` merges results.

Look at the exec path in `regexp_exec` where `dup_group_map` entries are resolved. The issue is likely that when multiple groups share a name (via renaming `x__1`, `x__2`), the code picks the first non-None value instead of the one from the matched alternative.

**Step 3: Fix duplicate group name resolution**

In the exec result assembly (search for `dup_group_map` usage in the exec path), the logic should: for each original name in the dup map, find the variant that actually matched in this exec (only one alternative can match), and use that value. If none matched, use `undefined`. The current code likely takes the first non-None value, which is wrong when one alternative's capture persists from a failed branch.

**Step 4: Build and test**

Run:
```bash
cargo build --release
uv run python scripts/run-test262.py test262/test/built-ins/RegExp/named-groups/duplicate-names-exec.js test262/test/built-ins/RegExp/named-groups/ -j $(nproc)
```

**Step 5: Commit**

```bash
git add src/interpreter/builtins/regexp.rs
git commit -m "Fix quantified/duplicate group capture clearing per spec"
```

---

### Task 6: Unicode case folding without `/u` flag (~6 scenarios)

**Files:**
- Modify: `src/interpreter/builtins/regexp.rs` (`translate_js_pattern_ex` line 1072-1077)

**Step 1: Understand the problem**

Currently, `translate_js_pattern_ex` unconditionally emits `(?i)` for the `/i` flag. The Rust regex crate's `(?i)` uses Unicode-aware case folding, which matches Kelvin sign (U+212A) to `k`/`K` etc. Per spec, without `/u`, case-insensitive matching should only use simple case folding (ASCII range for most practical purposes).

**Step 2: Fix case folding**

When `/i` is set but `/u` and `/v` are not, instead of emitting `(?i)`, we need to not emit the global `(?i)` and instead handle case-insensitivity manually for ASCII ranges only, OR use the regex crate's `(?i)` but with the `unicode` flag disabled.

The `regex` crate supports `(?-u)(?i)` to disable unicode-aware case folding. Check if `fancy-regex` supports the same. If so, the fix is:

```rust
if flags.contains('i') {
    if flags.contains('u') || flags.contains('v') {
        result.push_str("(?i)");  // Unicode-aware
    } else {
        result.push_str("(?-u)(?i)");  // ASCII-only case folding
    }
}
```

**Caution**: `(?-u)` disables Unicode for `.` and `\w`/`\d`/`\s` too. Need to check if this has side effects. It may be better to scope it: use `(?i)` globally but manually convert specific character ranges.

Investigate by running the regexp-modifiers tests that fail.

**Step 3: Build and test**

Run:
```bash
cargo build --release
uv run python scripts/run-test262.py test262/test/built-ins/RegExp/regexp-modifiers/ test262/test/language/literals/regexp/u-case-mapping.js -j $(nproc)
```

**Step 4: Commit**

```bash
git add src/interpreter/builtins/regexp.rs
git commit -m "Restrict case folding to ASCII without /u flag"
```

---

### Task 7: Forward references + undefined backreferences (~4 scenarios)

**Files:**
- Modify: `src/interpreter/builtins/regexp.rs` (`translate_js_pattern_ex`)

**Step 1: Understand the problem**

`/\k<a>(?<a>x)/` — the `\k<a>` appears before `(?<a>x)` and should match the empty string. Similarly, `/(a)?\1/` where group 1 didn't capture should have `\1` match empty.

In Rust regex, forward references and references to unmatched groups behave differently from JS. The translator may need to emit a conditional pattern: if the group hasn't matched yet, match empty.

**Step 2: For forward backreferences, emit `(?:)` (empty match) as a fallback**

In the backreference handler in `translate_js_pattern_ex`, when emitting `\1` or `(?P=name)`, check whether the referenced group has been seen yet. If not (forward reference), emit a zero-width match or use a conditional pattern.

The `regex` crate doesn't support conditionals, but `fancy-regex` does via `(?(group)yes|no)`. Emit: `(?(group_num)\\group_num|)` — if group matched, use backreference; else match empty.

**Step 3: Build and test**

Run:
```bash
cargo build --release
uv run python scripts/run-test262.py test262/test/built-ins/RegExp/named-groups/ -j $(nproc)
```

**Step 4: Commit**

```bash
git add src/interpreter/builtins/regexp.rs
git commit -m "Handle forward references and undefined backrefs per spec"
```

---

### Task 8: Annex B RegExp legacy + minor fixes (~8 scenarios)

**Files:**
- Modify: `src/interpreter/builtins/regexp.rs`

**Step 1: Fix Annex B control escape in character class ranges**

In non-unicode mode, `[\c0]` should be handled per Annex B grammar. Investigate the specific failing test `RegExp-invalid-control-escape-character-class-range` to determine expected behavior and fix the character class range validator.

**Step 2: Fix any remaining minor pattern validation issues**

Run the full annexB/built-ins/RegExp suite, examine remaining failures after prior fixes (excluding cross-realm tests), and apply targeted fixes.

**Step 3: Build and test**

Run:
```bash
cargo build --release
uv run python scripts/run-test262.py test262/test/annexB/built-ins/RegExp/ -j $(nproc)
```

**Step 4: Commit**

```bash
git add src/interpreter/builtins/regexp.rs
git commit -m "Fix Annex B RegExp control escape and minor validation issues"
```

---

### Task 9: Final validation and progress update

**Step 1: Run the full RegExp test suite**

```bash
cargo build --release
uv run python scripts/run-test262.py test262/test/built-ins/RegExp/ test262/test/language/literals/regexp/ test262/test/annexB/built-ins/RegExp/ -j $(nproc)
```

Record new totals. Expected improvement: ~100-150 new passes (some fixes may have lower yield than estimated due to overlapping root causes or implementation limitations).

**Step 2: Run the full test262 suite**

```bash
uv run python scripts/run-test262.py -j $(nproc)
```

Check for regressions outside RegExp areas. Record total pass count.

**Step 3: Update PLAN.md and README.md**

Update the RegExp pass rate in PLAN.md table and the overall test262 progress in README.md.

**Step 4: Commit**

```bash
git add PLAN.md README.md test262-pass.txt
git commit -m "Update test262 results: RegExp compliance fixes"
```
