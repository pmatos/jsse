//! Parser for the custom right-to-left lookbehind matcher.

/// Custom RTL lookbehind matcher for patterns that fancy-regex cannot handle.
///
/// ECMAScript specifies that lookbehinds evaluate their content right-to-left (direction=-1).
/// This affects how backreferences resolve within the lookbehind: atoms are matched in
/// reverse order, so a later group captures before an earlier group.

#[derive(Debug, Clone)]
pub(super) struct LookbehindInfo {
    pub positive: bool,
    pub content: String,
    /// Whether this lookbehind is at the end of the pattern (suffix position).
    /// Suffix lookbehinds need retry-with-shorter-text when they fail.
    pub is_suffix: bool,
    /// Number of capturing groups before this lookbehind in the original pattern.
    /// Used to offset group numbering in the lookbehind parser so that backrefs
    /// correctly reference the global group numbers.
    pub capture_offset: u32,
    /// Number of capturing groups inside this lookbehind's content.
    pub num_captures: u32,
    /// Group index of the marker `()` in the stripped pattern (1-indexed).
    /// Used to read the position where the lookbehind should be verified.
    pub marker_group: u32,
}

#[derive(Debug, Clone)]
pub(super) enum LbAtom {
    Literal(char),
    CharClass(Vec<(char, char)>, bool),
    Dot(bool),
    CaptureGroup {
        num: u32,
        content: Vec<LbAtom>,
    },
    NonCaptureGroup {
        content: Vec<LbAtom>,
    },
    Backref(u32),
    Anchor(AnchorKind),
    Lookahead {
        positive: bool,
        content: Vec<LbAtom>,
    },
    Alternation(Vec<Vec<LbAtom>>),
    Quantified {
        atom: Box<LbAtom>,
        min: u32,
        max: Option<u32>,
        greedy: bool,
    },
}

#[derive(Debug, Clone)]
pub(super) enum AnchorKind {
    Start,
    End,
    WordBoundary,
    NonWordBoundary,
}

#[derive(Debug, Clone)]
pub(super) struct LbFlags {
    pub ignore_case: bool,
    pub multiline: bool,
    pub dot_all: bool,
    /// `u`/`v`: a lead and a trail `\uHHHH` escape form one code point.
    pub unicode: bool,
    /// The stand-in window the text view draws lone surrogates from.
    pub stand_in_base: u32,
}

impl LbFlags {
    pub(super) fn new(flags: &str, stand_in_base: u32) -> Self {
        Self {
            ignore_case: flags.contains('i'),
            multiline: flags.contains('m'),
            dot_all: flags.contains('s'),
            unicode: flags.contains('u') || flags.contains('v'),
            stand_in_base,
        }
    }
}

pub(super) fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

// ============================================================================
// Pattern parser
// ============================================================================

pub(super) fn parse_lb_atoms(pattern: &str, flags: &LbFlags, capture_offset: u32) -> Vec<LbAtom> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut gc = capture_offset;
    let (atoms, _) = parse_seq(&chars, 0, &mut gc, flags);
    atoms
}

fn parse_seq(chars: &[char], start: usize, gc: &mut u32, flags: &LbFlags) -> (Vec<LbAtom>, usize) {
    let mut alts: Vec<Vec<LbAtom>> = Vec::new();
    let mut cur: Vec<LbAtom> = Vec::new();
    let mut i = start;
    while i < chars.len() {
        match chars[i] {
            ')' => {
                if alts.is_empty() {
                    return (cur, i);
                }
                alts.push(cur);
                return (vec![LbAtom::Alternation(alts)], i);
            }
            '|' => {
                alts.push(cur);
                cur = Vec::new();
                i += 1;
            }
            '(' => {
                let (atom, end) = parse_group(chars, i, gc, flags);
                i = end + 1;
                let (atom, new_i) = maybe_quantify(chars, atom, i);
                cur.push(atom);
                i = new_i;
            }
            '[' => {
                let (atom, end) = parse_char_class(chars, i, flags);
                let (atom, new_i) = maybe_quantify(chars, atom, end);
                cur.push(atom);
                i = new_i;
            }
            '\\' if i + 1 < chars.len() => {
                let (atom, end) = parse_escape(chars, i + 1, flags);
                let (atom, new_i) = maybe_quantify(chars, atom, end);
                cur.push(atom);
                i = new_i;
            }
            '.' => {
                let atom = LbAtom::Dot(flags.dot_all);
                let (atom, new_i) = maybe_quantify(chars, atom, i + 1);
                cur.push(atom);
                i = new_i;
            }
            '^' => {
                cur.push(LbAtom::Anchor(AnchorKind::Start));
                i += 1;
            }
            '$' => {
                cur.push(LbAtom::Anchor(AnchorKind::End));
                i += 1;
            }
            _ => {
                let atom = LbAtom::Literal(chars[i]);
                let (atom, new_i) = maybe_quantify(chars, atom, i + 1);
                cur.push(atom);
                i = new_i;
            }
        }
    }
    if alts.is_empty() {
        (cur, i)
    } else {
        alts.push(cur);
        (vec![LbAtom::Alternation(alts)], i)
    }
}

fn parse_group(chars: &[char], start: usize, gc: &mut u32, flags: &LbFlags) -> (LbAtom, usize) {
    let mut i = start + 1;
    if i < chars.len() && chars[i] == '?' {
        i += 1;
        if i < chars.len() {
            match chars[i] {
                ':' => {
                    i += 1;
                    let (c, e) = parse_seq(chars, i, gc, flags);
                    return (LbAtom::NonCaptureGroup { content: c }, e);
                }
                '=' => {
                    i += 1;
                    let (c, e) = parse_seq(chars, i, gc, flags);
                    return (
                        LbAtom::Lookahead {
                            positive: true,
                            content: c,
                        },
                        e,
                    );
                }
                '!' => {
                    i += 1;
                    let (c, e) = parse_seq(chars, i, gc, flags);
                    return (
                        LbAtom::Lookahead {
                            positive: false,
                            content: c,
                        },
                        e,
                    );
                }
                '<' => {
                    // Check if named capture (?<name>...) vs lookbehind (?<= or ?<!)
                    if i + 1 < chars.len() && (chars[i + 1] == '=' || chars[i + 1] == '!') {
                        // Lookbehind assertion — treat as non-capturing
                        let positive = chars[i + 1] == '=';
                        i += 2;
                        let (c, e) = parse_seq(chars, i, gc, flags);
                        return (
                            LbAtom::Lookahead {
                                positive,
                                content: c,
                            },
                            e,
                        );
                    }
                    // Named capture group (?<name>...)
                    i += 1;
                    while i < chars.len() && chars[i] != '>' {
                        i += 1;
                    }
                    if i < chars.len() {
                        i += 1; // skip '>'
                    }
                    *gc += 1;
                    let num = *gc;
                    let (content, end) = parse_seq(chars, i, gc, flags);
                    return (LbAtom::CaptureGroup { num, content }, end);
                }
                _ => {
                    let (c, e) = parse_seq(chars, i, gc, flags);
                    return (LbAtom::NonCaptureGroup { content: c }, e);
                }
            }
        }
    }
    *gc += 1;
    let num = *gc;
    let (content, end) = parse_seq(chars, i, gc, flags);
    (LbAtom::CaptureGroup { num, content }, end)
}

fn parse_char_class(chars: &[char], start: usize, flags: &LbFlags) -> (LbAtom, usize) {
    let mut i = start + 1;
    let negated = if i < chars.len() && chars[i] == '^' {
        i += 1;
        true
    } else {
        false
    };
    let mut ranges: Vec<(char, char)> = Vec::new();
    while i < chars.len() && chars[i] != ']' {
        if chars[i] == '\\'
            && let Some((set, neg)) = chars.get(i + 1).and_then(|&c| class_escape(c))
        {
            ranges.extend(if neg { complement(set) } else { set });
            i += 2;
            continue;
        }
        let c = if chars[i] == '\\' && chars.get(i + 1) == Some(&'b') {
            i += 2;
            '\u{8}'
        } else if chars[i] == '\\' && i + 1 < chars.len() {
            let (c, next) = read_escaped_char(chars, i + 1, flags);
            i = next;
            c
        } else {
            i += 1;
            chars[i - 1]
        };
        if i + 1 < chars.len() && chars[i] == '-' && chars[i + 1] != ']' {
            i += 1;
            let e = if chars[i] == '\\' && i + 1 < chars.len() {
                let (e, next) = read_escaped_char(chars, i + 1, flags);
                i = next;
                e
            } else {
                i += 1;
                chars[i - 1]
            };
            ranges.push((c, e));
        } else {
            ranges.push((c, c));
        }
    }
    if i < chars.len() {
        i += 1;
    }
    (LbAtom::CharClass(ranges, negated), i)
}

fn unescape_char(c: char) -> char {
    match c {
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        'f' => '\x0C',
        'v' => '\x0B',
        '0' => '\0',
        _ => c,
    }
}

/// The character an escape denotes, `chars[i]` being the character after the
/// backslash, and the index after the escape. `\xHH`, `\uHHHH` and `\u{H+}`
/// are read as code points; a lone surrogate becomes its stand-in in the text
/// view's window, and a lead and a trail `\uHHHH` form one code point only in
/// Unicode mode.
fn read_escaped_char(chars: &[char], i: usize, flags: &LbFlags) -> (char, usize) {
    let hex = |from: usize, count: usize| -> Option<u32> {
        let digits = chars.get(from..from + count)?;
        digits
            .iter()
            .try_fold(0u32, |acc, c| Some(acc * 16 + c.to_digit(16)?))
    };
    let code_point = |cp: u32| -> Option<char> {
        if (0xD800..=0xDFFF).contains(&cp) {
            Some(super::regexp::surrogate_to_stand_in(
                cp,
                flags.stand_in_base,
            ))
        } else {
            char::from_u32(cp)
        }
    };
    match chars[i] {
        'x' => {
            if let Some(c) = hex(i + 1, 2).and_then(code_point) {
                return (c, i + 3);
            }
        }
        'u' if flags.unicode && chars.get(i + 1) == Some(&'{') => {
            if let Some(close) = chars[i + 2..].iter().position(|&c| c == '}') {
                let digits = &chars[i + 2..i + 2 + close];
                if !digits.is_empty()
                    && let Some(cp) = digits.iter().try_fold(0u32, |acc, c| {
                        acc.checked_mul(16)?.checked_add(c.to_digit(16)?)
                    })
                    && let Some(c) = code_point(cp)
                {
                    return (c, i + 2 + close + 1);
                }
            }
        }
        'u' => {
            if let Some(cp) = hex(i + 1, 4) {
                if flags.unicode
                    && (0xD800..=0xDBFF).contains(&cp)
                    && chars.get(i + 5) == Some(&'\\')
                    && chars.get(i + 6) == Some(&'u')
                    && let Some(trail) = hex(i + 7, 4)
                    && (0xDC00..=0xDFFF).contains(&trail)
                    && let Some(c) =
                        char::from_u32(0x10000 + ((cp - 0xD800) << 10) + (trail - 0xDC00))
                {
                    return (c, i + 11);
                }
                if let Some(c) = code_point(cp) {
                    return (c, i + 5);
                }
            }
        }
        _ => {}
    }
    (unescape_char(chars[i]), i + 1)
}

fn class_escape(c: char) -> Option<(Vec<(char, char)>, bool)> {
    let word = || vec![('a', 'z'), ('A', 'Z'), ('0', '9'), ('_', '_')];
    let space = || {
        vec![
            ('\t', '\r'),
            (' ', ' '),
            ('\u{a0}', '\u{a0}'),
            ('\u{1680}', '\u{1680}'),
            ('\u{2000}', '\u{200a}'),
            ('\u{2028}', '\u{2029}'),
            ('\u{202f}', '\u{202f}'),
            ('\u{205f}', '\u{205f}'),
            ('\u{3000}', '\u{3000}'),
            ('\u{feff}', '\u{feff}'),
        ]
    };
    match c {
        'd' => Some((vec![('0', '9')], false)),
        'D' => Some((vec![('0', '9')], true)),
        'w' => Some((word(), false)),
        'W' => Some((word(), true)),
        's' => Some((space(), false)),
        'S' => Some((space(), true)),
        _ => None,
    }
}

fn complement(mut ranges: Vec<(char, char)>) -> Vec<(char, char)> {
    ranges.sort();
    let mut out = Vec::new();
    let mut next = 0u32;
    for (s, e) in ranges {
        if (s as u32) > next
            && let Some(hi) = char::from_u32(s as u32 - 1).or(char::from_u32(0xD7FF))
            && let Some(lo) = char::from_u32(next).or(char::from_u32(0xE000))
            && lo <= hi
        {
            out.push((lo, hi));
        }
        next = next.max(e as u32 + 1);
    }
    if let Some(lo) = char::from_u32(next).or(char::from_u32(0xE000)) {
        out.push((lo, '\u{10FFFF}'));
    }
    out
}

fn parse_escape(chars: &[char], i: usize, flags: &LbFlags) -> (LbAtom, usize) {
    if let Some((ranges, negated)) = class_escape(chars[i]) {
        return (LbAtom::CharClass(ranges, negated), i + 1);
    }
    match chars[i] {
        'b' => (LbAtom::Anchor(AnchorKind::WordBoundary), i + 1),
        'B' => (LbAtom::Anchor(AnchorKind::NonWordBoundary), i + 1),
        '1'..='9' => {
            let mut end = i + 1;
            while end < chars.len() && chars[end].is_ascii_digit() {
                end += 1;
            }
            let n: u32 = chars[i..end]
                .iter()
                .collect::<String>()
                .parse()
                .unwrap_or(0);
            (LbAtom::Backref(n), end)
        }
        _ => {
            let (c, end) = read_escaped_char(chars, i, flags);
            (LbAtom::Literal(c), end)
        }
    }
}

fn maybe_quantify(chars: &[char], atom: LbAtom, i: usize) -> (LbAtom, usize) {
    if i >= chars.len() {
        return (atom, i);
    }
    let (min, max, end) = match chars[i] {
        '*' => (0, None, i + 1),
        '+' => (1, None, i + 1),
        '?' => (0, Some(1), i + 1),
        '{' => {
            let mut j = i + 1;
            let ns = j;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            if j == ns || j >= chars.len() {
                return (atom, i);
            }
            let first: u32 = chars[ns..j].iter().collect::<String>().parse().unwrap_or(0);
            if chars[j] == '}' {
                (first, Some(first), j + 1)
            } else if chars[j] == ',' {
                j += 1;
                if j < chars.len() && chars[j] == '}' {
                    (first, None, j + 1)
                } else {
                    let ns2 = j;
                    while j < chars.len() && chars[j].is_ascii_digit() {
                        j += 1;
                    }
                    if j > ns2 && j < chars.len() && chars[j] == '}' {
                        let second: u32 = chars[ns2..j]
                            .iter()
                            .collect::<String>()
                            .parse()
                            .unwrap_or(0);
                        (first, Some(second), j + 1)
                    } else {
                        return (atom, i);
                    }
                }
            } else {
                return (atom, i);
            }
        }
        _ => return (atom, i),
    };
    let (greedy, end) = if end < chars.len() && chars[end] == '?' {
        (false, end + 1)
    } else {
        (true, end)
    };
    (
        LbAtom::Quantified {
            atom: Box::new(atom),
            min,
            max,
            greedy,
        },
        end,
    )
}
