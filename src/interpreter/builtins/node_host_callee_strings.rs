//! String literals for the callee scanner (`node_host_callee.rs`, #1567): V8
//! prints the cooked value of a string key (`o['\\']()` is `o.\ is not a
//! function`), and a bracket inside a string (`a["]"]()`) does not close the
//! index.

/// The index of the quote that closes the string literal opened at `open`.
pub(super) fn string_close_after(b: &[u8], open: usize) -> Option<usize> {
    let quote = b[open];
    let mut at = open + 1;
    while at < b.len() {
        match b[at] {
            b'\\' => at += 1,
            c if c == quote => return Some(at),
            b'\n' => return None,
            _ => {}
        }
        at += 1;
    }
    None
}

fn hex(digits: &str) -> Option<u32> {
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| u32::from_str_radix(digits, 16).ok())
        .flatten()
}

/// The cooked value of the string literal `lit` (quotes included).
pub(super) fn unquote(lit: &str) -> Option<String> {
    let quote = lit.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    let body = lit.strip_prefix(quote)?.strip_suffix(quote)?;
    let mut out = String::new();
    let mut units: Vec<u16> = Vec::new();
    let mut chars = body.chars().peekable();
    let flush = |units: &mut Vec<u16>, out: &mut String| {
        out.push_str(&String::from_utf16_lossy(units));
        units.clear();
    };
    while let Some(c) = chars.next() {
        if c != '\\' {
            flush(&mut units, &mut out);
            out.push(c);
            continue;
        }
        let esc = chars.next()?;
        match esc {
            'x' | 'u' => {
                let digits: String = if esc == 'u' && chars.peek() == Some(&'{') {
                    chars.next();
                    let mut d = String::new();
                    loop {
                        match chars.next()? {
                            '}' => break,
                            c => d.push(c),
                        }
                    }
                    d
                } else {
                    (0..if esc == 'x' { 2 } else { 4 })
                        .map(|_| chars.next())
                        .collect::<Option<String>>()?
                };
                let code = hex(&digits)?;
                match char::from_u32(code) {
                    Some(ch) => {
                        flush(&mut units, &mut out);
                        out.push(ch);
                    }
                    None => units.push(u16::try_from(code).ok()?),
                }
            }
            _ => {
                flush(&mut units, &mut out);
                match esc {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'v' => out.push('\u{b}'),
                    '0' if !chars.peek().is_some_and(char::is_ascii_digit) => out.push('\0'),
                    '0'..='7' => return None,
                    '\n' | '\u{2028}' | '\u{2029}' => {}
                    '\r' => {
                        chars.next_if_eq(&'\n');
                    }
                    other => out.push(other),
                }
            }
        }
    }
    flush(&mut units, &mut out);
    Some(out)
}
