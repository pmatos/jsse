use super::regexp_lookbehind_parse::AnchorKind;
pub(super) use super::regexp_lookbehind_parse::{
    LbAtom, LbFlags, LookbehindInfo, is_line_terminator, parse_lb_atoms,
};
use std::collections::HashMap;

// ============================================================================
// RTL Matcher — proper backtracking across group boundaries
// ============================================================================

/// Match atoms right-to-left from `end_pos`. Returns start position if successful.
/// Captures are in char-index space (not byte offsets).
pub(super) fn match_rtl(
    atoms: &[LbAtom],
    input: &[char],
    end_pos: usize,
    captures: &mut Vec<Option<(usize, usize)>>,
    ext_caps: &[Option<String>],
    flags: &LbFlags,
    full_input: &[char],
    full_offset: usize,
) -> Option<usize> {
    // Base case: no more atoms to match
    if atoms.is_empty() {
        return Some(end_pos);
    }

    // Process the LAST atom first (RTL means rightmost atom is matched first)
    let last = &atoms[atoms.len() - 1];
    let rest = &atoms[..atoms.len() - 1];

    match last {
        LbAtom::Literal(ch) => {
            if end_pos == 0 {
                return None;
            }
            let c = input[end_pos - 1];
            if flags.ignore_case {
                if !char_eq_ic(c, *ch) {
                    return None;
                }
            } else if c != *ch {
                return None;
            }
            match_rtl(
                rest,
                input,
                end_pos - 1,
                captures,
                ext_caps,
                flags,
                full_input,
                full_offset,
            )
        }
        LbAtom::CharClass(ranges, negated) => {
            if end_pos == 0 {
                return None;
            }
            let c = input[end_pos - 1];
            let in_class = char_in_class(c, ranges, flags.ignore_case);
            if *negated == in_class {
                return None;
            }
            match_rtl(
                rest,
                input,
                end_pos - 1,
                captures,
                ext_caps,
                flags,
                full_input,
                full_offset,
            )
        }
        LbAtom::Dot(dot_all) => {
            if end_pos == 0 {
                return None;
            }
            let c = input[end_pos - 1];
            if !*dot_all && matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
                return None;
            }
            match_rtl(
                rest,
                input,
                end_pos - 1,
                captures,
                ext_caps,
                flags,
                full_input,
                full_offset,
            )
        }
        LbAtom::CaptureGroup { num, content } => {
            let saved_caps = captures.clone();
            let results = match_rtl_all(
                content,
                input,
                end_pos,
                captures,
                ext_caps,
                flags,
                full_input,
                full_offset,
            );
            for (start, inner_caps) in results {
                let mut try_caps = inner_caps;
                set_cap(&mut try_caps, *num as usize, Some((start, end_pos)));
                if let Some(result) = match_rtl(
                    rest,
                    input,
                    start,
                    &mut try_caps,
                    ext_caps,
                    flags,
                    full_input,
                    full_offset,
                ) {
                    *captures = try_caps;
                    return Some(result);
                }
            }
            *captures = saved_caps;
            None
        }
        LbAtom::NonCaptureGroup { content } => {
            let saved_caps = captures.clone();
            let results = match_rtl_all(
                content,
                input,
                end_pos,
                captures,
                ext_caps,
                flags,
                full_input,
                full_offset,
            );
            for (start, inner_caps) in results {
                let mut try_caps = inner_caps;
                if let Some(result) = match_rtl(
                    rest,
                    input,
                    start,
                    &mut try_caps,
                    ext_caps,
                    flags,
                    full_input,
                    full_offset,
                ) {
                    *captures = try_caps;
                    return Some(result);
                }
            }
            *captures = saved_caps;
            None
        }
        LbAtom::Backref(num) => {
            let n = *num as usize;
            let ref_text = get_backref_text(captures, ext_caps, n, input);
            let ref_chars: Vec<char> = ref_text.chars().collect();
            let ref_len = ref_chars.len();
            if end_pos < ref_len {
                return None;
            }
            let start = end_pos - ref_len;
            for (j, rc) in ref_chars.iter().enumerate() {
                let ic = input[start + j];
                if flags.ignore_case {
                    if !char_eq_ic(ic, *rc) {
                        return None;
                    }
                } else if ic != *rc {
                    return None;
                }
            }
            match_rtl(
                rest,
                input,
                start,
                captures,
                ext_caps,
                flags,
                full_input,
                full_offset,
            )
        }
        LbAtom::Anchor(kind) => {
            let abs_pos = end_pos + full_offset;
            match kind {
                AnchorKind::Start => {
                    if flags.multiline {
                        if abs_pos != 0
                            && !(abs_pos > 0 && is_line_terminator(full_input[abs_pos - 1]))
                        {
                            return None;
                        }
                    } else if abs_pos != 0 {
                        return None;
                    }
                }
                AnchorKind::End => {
                    if flags.multiline {
                        if abs_pos != full_input.len() && !is_line_terminator(full_input[abs_pos]) {
                            return None;
                        }
                    } else if abs_pos != full_input.len() {
                        return None;
                    }
                }
                AnchorKind::WordBoundary => {
                    let before = abs_pos > 0 && is_word_char(full_input[abs_pos - 1]);
                    let after = abs_pos < full_input.len() && is_word_char(full_input[abs_pos]);
                    if before == after {
                        return None;
                    }
                }
                AnchorKind::NonWordBoundary => {
                    let before = abs_pos > 0 && is_word_char(full_input[abs_pos - 1]);
                    let after = abs_pos < full_input.len() && is_word_char(full_input[abs_pos]);
                    if before != after {
                        return None;
                    }
                }
            }
            match_rtl(
                rest,
                input,
                end_pos,
                captures,
                ext_caps,
                flags,
                full_input,
                full_offset,
            )
        }
        LbAtom::Lookahead { positive, content } => {
            let abs_pos = end_pos + full_offset;
            let mut la_caps = captures.clone();
            let matched = match_ltr(content, full_input, abs_pos, &mut la_caps, ext_caps, flags);
            if *positive != matched.is_some() {
                return None;
            }
            if matched.is_some() {
                *captures = la_caps;
            }
            match_rtl(
                rest,
                input,
                end_pos,
                captures,
                ext_caps,
                flags,
                full_input,
                full_offset,
            )
        }
        LbAtom::Alternation(alternatives) => {
            for alt in alternatives {
                let mut alt_caps = captures.clone();
                if let Some(start) = match_rtl(
                    alt,
                    input,
                    end_pos,
                    &mut alt_caps,
                    ext_caps,
                    flags,
                    full_input,
                    full_offset,
                ) && let Some(result) = match_rtl(
                    rest,
                    input,
                    start,
                    &mut alt_caps,
                    ext_caps,
                    flags,
                    full_input,
                    full_offset,
                ) {
                    *captures = alt_caps;
                    return Some(result);
                }
            }
            None
        }
        LbAtom::Quantified {
            atom,
            min,
            max,
            greedy,
        } => {
            // Collect all possible match positions for 0..max repetitions going left
            let max_count = max.unwrap_or(end_pos as u32 + 1);
            let mut positions: Vec<(usize, Vec<Option<(usize, usize)>>)> = Vec::new();

            if *min == 0 {
                positions.push((end_pos, captures.clone()));
            }

            let mut cur_pos = end_pos;
            let mut cur_caps = captures.clone();
            for count in 1..=max_count {
                let mut iter_caps = cur_caps.clone();
                if let Some(new_pos) = match_single_atom_rtl(
                    atom,
                    input,
                    cur_pos,
                    &mut iter_caps,
                    ext_caps,
                    flags,
                    full_input,
                    full_offset,
                ) {
                    if new_pos == cur_pos {
                        break;
                    } // zero-width match would loop forever
                    cur_pos = new_pos;
                    cur_caps = iter_caps;
                    if count >= *min {
                        positions.push((cur_pos, cur_caps.clone()));
                    }
                } else {
                    break;
                }
            }

            // Try in order: greedy = longest first (most matches)
            if *greedy {
                positions.reverse();
            }

            for (pos, pos_caps) in positions {
                let mut try_caps = pos_caps;
                if let Some(result) = match_rtl(
                    rest,
                    input,
                    pos,
                    &mut try_caps,
                    ext_caps,
                    flags,
                    full_input,
                    full_offset,
                ) {
                    *captures = try_caps;
                    return Some(result);
                }
            }
            None
        }
    }
}

/// Return all possible (start_pos, captures) results for matching atoms RTL.
/// Results are ordered greedy-first (longest match first).
fn match_rtl_all(
    atoms: &[LbAtom],
    input: &[char],
    end_pos: usize,
    captures: &mut [Option<(usize, usize)>],
    ext_caps: &[Option<String>],
    flags: &LbFlags,
    full_input: &[char],
    full_offset: usize,
) -> Vec<(usize, Vec<Option<(usize, usize)>>)> {
    if atoms.is_empty() {
        return vec![(end_pos, captures.to_vec())];
    }

    let last = &atoms[atoms.len() - 1];
    let rest = &atoms[..atoms.len() - 1];
    let mut results = Vec::new();

    match last {
        LbAtom::Quantified {
            atom,
            min,
            max,
            greedy,
        } => {
            let max_count = max.unwrap_or(end_pos as u32 + 1);
            let mut positions: Vec<(usize, Vec<Option<(usize, usize)>>)> = Vec::new();
            if *min == 0 {
                positions.push((end_pos, captures.to_vec()));
            }
            let mut cur_pos = end_pos;
            let mut cur_caps = captures.to_vec();
            for count in 1..=max_count {
                let mut iter_caps = cur_caps.clone();
                if let Some(new_pos) = match_single_atom_rtl(
                    atom,
                    input,
                    cur_pos,
                    &mut iter_caps,
                    ext_caps,
                    flags,
                    full_input,
                    full_offset,
                ) {
                    if new_pos == cur_pos {
                        break;
                    }
                    cur_pos = new_pos;
                    cur_caps = iter_caps;
                    if count >= *min {
                        positions.push((cur_pos, cur_caps.clone()));
                    }
                } else {
                    break;
                }
            }
            if *greedy {
                positions.reverse();
            }
            for (pos, pos_caps) in positions {
                let mut try_caps = pos_caps;
                let rest_results = match_rtl_all(
                    rest,
                    input,
                    pos,
                    &mut try_caps,
                    ext_caps,
                    flags,
                    full_input,
                    full_offset,
                );
                results.extend(rest_results);
            }
        }
        LbAtom::Alternation(alternatives) => {
            for alt in alternatives {
                let mut alt_caps = captures.to_vec();
                let alt_results = match_rtl_all(
                    alt,
                    input,
                    end_pos,
                    &mut alt_caps,
                    ext_caps,
                    flags,
                    full_input,
                    full_offset,
                );
                for (start, inner_caps) in alt_results {
                    let mut try_caps = inner_caps;
                    let rest_results = match_rtl_all(
                        rest,
                        input,
                        start,
                        &mut try_caps,
                        ext_caps,
                        flags,
                        full_input,
                        full_offset,
                    );
                    results.extend(rest_results);
                }
            }
        }
        _ => {
            // For simple atoms, just try matching and recurse
            let mut try_caps = captures.to_vec();
            if let Some(new_pos) = match_rtl(
                std::slice::from_ref(last),
                input,
                end_pos,
                &mut try_caps,
                ext_caps,
                flags,
                full_input,
                full_offset,
            ) {
                let rest_results = match_rtl_all(
                    rest,
                    input,
                    new_pos,
                    &mut try_caps,
                    ext_caps,
                    flags,
                    full_input,
                    full_offset,
                );
                results.extend(rest_results);
            }
        }
    }

    results
}

/// Match a single atom RTL (no recursion into remaining atoms).
fn match_single_atom_rtl(
    atom: &LbAtom,
    input: &[char],
    end_pos: usize,
    captures: &mut Vec<Option<(usize, usize)>>,
    ext_caps: &[Option<String>],
    flags: &LbFlags,
    full_input: &[char],
    full_offset: usize,
) -> Option<usize> {
    match atom {
        LbAtom::Literal(ch) => {
            if end_pos == 0 {
                return None;
            }
            let c = input[end_pos - 1];
            if flags.ignore_case {
                if !char_eq_ic(c, *ch) {
                    return None;
                }
            } else if c != *ch {
                return None;
            }
            Some(end_pos - 1)
        }
        LbAtom::CharClass(ranges, negated) => {
            if end_pos == 0 {
                return None;
            }
            let c = input[end_pos - 1];
            let in_class = char_in_class(c, ranges, flags.ignore_case);
            if *negated == in_class {
                return None;
            }
            Some(end_pos - 1)
        }
        LbAtom::Dot(dot_all) => {
            if end_pos == 0 {
                return None;
            }
            let c = input[end_pos - 1];
            if !*dot_all && matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
                return None;
            }
            Some(end_pos - 1)
        }
        LbAtom::CaptureGroup { num, content } => {
            if let Some(start) = match_rtl(
                content,
                input,
                end_pos,
                captures,
                ext_caps,
                flags,
                full_input,
                full_offset,
            ) {
                set_cap(captures, *num as usize, Some((start, end_pos)));
                Some(start)
            } else {
                None
            }
        }
        LbAtom::NonCaptureGroup { content } => match_rtl(
            content,
            input,
            end_pos,
            captures,
            ext_caps,
            flags,
            full_input,
            full_offset,
        ),
        LbAtom::Alternation(alternatives) => {
            for alt in alternatives {
                let mut alt_caps = captures.clone();
                if let Some(start) = match_rtl(
                    alt,
                    input,
                    end_pos,
                    &mut alt_caps,
                    ext_caps,
                    flags,
                    full_input,
                    full_offset,
                ) {
                    *captures = alt_caps;
                    return Some(start);
                }
            }
            None
        }
        _ => {
            // For other atoms (anchors, backrefs, etc.), wrap in a slice
            match_rtl(
                std::slice::from_ref(atom),
                input,
                end_pos,
                captures,
                ext_caps,
                flags,
                full_input,
                full_offset,
            )
        }
    }
}

// ============================================================================
// LTR Matcher for lookaheads inside lookbehinds
// ============================================================================

fn match_ltr(
    atoms: &[LbAtom],
    input: &[char],
    start_pos: usize,
    captures: &mut Vec<Option<(usize, usize)>>,
    ext_caps: &[Option<String>],
    flags: &LbFlags,
) -> Option<usize> {
    if atoms.is_empty() {
        return Some(start_pos);
    }
    let first = &atoms[0];
    let rest = &atoms[1..];

    match first {
        LbAtom::Literal(ch) => {
            if start_pos >= input.len() {
                return None;
            }
            if flags.ignore_case {
                if !char_eq_ic(input[start_pos], *ch) {
                    return None;
                }
            } else if input[start_pos] != *ch {
                return None;
            }
            match_ltr(rest, input, start_pos + 1, captures, ext_caps, flags)
        }
        LbAtom::CharClass(ranges, negated) => {
            if start_pos >= input.len() {
                return None;
            }
            let in_class = char_in_class(input[start_pos], ranges, flags.ignore_case);
            if *negated == in_class {
                return None;
            }
            match_ltr(rest, input, start_pos + 1, captures, ext_caps, flags)
        }
        LbAtom::Dot(dot_all) => {
            if start_pos >= input.len() {
                return None;
            }
            if !*dot_all && matches!(input[start_pos], '\n' | '\r' | '\u{2028}' | '\u{2029}') {
                return None;
            }
            match_ltr(rest, input, start_pos + 1, captures, ext_caps, flags)
        }
        LbAtom::CaptureGroup { num, content } => {
            let saved = get_cap(captures, *num as usize);
            if let Some(end) = match_ltr(content, input, start_pos, captures, ext_caps, flags) {
                set_cap(captures, *num as usize, Some((start_pos, end)));
                if let Some(result) = match_ltr(rest, input, end, captures, ext_caps, flags) {
                    return Some(result);
                }
                set_cap(captures, *num as usize, saved);
            }
            None
        }
        LbAtom::NonCaptureGroup { content } => {
            if let Some(end) = match_ltr(content, input, start_pos, captures, ext_caps, flags) {
                match_ltr(rest, input, end, captures, ext_caps, flags)
            } else {
                None
            }
        }
        LbAtom::Backref(num) => {
            let n = *num as usize;
            let ref_text = get_backref_text(captures, ext_caps, n, input);
            let ref_chars: Vec<char> = ref_text.chars().collect();
            if start_pos + ref_chars.len() > input.len() {
                return None;
            }
            for (j, rc) in ref_chars.iter().enumerate() {
                if flags.ignore_case {
                    if !char_eq_ic(input[start_pos + j], *rc) {
                        return None;
                    }
                } else if input[start_pos + j] != *rc {
                    return None;
                }
            }
            match_ltr(
                rest,
                input,
                start_pos + ref_chars.len(),
                captures,
                ext_caps,
                flags,
            )
        }
        LbAtom::Anchor(kind) => {
            match kind {
                AnchorKind::Start => {
                    if flags.multiline {
                        if start_pos != 0
                            && !(start_pos > 0 && is_line_terminator(input[start_pos - 1]))
                        {
                            return None;
                        }
                    } else if start_pos != 0 {
                        return None;
                    }
                }
                AnchorKind::End => {
                    if flags.multiline {
                        if start_pos != input.len() && !is_line_terminator(input[start_pos]) {
                            return None;
                        }
                    } else if start_pos != input.len() {
                        return None;
                    }
                }
                AnchorKind::WordBoundary => {
                    let before = start_pos > 0 && is_word_char(input[start_pos - 1]);
                    let after = start_pos < input.len() && is_word_char(input[start_pos]);
                    if before == after {
                        return None;
                    }
                }
                AnchorKind::NonWordBoundary => {
                    let before = start_pos > 0 && is_word_char(input[start_pos - 1]);
                    let after = start_pos < input.len() && is_word_char(input[start_pos]);
                    if before != after {
                        return None;
                    }
                }
            }
            match_ltr(rest, input, start_pos, captures, ext_caps, flags)
        }
        LbAtom::Lookahead { positive, content } => {
            let mut la_caps = captures.clone();
            let matched = match_ltr(content, input, start_pos, &mut la_caps, ext_caps, flags);
            if *positive != matched.is_some() {
                return None;
            }
            if matched.is_some() {
                *captures = la_caps;
            }
            match_ltr(rest, input, start_pos, captures, ext_caps, flags)
        }
        LbAtom::Alternation(alternatives) => {
            for alt in alternatives {
                let mut alt_caps = captures.clone();
                if let Some(end) = match_ltr(alt, input, start_pos, &mut alt_caps, ext_caps, flags)
                    && let Some(result) =
                        match_ltr(rest, input, end, &mut alt_caps, ext_caps, flags)
                {
                    *captures = alt_caps;
                    return Some(result);
                }
            }
            None
        }
        LbAtom::Quantified {
            atom,
            min,
            max,
            greedy,
        } => {
            let max_count = max.unwrap_or((input.len() - start_pos) as u32 + 1);
            let mut positions: Vec<(usize, Vec<Option<(usize, usize)>>)> = Vec::new();
            if *min == 0 {
                positions.push((start_pos, captures.clone()));
            }
            let mut cur_pos = start_pos;
            let mut cur_caps = captures.clone();
            for count in 1..=max_count {
                let mut iter_caps = cur_caps.clone();
                if let Some(new_pos) =
                    match_single_atom_ltr(atom, input, cur_pos, &mut iter_caps, ext_caps, flags)
                {
                    if new_pos == cur_pos {
                        break;
                    }
                    cur_pos = new_pos;
                    cur_caps = iter_caps;
                    if count >= *min {
                        positions.push((cur_pos, cur_caps.clone()));
                    }
                } else {
                    break;
                }
            }
            if *greedy {
                positions.reverse();
            }
            for (pos, pos_caps) in positions {
                let mut try_caps = pos_caps;
                if let Some(result) = match_ltr(rest, input, pos, &mut try_caps, ext_caps, flags) {
                    *captures = try_caps;
                    return Some(result);
                }
            }
            None
        }
    }
}

fn match_single_atom_ltr(
    atom: &LbAtom,
    input: &[char],
    start_pos: usize,
    captures: &mut Vec<Option<(usize, usize)>>,
    ext_caps: &[Option<String>],
    flags: &LbFlags,
) -> Option<usize> {
    match atom {
        LbAtom::Literal(ch) => {
            if start_pos >= input.len() {
                return None;
            }
            if flags.ignore_case {
                if !char_eq_ic(input[start_pos], *ch) {
                    return None;
                }
            } else if input[start_pos] != *ch {
                return None;
            }
            Some(start_pos + 1)
        }
        LbAtom::CharClass(ranges, negated) => {
            if start_pos >= input.len() {
                return None;
            }
            let in_class = char_in_class(input[start_pos], ranges, flags.ignore_case);
            if *negated == in_class {
                return None;
            }
            Some(start_pos + 1)
        }
        LbAtom::Dot(dot_all) => {
            if start_pos >= input.len() {
                return None;
            }
            if !*dot_all && matches!(input[start_pos], '\n' | '\r' | '\u{2028}' | '\u{2029}') {
                return None;
            }
            Some(start_pos + 1)
        }
        LbAtom::CaptureGroup { num, content } => {
            if let Some(end) = match_ltr(content, input, start_pos, captures, ext_caps, flags) {
                set_cap(captures, *num as usize, Some((start_pos, end)));
                Some(end)
            } else {
                None
            }
        }
        LbAtom::NonCaptureGroup { content } => {
            match_ltr(content, input, start_pos, captures, ext_caps, flags)
        }
        LbAtom::Alternation(alternatives) => {
            for alt in alternatives {
                let mut alt_caps = captures.clone();
                if let Some(end) = match_ltr(alt, input, start_pos, &mut alt_caps, ext_caps, flags)
                {
                    *captures = alt_caps;
                    return Some(end);
                }
            }
            None
        }
        _ => match_ltr(
            std::slice::from_ref(atom),
            input,
            start_pos,
            captures,
            ext_caps,
            flags,
        ),
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn char_eq_ic(a: char, b: char) -> bool {
    a.to_lowercase().eq(b.to_lowercase())
}

fn char_in_class(c: char, ranges: &[(char, char)], ignore_case: bool) -> bool {
    if ignore_case {
        for cl in c.to_lowercase() {
            for &(s, e) in ranges {
                for sl in s.to_lowercase() {
                    for el in e.to_lowercase() {
                        if cl >= sl && cl <= el {
                            return true;
                        }
                    }
                }
            }
        }
        false
    } else {
        ranges.iter().any(|&(s, e)| c >= s && c <= e)
    }
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn get_cap(caps: &[Option<(usize, usize)>], idx: usize) -> Option<(usize, usize)> {
    caps.get(idx).copied().flatten()
}

fn set_cap(caps: &mut Vec<Option<(usize, usize)>>, idx: usize, val: Option<(usize, usize)>) {
    while caps.len() <= idx {
        caps.push(None);
    }
    caps[idx] = val;
}

fn get_backref_text(
    captures: &[Option<(usize, usize)>],
    ext_caps: &[Option<String>],
    n: usize,
    input: &[char],
) -> String {
    if n < captures.len()
        && let Some((start, end)) = captures[n]
    {
        return input[start..end].iter().collect();
    }
    if n < ext_caps.len() {
        return ext_caps[n].clone().unwrap_or_default();
    }
    String::new()
}

// ============================================================================
// Pattern decomposition
// ============================================================================

fn is_suffix_position(chars: &[char], pos: usize) -> bool {
    for ch in &chars[pos..] {
        match ch {
            '$' | ')' => continue,
            _ => return false,
        }
    }
    true
}

pub(super) fn extract_lookbehinds(source: &str) -> (Vec<LookbehindInfo>, String) {
    let chars: Vec<char> = source.chars().collect();
    let len = chars.len();
    let mut lookbehinds = Vec::new();
    let mut stripped = String::new();
    let mut i = 0;
    let mut in_char_class = false;
    let mut orig_capture_count: u32 = 0;
    let mut stripped_capture_count: u32 = 0;

    while i < len {
        if chars[i] == '[' && !in_char_class {
            in_char_class = true;
            stripped.push(chars[i]);
            i += 1;
            continue;
        }
        if chars[i] == ']' && in_char_class {
            in_char_class = false;
            stripped.push(chars[i]);
            i += 1;
            continue;
        }
        if chars[i] == '\\' && i + 1 < len {
            stripped.push(chars[i]);
            stripped.push(chars[i + 1]);
            i += 2;
            continue;
        }

        if !in_char_class
            && chars[i] == '('
            && i + 3 < len
            && chars[i + 1] == '?'
            && chars[i + 2] == '<'
            && (chars[i + 3] == '=' || chars[i + 3] == '!')
        {
            let positive = chars[i + 3] == '=';
            let content_start = i + 4;
            let mut depth = 1;
            let mut j = content_start;
            let mut in_cc = false;
            while j < len && depth > 0 {
                if chars[j] == '[' && !in_cc {
                    in_cc = true;
                } else if chars[j] == ']' && in_cc {
                    in_cc = false;
                } else if chars[j] == '\\' && j + 1 < len {
                    j += 1;
                } else if chars[j] == '(' && !in_cc {
                    depth += 1;
                } else if chars[j] == ')' && !in_cc {
                    depth -= 1;
                }
                if depth > 0 {
                    j += 1;
                }
            }
            let content: String = chars[content_start..j].iter().collect();

            let mut num_captures: u32 = 0;
            let mut k = content_start;
            let mut in_cc2 = false;
            while k < j {
                if chars[k] == '[' && !in_cc2 {
                    in_cc2 = true;
                } else if chars[k] == ']' && in_cc2 {
                    in_cc2 = false;
                } else if chars[k] == '\\' && k + 1 < len {
                    k += 1;
                } else if chars[k] == '(' && !in_cc2 {
                    if k + 1 < len && chars[k + 1] == '?' {
                        if k + 2 < len
                            && chars[k + 2] == '<'
                            && k + 3 < len
                            && chars[k + 3] != '='
                            && chars[k + 3] != '!'
                        {
                            num_captures += 1;
                        }
                    } else {
                        num_captures += 1;
                    }
                }
                k += 1;
            }

            let is_suffix = is_suffix_position(&chars, j + 1);
            let capture_offset = orig_capture_count;
            let assertion =
                super::regexp_lookbehind_route::capture_free_assertion(&content, positive);

            // Add marker capturing group to track the lookbehind position
            stripped_capture_count += 1;
            let marker_group = stripped_capture_count;

            lookbehinds.push(LookbehindInfo {
                positive,
                content,
                is_suffix,
                capture_offset,
                num_captures,
                marker_group,
            });
            orig_capture_count += num_captures;
            match assertion {
                Some(assertion) => {
                    stripped.push_str(&assertion);
                    stripped.push_str("()");
                }
                None => stripped.push_str("()"),
            }
            i = j + 1;
            continue;
        }

        // Count capturing groups in non-lookbehind parts
        if !in_char_class && chars[i] == '(' {
            if i + 1 < len && chars[i + 1] == '?' {
                if i + 2 < len
                    && chars[i + 2] == '<'
                    && i + 3 < len
                    && chars[i + 3] != '='
                    && chars[i + 3] != '!'
                {
                    orig_capture_count += 1;
                    stripped_capture_count += 1;
                }
            } else {
                orig_capture_count += 1;
                stripped_capture_count += 1;
            }
        }

        stripped.push(chars[i]);
        i += 1;
    }

    (lookbehinds, stripped)
}

/// Extract the remaining pattern (after stripping all lookbehinds) without
/// marker groups. Returns (lookbehinds, remaining_pattern).
pub(super) fn extract_lookbehinds_remaining(source: &str) -> (Vec<LookbehindInfo>, String) {
    let chars: Vec<char> = source.chars().collect();
    let len = chars.len();
    let mut lookbehinds = Vec::new();
    let mut remaining = String::new();
    let mut i = 0;
    let mut in_char_class = false;

    while i < len {
        if chars[i] == '[' && !in_char_class {
            in_char_class = true;
            remaining.push(chars[i]);
            i += 1;
            continue;
        }
        if chars[i] == ']' && in_char_class {
            in_char_class = false;
            remaining.push(chars[i]);
            i += 1;
            continue;
        }
        if chars[i] == '\\' && i + 1 < len {
            remaining.push(chars[i]);
            remaining.push(chars[i + 1]);
            i += 2;
            continue;
        }

        if !in_char_class
            && chars[i] == '('
            && i + 3 < len
            && chars[i + 1] == '?'
            && chars[i + 2] == '<'
            && (chars[i + 3] == '=' || chars[i + 3] == '!')
        {
            let positive = chars[i + 3] == '=';
            let content_start = i + 4;
            let mut depth = 1;
            let mut j = content_start;
            let mut in_cc = false;
            while j < len && depth > 0 {
                if chars[j] == '[' && !in_cc {
                    in_cc = true;
                } else if chars[j] == ']' && in_cc {
                    in_cc = false;
                } else if chars[j] == '\\' && j + 1 < len {
                    j += 1;
                } else if chars[j] == '(' && !in_cc {
                    depth += 1;
                } else if chars[j] == ')' && !in_cc {
                    depth -= 1;
                }
                if depth > 0 {
                    j += 1;
                }
            }
            let content: String = chars[content_start..j].iter().collect();
            lookbehinds.push(LookbehindInfo {
                positive,
                content,
                is_suffix: false,
                capture_offset: 0,
                num_captures: 0,
                marker_group: 0,
            });
            // Skip lookbehind — don't add to remaining
            i = j + 1;
            continue;
        }

        remaining.push(chars[i]);
        i += 1;
    }

    (lookbehinds, remaining)
}

// ============================================================================
// Integration: match with custom lookbehind
// ============================================================================

pub(super) fn match_with_lookbehind(
    outer_regex: &fancy_regex::Regex,
    lookbehinds: &[LookbehindInfo],
    text: &str,
    flags: &str,
    base: u32,
    start_pos: usize,
    total_groups: usize,
) -> Option<Vec<Option<(usize, usize)>>> {
    let text_chars: Vec<char> = text.chars().collect();
    let lb_flags = LbFlags::new(flags, base);

    let mut char_to_byte: Vec<usize> = Vec::with_capacity(text_chars.len() + 1);
    let mut byte_off = 0;
    for ch in &text_chars {
        char_to_byte.push(byte_off);
        byte_off += ch.len_utf8();
    }
    char_to_byte.push(byte_off);

    // Collect marker group indices (to skip when building results)
    let marker_groups: Vec<u32> = lookbehinds.iter().map(|lb| lb.marker_group).collect();

    let mut search_pos = start_pos;
    loop {
        let mut text_limit = text.len();

        'retry: loop {
            let search_text = &text[..text_limit];
            let caps = match outer_regex.captures_from_pos(search_text, search_pos) {
                Ok(Some(c)) => c,
                _ => break,
            };

            let overall = match caps.get(0) {
                Some(m) => m,
                None => break,
            };
            let match_start = overall.start();
            let match_end = overall.end();

            let mut ext_caps: Vec<Option<String>> = Vec::new();
            for i in 0..caps.len() {
                ext_caps.push(caps.get(i).map(|m| m.as_str().to_string()));
            }

            let mut all_ok = true;
            let mut suffix_failed = false;
            let mut lb_cap_results: Vec<Vec<Option<(usize, usize)>>> = Vec::new();

            for lb in lookbehinds {
                let mut lb_caps: Vec<Option<(usize, usize)>> = Vec::new();

                // If the marker group wasn't captured, this match came from
                // an alternative that doesn't include the lookbehind — skip.
                let marker_match = caps.get(lb.marker_group as usize);
                if marker_match.is_none() {
                    lb_cap_results.push(lb_caps);
                    continue;
                }

                let atoms = parse_lb_atoms(&lb.content, &lb_flags, lb.capture_offset);

                let marker_byte_pos = marker_match.map(|m| m.start()).unwrap_or(match_start);
                let check_pos = text[..marker_byte_pos].chars().count();

                let found = match_rtl(
                    &atoms,
                    &text_chars[..check_pos],
                    check_pos,
                    &mut lb_caps,
                    &ext_caps,
                    &lb_flags,
                    &text_chars,
                    0,
                )
                .is_some();

                if lb.positive != found {
                    all_ok = false;
                    if lb.is_suffix {
                        suffix_failed = true;
                    }
                    break;
                }
                lb_cap_results.push(lb_caps);
            }

            if all_ok {
                let mut result: Vec<Option<(usize, usize)>> = vec![None; total_groups + 1];
                result[0] = Some((match_start, match_end));

                // Map outer regex captures to original group numbers, skipping markers
                let mut rank = 0;
                for stripped_idx in 1..caps.len() {
                    if marker_groups.contains(&(stripped_idx as u32)) {
                        continue;
                    }
                    rank += 1;
                    let orig_idx =
                        super::regexp_lookbehind_route::outer_orig_index(lookbehinds, rank);
                    if orig_idx < result.len()
                        && let Some(m) = caps.get(stripped_idx)
                    {
                        result[orig_idx] = Some((m.start(), m.end()));
                    }
                }

                // Merge lookbehind captures (convert char offsets to byte offsets)
                for lb_caps in &lb_cap_results {
                    for (idx, cap) in lb_caps.iter().enumerate() {
                        if idx > 0
                            && idx < result.len()
                            && let Some((cs, ce)) = cap
                        {
                            result[idx] = Some((char_to_byte[*cs], char_to_byte[*ce]));
                        }
                    }
                }

                return Some(result);
            }

            // If a suffix lookbehind failed, try shorter text to get a shorter match
            if suffix_failed && text_limit > match_start {
                let mut new_limit = match_end;
                if new_limit > 0 {
                    new_limit -= 1;
                    while new_limit > match_start && !text.is_char_boundary(new_limit) {
                        new_limit -= 1;
                    }
                }
                if new_limit > match_start {
                    text_limit = new_limit;
                    continue 'retry;
                }
            }

            break;
        }

        // Advance search position
        if search_pos < text.len() {
            search_pos += 1;
            while search_pos < text.len() && !text.is_char_boundary(search_pos) {
                search_pos += 1;
            }
            if search_pos > text.len() {
                return None;
            }
        } else {
            return None;
        }
    }
}

// ============================================================================
// No-backtrack matching for lookbehind captures with external backrefs
// ============================================================================

/// Match pattern where lookbehind captures are referenced by external backrefs.
/// Iterates positions, runs RTL lookbehind at each, substitutes capture values
/// as literals into the remaining pattern, and tries to match.
pub(super) fn match_with_lookbehind_no_backtrack(
    lookbehinds: &[LookbehindInfo],
    remaining_source: &str,
    flags_str: &str,
    base: u32,
    text: &str,
    start_pos: usize,
    total_groups: usize,
    external_lb_backrefs: &[(u32, u32)],
) -> Option<Vec<Option<(usize, usize)>>> {
    const MAX_NO_BACKTRACK_COMPILES: usize = 256;

    let text_chars: Vec<char> = text.chars().collect();
    let char_to_byte: Vec<usize> = text_chars
        .iter()
        .scan(0usize, |acc, c| {
            let pos = *acc;
            *acc += c.len_utf8();
            Some(pos)
        })
        .collect();
    let byte_to_char = |byte_pos: usize| -> usize {
        char_to_byte
            .iter()
            .position(|&b| b == byte_pos)
            .unwrap_or(text_chars.len())
    };
    let char_to_byte_fn = |char_pos: usize| -> usize {
        if char_pos < char_to_byte.len() {
            char_to_byte[char_pos]
        } else {
            text.len()
        }
    };

    let lb_flags = LbFlags::new(flags_str, base);

    let start_char = byte_to_char(start_pos);
    let parsed_lookbehinds: Vec<Vec<LbAtom>> = lookbehinds
        .iter()
        .map(|lb| parse_lb_atoms(&lb.content, &lb_flags, lb.capture_offset))
        .collect();
    let mut regex_cache: HashMap<String, fancy_regex::Regex> = HashMap::new();
    let mut compile_count = 0usize;

    for pos_char in start_char..=text_chars.len() {
        let pos_byte = char_to_byte_fn(pos_char);

        // Run each lookbehind at this position
        let mut all_ok = true;
        let mut lb_caps_all: Vec<Vec<Option<(usize, usize)>>> = Vec::new();

        for (lb, atoms) in lookbehinds.iter().zip(parsed_lookbehinds.iter()) {
            let cap_size = total_groups + 1;
            let mut lb_caps = vec![None; cap_size];

            let found = match_rtl(
                atoms,
                &text_chars[..pos_char],
                pos_char,
                &mut lb_caps,
                &[],
                &lb_flags,
                &text_chars,
                0,
            )
            .is_some();

            if lb.positive != found {
                all_ok = false;
                break;
            }
            lb_caps_all.push(lb_caps);
        }

        if !all_ok {
            continue;
        }

        // Build substituted remaining pattern: replace \N with literal captured text
        let mut replacements: Vec<(u32, String)> = Vec::new();
        for &(backref_num, lb_group_num) in external_lb_backrefs {
            let mut cap_text = String::new();
            for lb_caps in &lb_caps_all {
                if let Some(Some((s, e))) = lb_caps.get(lb_group_num as usize) {
                    cap_text = text_chars[*s..*e].iter().collect();
                    break;
                }
            }
            replacements.push((backref_num, regex_escape_for_js(&cap_text)));
        }
        let subst =
            super::regexp_lookbehind_route::substitute_backrefs(remaining_source, &replacements);

        if !regex_cache.contains_key(&subst) {
            // Cap unique compilations to bound DoS exposure, but keep scanning:
            // skipping this position preserves matches at later positions whose
            // substitutions either hit the existing cache or never compile.
            if compile_count >= MAX_NO_BACKTRACK_COMPILES {
                continue;
            }

            // Translate and compile substituted pattern anchored at this position
            let tr = match super::regexp::translate_js_pattern_ex(&subst, flags_str, base) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let anchored = format!("^(?:{})", tr.pattern);
            let re = match fancy_regex::Regex::new(&anchored) {
                Ok(r) => r,
                Err(_) => continue,
            };
            compile_count += 1;
            regex_cache.insert(subst.clone(), re);
        }
        let re = match regex_cache.get(&subst) {
            Some(r) => r,
            None => continue,
        };

        // Match against text from this position
        let text_from_pos = &text[pos_byte..];
        if let Ok(Some(caps)) = re.captures(text_from_pos) {
            let overall = caps.get(0)?;
            let match_start = pos_byte;
            let match_end = pos_byte + overall.end();

            let mut result: Vec<Option<(usize, usize)>> = vec![None; total_groups + 1];
            result[0] = Some((match_start, match_end));

            // Merge lookbehind captures
            for lb_caps in &lb_caps_all {
                for (idx, cap) in lb_caps.iter().enumerate() {
                    if idx > 0
                        && idx < result.len()
                        && let Some((cs, ce)) = cap
                    {
                        result[idx] = Some((char_to_byte_fn(*cs), char_to_byte_fn(*ce)));
                    }
                }
            }

            // Merge remaining captures from the substituted regex
            // Skip group 0 (already set). Remaining groups in the substituted regex
            // correspond to non-lookbehind groups in the original pattern.
            // We need to map them to the right indices.
            for i in 1..caps.len() {
                if let Some(m) = caps.get(i) {
                    let orig_idx = super::regexp_lookbehind_route::outer_orig_index(lookbehinds, i);
                    if orig_idx < result.len() && result[orig_idx].is_none() {
                        result[orig_idx] = Some((pos_byte + m.start(), pos_byte + m.end()));
                    }
                }
            }

            return Some(result);
        }
    }

    None
}

fn regex_escape_for_js(s: &str) -> String {
    let mut result = String::new();
    for ch in s.chars() {
        match ch {
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' => {
                result.push('\\');
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }
    result
}

// ============================================================================
// Lookbehind verification for no-backtrack semantics
// ============================================================================

/// Info for verifying lookbehind matches produced by fancy-regex.
/// Used when lookbehind captures are referenced by external backrefs,
/// which means fancy-regex might backtrack into the lookbehind (violating spec).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) struct LookbehindVerifyInfo {
    pub positive: bool,
    pub content: String,
    /// Global capture group numbers that are inside this lookbehind
    pub capture_groups: Vec<u32>,
    /// Number of groups before this lookbehind (for group numbering offset)
    pub capture_offset: u32,
}
