use super::regexp_lookbehind::{
    LbAtom, LbFlags, LookbehindInfo, extract_lookbehinds, parse_lb_atoms,
};

/// ES lookbehinds match right-to-left and never backtrack into their captures.
/// fancy-regex 0.20 matches variable-length hard lookbehinds forward, so a
/// lookbehind capture of unbounded width read by a backref outside it
/// (`/(?<=([abc]+)).\1/`) must go to the custom RTL matcher.
fn unbounded_capture_read_outside(source: &str) -> bool {
    super::regexp::lookbehind_captures_with_external_backrefs(source).is_some()
        && extract_lookbehinds(source)
            .0
            .iter()
            .any(|lb| lb.content.contains(['+', '*', '{', '|']))
}

/// Whether the pattern needs the custom right-to-left lookbehind matcher.
pub(super) fn needs_custom_rtl(source: &str, flags: &str) -> bool {
    legacy_needs_custom_rtl(source)
        || unbounded_capture_read_outside(source)
        || variable_width_capture(source, flags)
}

/// A positive lookbehind with a capture whose width varies (or whose capture
/// sits in a counted loop) reports the right-to-left choice of capture, which
/// a forward matcher cannot reproduce.
fn variable_width_capture(source: &str, flags: &str) -> bool {
    if flags.contains('v') {
        return false;
    }
    let lb_flags = LbFlags::new(flags, 0);
    extract_lookbehinds(source).0.iter().any(|lb| {
        if !lb.positive || !supported(&lb.content) {
            return false;
        }
        let atoms = parse_lb_atoms(&lb.content, &lb_flags, 0);
        has_capture(&atoms) && width(&atoms).is_none()
    })
}

/// Escapes and group forms the custom matcher's parser does not model.
fn supported(content: &str) -> bool {
    let chars: Vec<char> = content.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                if matches!(chars.get(i + 1), Some('p' | 'P' | 'c')) {
                    return false;
                }
                i += 1;
            }
            '(' if chars.get(i + 1) == Some(&'?')
                && !matches!(chars.get(i + 2), Some(':' | '=' | '!' | '<')) =>
            {
                return false;
            }
            _ => {}
        }
        i += 1;
    }
    true
}

fn has_capture(atoms: &[LbAtom]) -> bool {
    atoms.iter().any(|a| match a {
        LbAtom::CaptureGroup { .. } => true,
        LbAtom::NonCaptureGroup { content } | LbAtom::Lookahead { content, .. } => {
            has_capture(content)
        }
        LbAtom::Alternation(alts) => alts.iter().any(|alt| has_capture(alt)),
        LbAtom::Quantified { atom, .. } => has_capture(std::slice::from_ref(atom)),
        _ => false,
    })
}

/// The fixed number of characters the atoms consume, or `None` when it varies
/// or a capture inside a counted loop repeats (its last iteration depends on
/// the direction).
fn width(atoms: &[LbAtom]) -> Option<usize> {
    let mut total = 0;
    for a in atoms {
        total += match a {
            LbAtom::Literal(_) | LbAtom::CharClass(..) | LbAtom::Dot(_) => 1,
            LbAtom::CaptureGroup { content, .. } | LbAtom::NonCaptureGroup { content } => {
                width(content)?
            }
            LbAtom::Anchor(_) | LbAtom::Lookahead { .. } => 0,
            LbAtom::Backref(_) => return None,
            LbAtom::Alternation(alts) => {
                let first = width(alts.first()?)?;
                for alt in &alts[1..] {
                    if width(alt)? != first {
                        return None;
                    }
                }
                first
            }
            LbAtom::Quantified { atom, min, max, .. } => {
                let one = std::slice::from_ref(&**atom);
                let w = width(one)?;
                if *max != Some(*min) || (*min > 1 && has_capture(one)) {
                    return None;
                }
                w * *min as usize
            }
        };
    }
    Some(total)
}

/// Rewrites `\k<name>` to `\N` when the name is unique and the reference or
/// its group is inside a lookbehind, so the lookbehind matcher, which follows
/// numbered backreferences, resolves it.
pub(super) fn named_backrefs_to_numeric(source: &str) -> String {
    if !source.contains("\\k<") {
        return source.to_string();
    }
    #[derive(PartialEq)]
    enum Kind {
        Capture,
        Plain,
        Behind,
    }
    let chars: Vec<char> = source.chars().collect();
    let mut names: Vec<(String, u32, bool)> = Vec::new();
    let mut refs: Vec<(usize, usize, String, bool)> = Vec::new();
    let mut stack: Vec<Kind> = Vec::new();
    let mut count = 0u32;
    let mut in_cc = false;
    let mut i = 0;
    while i < chars.len() {
        let in_lb = stack.contains(&Kind::Behind);
        match chars[i] {
            '\\' => {
                if !in_cc
                    && chars.get(i + 1) == Some(&'k')
                    && chars.get(i + 2) == Some(&'<')
                    && let Some(len) = chars[i + 3..].iter().position(|&c| c == '>')
                {
                    let name: String = chars[i + 3..i + 3 + len].iter().collect();
                    refs.push((i, i + len + 4, name, in_lb));
                    i += len + 4;
                    continue;
                }
                i += 2;
                continue;
            }
            '[' => in_cc = true,
            ']' => in_cc = false,
            '(' if !in_cc => {
                if chars.get(i + 1) == Some(&'?') {
                    match (chars.get(i + 2), chars.get(i + 3)) {
                        (Some('<'), Some('=' | '!')) => stack.push(Kind::Behind),
                        (Some('<'), _) => {
                            count += 1;
                            let name: String =
                                chars[i + 3..].iter().take_while(|&&c| c != '>').collect();
                            names.push((name, count, in_lb));
                            stack.push(Kind::Capture);
                        }
                        _ => stack.push(Kind::Plain),
                    }
                } else {
                    count += 1;
                    stack.push(Kind::Capture);
                }
            }
            ')' if !in_cc => {
                stack.pop();
            }
            _ => {}
        }
        i += 1;
    }

    let mut out = String::with_capacity(source.len());
    let mut last = 0;
    for (start, end, name, ref_in_lb) in &refs {
        let mut named = names.iter().filter(|(n, _, _)| n == name);
        let (Some((_, num, group_in_lb)), None) = (named.next(), named.next()) else {
            continue;
        };
        if !(*ref_in_lb || *group_in_lb) {
            continue;
        }
        out.extend(&chars[last..*start]);
        if chars.get(*end).is_some_and(char::is_ascii_digit) {
            out.push_str(&format!("(?:\\{num})"));
        } else {
            out.push_str(&format!("\\{num}"));
        }
        last = *end;
    }
    out.extend(&chars[last..]);
    out
}

/// Replaces each `\N` backreference listed in `replacements` with its literal
/// text wrapped in `(?:...)`, so a following quantifier applies to the whole
/// captured text and an unset capture (empty text) stays a valid atom.
pub(super) fn substitute_backrefs(source: &str, replacements: &[(u32, String)]) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut in_cc = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            if !in_cc && chars[i + 1].is_ascii_digit() && chars[i + 1] != '0' {
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
                let num: String = chars[i + 1..j].iter().collect();
                let hit = num
                    .parse::<u32>()
                    .ok()
                    .and_then(|n| replacements.iter().find(|(k, _)| *k == n));
                if let Some((_, text)) = hit {
                    out.push_str("(?:");
                    out.push_str(text);
                    out.push(')');
                } else {
                    out.extend(&chars[i..j]);
                }
                i = j;
                continue;
            }
            out.push(c);
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }
        match c {
            '[' => in_cc = true,
            ']' => in_cc = false,
            _ => {}
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Original group number of the `rank`-th (1-based) capture outside every
/// lookbehind: the lookbehinds before it contribute their own captures.
pub(super) fn outer_orig_index(lookbehinds: &[LookbehindInfo], rank: usize) -> usize {
    let mut skipped = 0u32;
    let mut extra = 0usize;
    for lb in lookbehinds {
        let outer_before = lb.capture_offset - skipped;
        if (outer_before as usize) < rank {
            extra += lb.num_captures as usize;
        }
        skipped += lb.num_captures;
    }
    rank + extra
}

/// The lookbehind as a capture-free assertion, kept in the stripped outer
/// pattern so the forward matcher still backtracks across it; the custom
/// matcher then recomputes the captures right-to-left at the marker. `None`
/// when the content has a backreference, which needs its groups.
pub(super) fn capture_free_assertion(content: &str, positive: bool) -> Option<String> {
    let chars: Vec<char> = content.chars().collect();
    let mut out = String::with_capacity(content.len() + 8);
    out.push_str(if positive { "(?<=" } else { "(?<!" });
    let mut in_cc = false;
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                let next = *chars.get(i + 1)?;
                if !in_cc && (next == 'k' || next.is_ascii_digit() && next != '0') {
                    return None;
                }
                out.push('\\');
                out.push(next);
                i += 2;
                continue;
            }
            '[' => in_cc = true,
            ']' => in_cc = false,
            '(' if !in_cc => {
                if chars.get(i + 1) != Some(&'?') {
                    out.push_str("(?:");
                    i += 1;
                    continue;
                }
                if chars.get(i + 2) == Some(&'<') && !matches!(chars.get(i + 3), Some('=' | '!')) {
                    let close = chars[i..].iter().position(|&c| c == '>')?;
                    out.push_str("(?:");
                    i += close + 1;
                    continue;
                }
            }
            _ => {}
        }
        out.push(chars[i]);
        i += 1;
    }
    out.push(')');
    Some(out)
}

fn legacy_needs_custom_rtl(source: &str) -> bool {
    let chars: Vec<char> = source.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut in_cc = false;

    while i < len {
        if chars[i] == '[' && !in_cc {
            in_cc = true;
            i += 1;
            continue;
        }
        if chars[i] == ']' && in_cc {
            in_cc = false;
            i += 1;
            continue;
        }
        if chars[i] == '\\' && i + 1 < len {
            i += 2;
            continue;
        }

        // Detect lookbehind
        if !in_cc
            && chars[i] == '('
            && i + 3 < len
            && chars[i + 1] == '?'
            && chars[i + 2] == '<'
            && (chars[i + 3] == '=' || chars[i + 3] == '!')
        {
            let content_start = i + 4;
            let mut depth = 1;
            let mut j = content_start;
            let mut has_backref = false;
            let mut has_quantified_capture = false;
            let mut in_cc2 = false;
            let mut capturing_stack: Vec<bool> = Vec::new();
            while j < len && depth > 0 {
                if chars[j] == '[' && !in_cc2 {
                    in_cc2 = true;
                } else if chars[j] == ']' && in_cc2 {
                    in_cc2 = false;
                } else if chars[j] == '\\' && j + 1 < len {
                    j += 1;
                    if chars[j].is_ascii_digit() && chars[j] != '0' {
                        has_backref = true;
                    }
                } else if chars[j] == '(' && !in_cc2 {
                    depth += 1;
                    let is_cap = if j + 1 < len && chars[j + 1] == '?' {
                        j + 2 < len
                            && chars[j + 2] == '<'
                            && j + 3 < len
                            && chars[j + 3] != '='
                            && chars[j + 3] != '!'
                    } else {
                        true
                    };
                    capturing_stack.push(is_cap);
                } else if chars[j] == ')' && !in_cc2 {
                    depth -= 1;
                    if depth > 0 {
                        let was_cap = capturing_stack.pop().unwrap_or(false);
                        if was_cap && j + 1 < len {
                            // Only flag variable-width quantifiers (+ and *).
                            // Fixed-count {n} and optional ? are handled correctly by fancy-regex.
                            if matches!(chars[j + 1], '+' | '*') {
                                has_quantified_capture = true;
                            }
                        }
                    }
                }
                if depth > 0 {
                    j += 1;
                }
            }

            if has_backref || has_quantified_capture {
                return true;
            }
        }
        i += 1;
    }
    false
}
