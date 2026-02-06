use super::super::*;

fn is_syntax_character(c: char) -> bool {
    matches!(
        c,
        '^' | '$' | '\\' | '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|'
    )
}

fn is_whitespace_or_line_terminator(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\x0B' | '\x0C' | '\r' | ' ' | '\u{00A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

fn encode_for_regexp_escape(c: char) -> String {
    if is_syntax_character(c) || c == '/' {
        return format!("\\{}", c);
    }
    match c {
        '\t' => "\\t".to_string(),
        '\n' => "\\n".to_string(),
        '\x0B' => "\\v".to_string(),
        '\x0C' => "\\f".to_string(),
        '\r' => "\\r".to_string(),
        _ => {
            let cp = c as u32;
            // ASCII non-alphanumeric non-underscore printable chars
            if (0x20..=0x7E).contains(&cp) && !c.is_ascii_alphanumeric() && c != '_' {
                return format!("\\x{:02x}", cp);
            }
            // Unicode whitespace/line terminators
            if is_whitespace_or_line_terminator(c)
                && !matches!(c, '\t' | '\n' | '\x0B' | '\x0C' | '\r')
            {
                if cp <= 0xFF {
                    return format!("\\x{:02x}", cp);
                }
                return format!("\\u{:04x}", cp);
            }
            c.to_string()
        }
    }
}

fn translate_js_pattern(source: &str, flags: &str) -> String {
    let mut result = String::new();
    if flags.contains('i') {
        result.push_str("(?i)");
    }
    if flags.contains('s') {
        result.push_str("(?s)");
    }
    if flags.contains('m') {
        result.push_str("(?m)");
    }

    let chars: Vec<char> = source.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut in_char_class = false;

    while i < len {
        let c = chars[i];

        if c == '[' && !in_char_class {
            in_char_class = true;
            result.push(c);
            i += 1;
            continue;
        }
        if c == ']' && in_char_class {
            in_char_class = false;
            result.push(c);
            i += 1;
            continue;
        }

        if c == '\\' && i + 1 < len {
            let next = chars[i + 1];
            match next {
                // Named backreference: \k<name> → (?P=name)
                'k' if !in_char_class && i + 2 < len && chars[i + 2] == '<' => {
                    let start = i + 3;
                    if let Some(end) = chars[start..].iter().position(|&c| c == '>') {
                        let name: String = chars[start..start + end].iter().collect();
                        result.push_str(&format!("(?P={})", name));
                        i = start + end + 1;
                        continue;
                    }
                    result.push_str("\\k");
                    i += 2;
                }
                // \0 → null character
                '0' if i + 2 >= len || !chars[i + 2].is_ascii_digit() => {
                    result.push('\0');
                    i += 2;
                }
                // \cX → control character
                'c' if i + 2 < len && chars[i + 2].is_ascii_alphabetic() => {
                    let ctrl = (chars[i + 2] as u8 % 32) as char;
                    result.push(ctrl);
                    i += 3;
                }
                // \xHH → hex escape
                'x' if i + 3 < len
                    && chars[i + 2].is_ascii_hexdigit()
                    && chars[i + 3].is_ascii_hexdigit() =>
                {
                    let hex: String = chars[i + 2..i + 4].iter().collect();
                    if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(cp) {
                            push_literal_char(&mut result, ch, in_char_class);
                        }
                    }
                    i += 4;
                }
                // \uHHHH or \u{HHHH+}
                'u' => {
                    if i + 2 < len && chars[i + 2] == '{' {
                        // \u{HHHH+}
                        let start = i + 3;
                        if let Some(end) = chars[start..].iter().position(|&c| c == '}') {
                            let hex: String = chars[start..start + end].iter().collect();
                            if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                                if let Some(ch) = char::from_u32(cp) {
                                    push_literal_char(&mut result, ch, in_char_class);
                                }
                            }
                            i = start + end + 1;
                        } else {
                            result.push_str("\\u");
                            i += 2;
                        }
                    } else if i + 5 < len
                        && chars[i + 2].is_ascii_hexdigit()
                        && chars[i + 3].is_ascii_hexdigit()
                        && chars[i + 4].is_ascii_hexdigit()
                        && chars[i + 5].is_ascii_hexdigit()
                    {
                        let hex: String = chars[i + 2..i + 6].iter().collect();
                        if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                            if let Some(ch) = char::from_u32(cp) {
                                push_literal_char(&mut result, ch, in_char_class);
                            }
                        }
                        i += 6;
                    } else {
                        result.push_str("\\u");
                        i += 2;
                    }
                }
                // Pass through known regex escapes
                'd' | 'D' | 'w' | 'W' | 's' | 'S' | 'b' | 'B' | 'n' | 'r' | 't' | 'f' | 'v' => {
                    if next == 'v' {
                        result.push('\x0B');
                    } else {
                        result.push('\\');
                        result.push(next);
                    }
                    i += 2;
                }
                // Numeric backreferences
                '1'..='9' => {
                    result.push('\\');
                    result.push(next);
                    i += 2;
                    while i < len && chars[i].is_ascii_digit() {
                        result.push(chars[i]);
                        i += 1;
                    }
                }
                // Pass through other escaped chars
                _ => {
                    result.push('\\');
                    result.push(next);
                    i += 2;
                }
            }
            continue;
        }

        // Named group: (?<name>...) → (?P<name>...)
        if c == '(' && !in_char_class && i + 2 < len && chars[i + 1] == '?' && chars[i + 2] == '<' {
            // Check it's not (?<=...) or (?<!...)
            if i + 3 < len && (chars[i + 3] == '=' || chars[i + 3] == '!') {
                // Lookbehind - pass through
                result.push_str("(?<");
                result.push(chars[i + 3]);
                i += 4;
            } else {
                // Named group
                result.push_str("(?P<");
                i += 3;
            }
            continue;
        }

        result.push(c);
        i += 1;
    }

    result
}

fn push_literal_char(result: &mut String, ch: char, _in_char_class: bool) {
    // Escape regex-special chars when inserting literal
    if is_syntax_character(ch) || ch == '/' {
        result.push('\\');
    }
    result.push(ch);
}

fn build_fancy_regex(source: &str, flags: &str) -> Result<fancy_regex::Regex, String> {
    let pattern = translate_js_pattern(source, flags);
    fancy_regex::Regex::new(&pattern).map_err(|e| e.to_string())
}

enum CompiledRegex {
    Fancy(fancy_regex::Regex),
    Standard(regex::Regex),
}

struct RegexMatch {
    start: usize,
    end: usize,
    text: String,
}

struct RegexCaptures {
    groups: Vec<Option<RegexMatch>>,
    names: Vec<Option<String>>,
}

impl RegexCaptures {
    fn get(&self, i: usize) -> Option<&RegexMatch> {
        self.groups.get(i)?.as_ref()
    }
    fn len(&self) -> usize {
        self.groups.len()
    }
}

fn build_regex(source: &str, flags: &str) -> Result<CompiledRegex, String> {
    let pattern = translate_js_pattern(source, flags);
    match fancy_regex::Regex::new(&pattern) {
        Ok(r) => Ok(CompiledRegex::Fancy(r)),
        Err(_) => {
            // Fallback to standard regex (no backreferences/lookbehind but handles deep nesting)
            regex::Regex::new(&pattern)
                .map(CompiledRegex::Standard)
                .map_err(|e| e.to_string())
        }
    }
}

fn regex_captures(re: &CompiledRegex, text: &str) -> Option<RegexCaptures> {
    match re {
        CompiledRegex::Fancy(r) => {
            let caps = r.captures(text).ok()??;
            let names: Vec<Option<String>> = r
                .capture_names()
                .map(|n| n.map(|s| s.to_string()))
                .collect();
            let mut groups = Vec::new();
            for i in 0..caps.len() {
                groups.push(caps.get(i).map(|m| RegexMatch {
                    start: m.start(),
                    end: m.end(),
                    text: m.as_str().to_string(),
                }));
            }
            Some(RegexCaptures { groups, names })
        }
        CompiledRegex::Standard(r) => {
            let caps = r.captures(text)?;
            let names: Vec<Option<String>> = r
                .capture_names()
                .map(|n| n.map(|s| s.to_string()))
                .collect();
            let mut groups = Vec::new();
            for i in 0..caps.len() {
                groups.push(caps.get(i).map(|m| RegexMatch {
                    start: m.start(),
                    end: m.end(),
                    text: m.as_str().to_string(),
                }));
            }
            Some(RegexCaptures { groups, names })
        }
    }
}

fn regex_is_match(re: &CompiledRegex, text: &str) -> bool {
    match re {
        CompiledRegex::Fancy(r) => r.is_match(text).unwrap_or(false),
        CompiledRegex::Standard(r) => r.is_match(text),
    }
}

fn build_rust_regex(source: &str, flags: &str) -> Result<regex::Regex, String> {
    let mut pattern = String::new();
    if flags.contains('i') {
        pattern.push_str("(?i)");
    }
    if flags.contains('s') {
        pattern.push_str("(?s)");
    }
    if flags.contains('m') {
        pattern.push_str("(?m)");
    }
    pattern.push_str(source);
    regex::Regex::new(&pattern).map_err(|e| e.to_string())
}

fn extract_source_flags(interp: &Interpreter, this_val: &JsValue) -> Option<(String, String, u64)> {
    if let JsValue::Object(o) = this_val
        && let Some(obj) = interp.get_object(o.id)
    {
        let source = if let JsValue::String(s) = obj.borrow().get_property("source") {
            s.to_rust_string()
        } else {
            return None;
        };
        let flags = if let JsValue::String(s) = obj.borrow().get_property("flags") {
            s.to_rust_string()
        } else {
            String::new()
        };
        Some((source, flags, o.id))
    } else {
        None
    }
}

fn get_last_index(interp: &Interpreter, obj_id: u64) -> f64 {
    if let Some(obj) = interp.get_object(obj_id) {
        to_number(&obj.borrow().get_property("lastIndex"))
    } else {
        0.0
    }
}

fn set_last_index(interp: &Interpreter, obj_id: u64, val: f64) {
    if let Some(obj) = interp.get_object(obj_id) {
        obj.borrow_mut()
            .insert_builtin("lastIndex".to_string(), JsValue::Number(val));
    }
}

fn set_last_index_strict(interp: &mut Interpreter, obj_id: u64, val: f64) -> Result<(), JsValue> {
    if let Some(obj) = interp.get_object(obj_id) {
        let success = obj
            .borrow_mut()
            .set_property_value("lastIndex", JsValue::Number(val));
        if !success {
            return Err(
                interp.create_type_error("Cannot set property 'lastIndex' which is not writable"),
            );
        }
    }
    Ok(())
}

fn regexp_exec_abstract(
    interp: &mut Interpreter,
    rx_id: u64,
    s: &str,
) -> Completion {
    let rx_val = JsValue::Object(crate::types::JsObject { id: rx_id });
    let exec_val = match interp.get_object_property(rx_id, "exec", &rx_val) {
        Completion::Normal(v) => v,
        other => return other,
    };
    if interp.is_callable(&exec_val) {
        let result = interp.call_function(
            &exec_val,
            &rx_val,
            &[JsValue::String(JsString::from_str(s))],
        );
        match result {
            Completion::Normal(ref v) => {
                if matches!(v, JsValue::Object(_)) || matches!(v, JsValue::Null) {
                    result
                } else {
                    Completion::Throw(
                        interp.create_type_error("RegExp exec method returned something other than an Object or null"),
                    )
                }
            }
            other => other,
        }
    } else {
        let (source, flags) = if let Some(obj) = interp.get_object(rx_id) {
            let src = if let JsValue::String(s) = obj.borrow().get_property("source") {
                s.to_rust_string()
            } else {
                String::new()
            };
            let fl = if let JsValue::String(s) = obj.borrow().get_property("flags") {
                s.to_rust_string()
            } else {
                String::new()
            };
            (src, fl)
        } else {
            return Completion::Normal(JsValue::Null);
        };
        regexp_exec_raw(interp, rx_id, &source, &flags, s)
    }
}

fn advance_string_index(s: &str, index: usize, unicode: bool) -> usize {
    if !unicode {
        return index + 1;
    }
    if index >= s.len() {
        return index + 1;
    }
    let mut chars = s[index..].chars();
    if let Some(c) = chars.next() {
        index + c.len_utf8()
    } else {
        index + 1
    }
}

fn get_substitution(
    interp: &mut Interpreter,
    matched: &str,
    s: &str,
    position: usize,
    captures: &[JsValue],
    named_captures: &JsValue,
    replacement: &str,
) -> Result<String, JsValue> {
    let tail_pos = position + matched.len();
    let mut result = String::new();
    let bytes = replacement.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let m = captures.len();
    while i < len {
        if bytes[i] == b'$' && i + 1 < len {
            match bytes[i + 1] {
                b'$' => {
                    result.push('$');
                    i += 2;
                }
                b'&' => {
                    result.push_str(matched);
                    i += 2;
                }
                b'`' => {
                    if position > 0 && position <= s.len() {
                        result.push_str(&s[..position]);
                    }
                    i += 2;
                }
                b'\'' => {
                    if tail_pos < s.len() {
                        result.push_str(&s[tail_pos..]);
                    }
                    i += 2;
                }
                c if c.is_ascii_digit() => {
                    let d1 = (c - b'0') as usize;
                    // Try two-digit reference first
                    if i + 2 < len && bytes[i + 2].is_ascii_digit() {
                        let d2 = (bytes[i + 2] - b'0') as usize;
                        let nn = d1 * 10 + d2;
                        if nn >= 1 && nn <= m {
                            let cap = &captures[nn - 1];
                            if !cap.is_undefined() {
                                let cap_s = interp.to_string_value(cap)?;
                                result.push_str(&cap_s);
                            }
                            i += 3;
                            continue;
                        }
                    }
                    // Single-digit reference
                    if d1 >= 1 && d1 <= m {
                        let cap = &captures[d1 - 1];
                        if !cap.is_undefined() {
                            let cap_s = interp.to_string_value(cap)?;
                            result.push_str(&cap_s);
                        }
                    } else {
                        result.push('$');
                        result.push(c as char);
                    }
                    i += 2;
                }
                b'<' => {
                    if matches!(named_captures, JsValue::Undefined) {
                        result.push('$');
                        result.push('<');
                        i += 2;
                    } else {
                        let start = i + 2;
                        if let Some(end_pos) = replacement[start..].find('>') {
                            let group_name = &replacement[start..start + end_pos];
                            let nc_obj = match named_captures {
                                JsValue::Object(o) => o.id,
                                _ => {
                                    result.push('$');
                                    result.push('<');
                                    i += 2;
                                    continue;
                                }
                            };
                            let capture = match interp.get_object_property(
                                nc_obj,
                                group_name,
                                named_captures,
                            ) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Err(e),
                                _ => JsValue::Undefined,
                            };
                            if !capture.is_undefined() {
                                let cap_str = interp.to_string_value(&capture)?;
                                result.push_str(&cap_str);
                            }
                            i = start + end_pos + 1;
                        } else {
                            result.push('$');
                            result.push('<');
                            i += 2;
                        }
                    }
                }
                _ => {
                    result.push('$');
                    i += 1;
                }
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    Ok(result)
}

fn regexp_exec_raw(
    interp: &mut Interpreter,
    this_id: u64,
    source: &str,
    flags: &str,
    input: &str,
) -> Completion {
    let global = flags.contains('g');
    let sticky = flags.contains('y');
    let has_indices = flags.contains('d');

    let last_index = if global || sticky {
        let li = get_last_index(interp, this_id);
        let li_int = li as i64;
        if li_int < 0 || li_int as usize > input.len() {
            if let Err(e) = set_last_index_strict(interp, this_id, 0.0) {
                return Completion::Throw(e);
            }
            return Completion::Normal(JsValue::Null);
        }
        li_int as usize
    } else {
        0
    };

    let re = match build_regex(source, flags) {
        Ok(r) => r,
        Err(_) => return Completion::Normal(JsValue::Null),
    };

    let caps = match regex_captures(&re, &input[last_index..]) {
        Some(c) => c,
        None => {
            if global || sticky {
                if let Err(e) = set_last_index_strict(interp, this_id, 0.0) {
                    return Completion::Throw(e);
                }
            }
            return Completion::Normal(JsValue::Null);
        }
    };

    let full_match = caps.get(0).unwrap();
    let match_start = last_index + full_match.start;
    let match_end = last_index + full_match.end;

    if sticky && full_match.start != 0 {
        if let Err(e) = set_last_index_strict(interp, this_id, 0.0) {
            return Completion::Throw(e);
        }
        return Completion::Normal(JsValue::Null);
    }

    if global || sticky {
        if let Err(e) = set_last_index_strict(interp, this_id, match_end as f64) {
            return Completion::Throw(e);
        }
    }

    let mut elements: Vec<JsValue> = Vec::new();
    elements.push(JsValue::String(JsString::from_str(&full_match.text)));
    for i in 1..caps.len() {
        match caps.get(i) {
            Some(m) => elements.push(JsValue::String(JsString::from_str(&m.text))),
            None => elements.push(JsValue::Undefined),
        }
    }

    let has_named = caps.names.iter().any(|n| n.is_some());
    let groups_val = if has_named {
        let groups_obj = interp.create_object();
        groups_obj.borrow_mut().prototype = None;
        for (i, name_opt) in caps.names.iter().enumerate() {
            if let Some(name) = name_opt {
                let val = match caps.get(i) {
                    Some(m) => JsValue::String(JsString::from_str(&m.text)),
                    None => JsValue::Undefined,
                };
                groups_obj.borrow_mut().insert_value(name.to_string(), val);
            }
        }
        let id = groups_obj.borrow().id.unwrap();
        JsValue::Object(crate::types::JsObject { id })
    } else {
        JsValue::Undefined
    };

    let result = interp.create_array(elements);
    if let JsValue::Object(ref ro) = result
        && let Some(robj) = interp.get_object(ro.id)
    {
        robj.borrow_mut()
            .insert_value("index".to_string(), JsValue::Number(match_start as f64));
        robj.borrow_mut().insert_value(
            "input".to_string(),
            JsValue::String(JsString::from_str(input)),
        );
        robj.borrow_mut()
            .insert_value("groups".to_string(), groups_val.clone());

        if has_indices {
            let mut index_pairs: Vec<JsValue> = Vec::new();
            for i in 0..caps.len() {
                match caps.get(i) {
                    Some(m) => {
                        let pair = interp.create_array(vec![
                            JsValue::Number((last_index + m.start) as f64),
                            JsValue::Number((last_index + m.end) as f64),
                        ]);
                        index_pairs.push(pair);
                    }
                    None => index_pairs.push(JsValue::Undefined),
                }
            }
            let indices_arr = interp.create_array(index_pairs);
            if has_named {
                let idx_groups = interp.create_object();
                idx_groups.borrow_mut().prototype = None;
                for (i, name_opt) in caps.names.iter().enumerate() {
                    if let Some(name) = name_opt {
                        let val = match caps.get(i) {
                            Some(m) => interp.create_array(vec![
                                JsValue::Number((last_index + m.start) as f64),
                                JsValue::Number((last_index + m.end) as f64),
                            ]),
                            None => JsValue::Undefined,
                        };
                        idx_groups.borrow_mut().insert_value(name.to_string(), val);
                    }
                }
                let idx_groups_id = idx_groups.borrow().id.unwrap();
                if let JsValue::Object(ref io) = indices_arr
                    && let Some(iobj) = interp.get_object(io.id)
                {
                    iobj.borrow_mut().insert_value(
                        "groups".to_string(),
                        JsValue::Object(crate::types::JsObject { id: idx_groups_id }),
                    );
                }
            } else if let JsValue::Object(ref io) = indices_arr
                && let Some(iobj) = interp.get_object(io.id)
            {
                iobj.borrow_mut()
                    .insert_value("groups".to_string(), JsValue::Undefined);
            }
            robj.borrow_mut()
                .insert_value("indices".to_string(), indices_arr);
        }
    }
    Completion::Normal(result)
}

fn get_symbol_key(interp: &Interpreter, name: &str) -> Option<String> {
    interp.global_env.borrow().get("Symbol").and_then(|sv| {
        if let JsValue::Object(so) = sv {
            interp.get_object(so.id).map(|sobj| {
                let val = sobj.borrow().get_property(name);
                to_js_string(&val)
            })
        } else {
            None
        }
    })
}


impl Interpreter {
    pub(crate) fn setup_regexp(&mut self) {
        let regexp_proto = self.create_object();
        regexp_proto.borrow_mut().class_name = "RegExp".to_string();

        // RegExp.prototype.exec
        let exec_fn = self.create_function(JsFunction::native(
            "exec".to_string(),
            1,
            |interp, this_val, args| {
                if !matches!(this_val, JsValue::Object(_)) {
                    let err = interp.create_type_error(
                        "RegExp.prototype.exec requires that 'this' be an Object",
                    );
                    return Completion::Throw(err);
                }
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let input = match interp.to_string_value(&arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                if let Some((source, flags, obj_id)) = extract_source_flags(interp, this_val) {
                    return regexp_exec_raw(interp, obj_id, &source, &flags, &input);
                }
                Completion::Normal(JsValue::Null)
            },
        ));
        regexp_proto
            .borrow_mut()
            .insert_builtin("exec".to_string(), exec_fn);

        // RegExp.prototype.test
        let test_fn = self.create_function(JsFunction::native(
            "test".to_string(),
            1,
            |interp, this_val, args| {
                if !matches!(this_val, JsValue::Object(_)) {
                    let err = interp.create_type_error(
                        "RegExp.prototype.test requires that 'this' be an Object",
                    );
                    return Completion::Throw(err);
                }
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let input = match interp.to_string_value(&arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                if let Some((source, flags, obj_id)) = extract_source_flags(interp, this_val) {
                    let result = regexp_exec_raw(interp, obj_id, &source, &flags, &input);
                    return match result {
                        Completion::Normal(v) => {
                            Completion::Normal(JsValue::Boolean(!matches!(v, JsValue::Null)))
                        }
                        other => other,
                    };
                }
                Completion::Normal(JsValue::Boolean(false))
            },
        ));
        regexp_proto
            .borrow_mut()
            .insert_builtin("test".to_string(), test_fn);

        // RegExp.prototype.toString
        let tostring_fn = self.create_function(JsFunction::native(
            "toString".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let source = if let JsValue::String(s) = obj.borrow().get_property("source") {
                        s.to_rust_string()
                    } else {
                        String::new()
                    };
                    let flags = if let JsValue::String(s) = obj.borrow().get_property("flags") {
                        s.to_rust_string()
                    } else {
                        String::new()
                    };
                    return Completion::Normal(JsValue::String(JsString::from_str(&format!(
                        "/{}/{}",
                        source, flags
                    ))));
                }
                Completion::Normal(JsValue::String(JsString::from_str("/(?:)/")))
            },
        ));
        regexp_proto
            .borrow_mut()
            .insert_builtin("toString".to_string(), tostring_fn);

        // [@@match] (§22.2.5.6)
        let match_fn = self.create_function(JsFunction::native(
            "[Symbol.match]".to_string(),
            1,
            |interp, this_val, args| {
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let s = match interp.to_string_value(&arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let (source, flags, obj_id) = match extract_source_flags(interp, this_val) {
                    Some(v) => v,
                    None => return Completion::Normal(JsValue::Null),
                };
                let global = flags.contains('g');
                if !global {
                    return regexp_exec_raw(interp, obj_id, &source, &flags, &s);
                }
                // Global: collect all matches
                set_last_index(interp, obj_id, 0.0);
                let mut results: Vec<JsValue> = Vec::new();
                loop {
                    let result = regexp_exec_raw(interp, obj_id, &source, &flags, &s);
                    match result {
                        Completion::Normal(JsValue::Null) => break,
                        Completion::Normal(JsValue::Object(ref o)) => {
                            if let Some(arr) = interp.get_object(o.id) {
                                let matched = arr.borrow().get_property("0");
                                let match_str = to_js_string(&matched);
                                results.push(matched);
                                if match_str.is_empty() {
                                    let li = get_last_index(interp, obj_id);
                                    set_last_index(interp, obj_id, li + 1.0);
                                }
                            } else {
                                break;
                            }
                        }
                        other => return other,
                    }
                }
                if results.is_empty() {
                    Completion::Normal(JsValue::Null)
                } else {
                    Completion::Normal(interp.create_array(results))
                }
            },
        ));
        if let Some(key) = get_symbol_key(self, "match") {
            regexp_proto
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(match_fn, true, false, true));
        }

        // [@@search] (§22.2.5.9)
        let search_fn = self.create_function(JsFunction::native(
            "[Symbol.search]".to_string(),
            1,
            |interp, this_val, args| {
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let s = match interp.to_string_value(&arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let (source, flags, obj_id) = match extract_source_flags(interp, this_val) {
                    Some(v) => v,
                    None => return Completion::Normal(JsValue::Number(-1.0)),
                };
                let prev_last_index = get_last_index(interp, obj_id);
                set_last_index(interp, obj_id, 0.0);
                let result = regexp_exec_raw(interp, obj_id, &source, &flags, &s);
                set_last_index(interp, obj_id, prev_last_index);
                match result {
                    Completion::Normal(JsValue::Null) => Completion::Normal(JsValue::Number(-1.0)),
                    Completion::Normal(JsValue::Object(ref o)) => {
                        if let Some(obj) = interp.get_object(o.id) {
                            let idx = obj.borrow().get_property("index");
                            Completion::Normal(idx)
                        } else {
                            Completion::Normal(JsValue::Number(-1.0))
                        }
                    }
                    other => other,
                }
            },
        ));
        if let Some(key) = get_symbol_key(self, "search") {
            regexp_proto
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(search_fn, true, false, true));
        }

        // [@@replace] (§22.2.5.8)
        let replace_fn = self.create_function(JsFunction::native(
            "[Symbol.replace]".to_string(),
            2,
            |interp, this_val, args| {
                // 1. Let rx be the this value.
                // 2. If Type(rx) is not Object, throw a TypeError.
                let rx_id = match this_val {
                    JsValue::Object(o) => o.id,
                    _ => {
                        let err = interp.create_type_error(
                            "RegExp.prototype[@@replace] requires that 'this' be an Object",
                        );
                        return Completion::Throw(err);
                    }
                };

                // 3. Let S be ? ToString(string).
                let string_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let s = match interp.to_string_value(&string_arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let length_s = s.len();

                // 5. Let functionalReplace be IsCallable(replaceValue).
                let replace_value = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let functional_replace = interp.is_callable(&replace_value);

                // 6. If functionalReplace is false, set replaceValue to ? ToString(replaceValue).
                let replace_str = if !functional_replace {
                    match interp.to_string_value(&replace_value) {
                        Ok(s) => Some(s),
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    None
                };

                // 7. Let flags be ? ToString(? Get(rx, "flags")).
                let rx_val = JsValue::Object(crate::types::JsObject { id: rx_id });
                let flags_val = match interp.get_object_property(rx_id, "flags", &rx_val) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let flags = match interp.to_string_value(&flags_val) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };

                // 8. Let global be (flags contains "g").
                let global = flags.contains('g');
                let mut full_unicode = false;

                // 9. If global is true, then
                if global {
                    // a. Let fullUnicode be (flags contains "u" or "v").
                    full_unicode = flags.contains('u') || flags.contains('v');
                    // b. Perform ? Set(rx, "lastIndex", +0𝔽, true).
                    match set_last_index_strict(interp, rx_id, 0.0) {
                        Ok(()) => {}
                        Err(e) => return Completion::Throw(e),
                    }
                }

                // 10-11. Collect results
                let mut results: Vec<JsValue> = Vec::new();
                loop {
                    // 11a. Let result be ? RegExpExec(rx, S).
                    let result = regexp_exec_abstract(interp, rx_id, &s);
                    match result {
                        Completion::Normal(JsValue::Null) => break,
                        Completion::Normal(ref result_val) if matches!(result_val, JsValue::Object(_)) => {
                            let result_obj = result_val.clone();
                            results.push(result_obj.clone());

                            if !global {
                                break;
                            }

                            // For global: check if match is empty and advance
                            let result_id = if let JsValue::Object(ref o) = result_obj { o.id } else { unreachable!() };
                            let matched_val = match interp.get_object_property(result_id, "0", &result_obj) {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                            let match_str = match interp.to_string_value(&matched_val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            if match_str.is_empty() {
                                // a. Let thisIndex be ? ToLength(? Get(rx, "lastIndex")).
                                let rx_val = JsValue::Object(crate::types::JsObject { id: rx_id });
                                let li_val = match interp.get_object_property(rx_id, "lastIndex", &rx_val) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                };
                                let li_num = match interp.to_number_value(&li_val) {
                                    Ok(n) => n,
                                    Err(e) => return Completion::Throw(e),
                                };
                                let this_index = {
                                    let n = if li_num.is_nan() || li_num <= 0.0 { 0.0 } else { li_num.min(9007199254740991.0).floor() };
                                    n as usize
                                };
                                let next_index = advance_string_index(&s, this_index, full_unicode);
                                match set_last_index_strict(interp, rx_id, next_index as f64) {
                                    Ok(()) => {}
                                    Err(e) => return Completion::Throw(e),
                                }
                            }
                        }
                        Completion::Normal(_) => break,
                        other => return other,
                    }
                }

                // 14. For each element result of results, do
                let mut accumulated_result = String::new();
                let mut next_source_position: usize = 0;

                for result_val in &results {
                    let result_id = if let JsValue::Object(o) = result_val { o.id } else { continue };

                    // a. Let nCaptures be ? ToLength(? Get(result, "length")).
                    let len_val = match interp.get_object_property(result_id, "length", result_val) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let n_captures = {
                        let n = match interp.to_number_value(&len_val) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        };
                        let len = if n.is_nan() || n <= 0.0 { 0.0 } else { n.min(9007199254740991.0).floor() };
                        (len as usize).max(1) // at least 1
                    };
                    // nCaptures = max(nCaptures - 1, 0) — number of capture groups
                    let n_cap = if n_captures > 0 { n_captures - 1 } else { 0 };

                    // d. Let matched be ? ToString(? Get(result, "0")).
                    let matched_val = match interp.get_object_property(result_id, "0", result_val) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let matched = match interp.to_string_value(&matched_val) {
                        Ok(s) => s,
                        Err(e) => return Completion::Throw(e),
                    };
                    let match_length = matched.len();

                    // e. Let position be ? ToIntegerOrInfinity(? Get(result, "index")).
                    let index_val = match interp.get_object_property(result_id, "index", result_val) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let position = {
                        let n = match interp.to_number_value(&index_val) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        };
                        let int = to_integer_or_infinity(n);
                        int.max(0.0).min(length_s as f64) as usize
                    };

                    // g-i. Get captures
                    let mut captures: Vec<JsValue> = Vec::new();
                    for n in 1..=n_cap {
                        let cap_n = match interp.get_object_property(result_id, &n.to_string(), result_val) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        if !cap_n.is_undefined() {
                            let cap_str = match interp.to_string_value(&cap_n) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            captures.push(JsValue::String(JsString::from_str(&cap_str)));
                        } else {
                            captures.push(JsValue::Undefined);
                        }
                    }

                    // j. Let namedCaptures be ? Get(result, "groups").
                    let named_captures = match interp.get_object_property(result_id, "groups", result_val) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };

                    let replacement = if functional_replace {
                        // k. If functionalReplace is true, then
                        let mut replacer_args: Vec<JsValue> = Vec::new();
                        replacer_args.push(JsValue::String(JsString::from_str(&matched)));
                        for cap in &captures {
                            replacer_args.push(cap.clone());
                        }
                        replacer_args.push(JsValue::Number(position as f64));
                        replacer_args.push(JsValue::String(JsString::from_str(&s)));
                        if !named_captures.is_undefined() {
                            replacer_args.push(named_captures.clone());
                        }
                        let repl_val = interp.call_function(
                            &replace_value,
                            &JsValue::Undefined,
                            &replacer_args,
                        );
                        match repl_val {
                            Completion::Normal(v) => {
                                match interp.to_string_value(&v) {
                                    Ok(s) => s,
                                    Err(e) => return Completion::Throw(e),
                                }
                            }
                            other => return other,
                        }
                    } else {
                        // l. Else (string replace)
                        let template = replace_str.as_ref().unwrap();
                        let named_captures_obj = if !named_captures.is_undefined() {
                            // i. Set namedCaptures to ? ToObject(namedCaptures).
                            match interp.to_object(&named_captures) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => JsValue::Undefined,
                            }
                        } else {
                            JsValue::Undefined
                        };
                        match get_substitution(
                            interp,
                            &matched,
                            &s,
                            position,
                            &captures,
                            &named_captures_obj,
                            template,
                        ) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        }
                    };

                    // p. If position >= nextSourcePosition, then
                    if position >= next_source_position {
                        accumulated_result.push_str(&s[next_source_position..position]);
                        accumulated_result.push_str(&replacement);
                        next_source_position = position + match_length;
                    }
                }

                // 15. Return accumulatedResult + remainder of S.
                if next_source_position < length_s {
                    accumulated_result.push_str(&s[next_source_position..]);
                }
                Completion::Normal(JsValue::String(JsString::from_str(&accumulated_result)))
            },
        ));
        if let Some(key) = get_symbol_key(self, "replace") {
            regexp_proto
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(replace_fn, true, false, true));
        }

        // [@@split] (§22.2.5.11)
        let split_fn = self.create_function(JsFunction::native(
            "[Symbol.split]".to_string(),
            2,
            |interp, this_val, args| {
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let s = match interp.to_string_value(&arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let limit = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let lim = if matches!(limit, JsValue::Undefined) {
                    u32::MAX
                } else {
                    interp.to_number_coerce(&limit) as u32
                };
                let (source, flags, _obj_id) = match extract_source_flags(interp, this_val) {
                    Some(v) => v,
                    None => {
                        return Completion::Normal(
                            interp.create_array(vec![JsValue::String(JsString::from_str(&s))]),
                        );
                    }
                };
                if lim == 0 {
                    return Completion::Normal(interp.create_array(vec![]));
                }

                if s.is_empty() {
                    let re = match build_regex(&source, &flags) {
                        Ok(r) => r,
                        Err(_) => {
                            return Completion::Normal(
                                interp.create_array(vec![JsValue::String(JsString::from_str(""))]),
                            );
                        }
                    };
                    if regex_is_match(&re, "") {
                        return Completion::Normal(interp.create_array(vec![]));
                    } else {
                        return Completion::Normal(
                            interp.create_array(vec![JsValue::String(JsString::from_str(""))]),
                        );
                    }
                }

                let re = match build_regex(&source, &flags) {
                    Ok(r) => r,
                    Err(_) => {
                        return Completion::Normal(
                            interp.create_array(vec![JsValue::String(JsString::from_str(&s))]),
                        );
                    }
                };

                let mut result: Vec<JsValue> = Vec::new();
                let mut last_end = 0;
                let mut search_start = 0;

                while search_start <= s.len() {
                    let caps = match regex_captures(&re, &s[search_start..]) {
                        Some(c) => c,
                        None => break,
                    };
                    let full = caps.get(0).unwrap();
                    let abs_start = search_start + full.start;
                    let abs_end = search_start + full.end;
                    if abs_start == abs_end && abs_start == last_end {
                        search_start += 1;
                        continue;
                    }
                    result.push(JsValue::String(JsString::from_str(&s[last_end..abs_start])));
                    if result.len() as u32 >= lim {
                        return Completion::Normal(interp.create_array(result));
                    }
                    for i in 1..caps.len() {
                        match caps.get(i) {
                            Some(m) => result.push(JsValue::String(JsString::from_str(&m.text))),
                            None => result.push(JsValue::Undefined),
                        }
                        if result.len() as u32 >= lim {
                            return Completion::Normal(interp.create_array(result));
                        }
                    }
                    last_end = abs_end;
                    search_start = if abs_start == abs_end {
                        abs_end + 1
                    } else {
                        abs_end
                    };
                }
                result.push(JsValue::String(JsString::from_str(&s[last_end..])));
                Completion::Normal(interp.create_array(result))
            },
        ));
        if let Some(key) = get_symbol_key(self, "split") {
            regexp_proto
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(split_fn, true, false, true));
        }

        // [@@matchAll] (§22.2.5.7)
        let match_all_fn = self.create_function(JsFunction::native(
            "[Symbol.matchAll]".to_string(),
            1,
            |interp, this_val, args| {
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let s = match interp.to_string_value(&arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let (source, flags, obj_id) = match extract_source_flags(interp, this_val) {
                    Some(v) => v,
                    None => {
                        let err = interp.create_type_error("RegExp expected");
                        return Completion::Throw(err);
                    }
                };
                let global = flags.contains('g');
                let last_index = get_last_index(interp, obj_id);

                // Create iterator
                let iter_obj = interp.create_object();
                iter_obj.borrow_mut().class_name = "RegExp String Iterator".to_string();
                if let Some(ref ip) = interp.iterator_prototype {
                    iter_obj.borrow_mut().prototype = Some(ip.clone());
                }

                iter_obj.borrow_mut().iterator_state = Some(IteratorState::RegExpStringIterator {
                    source,
                    flags,
                    string: s,
                    global,
                    last_index: last_index as usize,
                    done: false,
                });

                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    |interp, this_val, _args| {
                        if let JsValue::Object(o) = this_val
                            && let Some(obj) = interp.get_object(o.id)
                        {
                            let state = obj.borrow().iterator_state.clone();
                            if let Some(IteratorState::RegExpStringIterator {
                                ref source,
                                ref flags,
                                ref string,
                                global,
                                last_index,
                                done,
                            }) = state
                            {
                                if done {
                                    return Completion::Normal(
                                        interp.create_iter_result_object(JsValue::Undefined, true),
                                    );
                                }

                                let re = match build_regex(source, flags) {
                                    Ok(r) => r,
                                    Err(_) => {
                                        obj.borrow_mut().iterator_state =
                                            Some(IteratorState::RegExpStringIterator {
                                                source: source.clone(),
                                                flags: flags.clone(),
                                                string: string.clone(),
                                                global,
                                                last_index,
                                                done: true,
                                            });
                                        return Completion::Normal(
                                            interp.create_iter_result_object(
                                                JsValue::Undefined,
                                                true,
                                            ),
                                        );
                                    }
                                };

                                if last_index > string.len() {
                                    obj.borrow_mut().iterator_state =
                                        Some(IteratorState::RegExpStringIterator {
                                            source: source.clone(),
                                            flags: flags.clone(),
                                            string: string.clone(),
                                            global,
                                            last_index,
                                            done: true,
                                        });
                                    return Completion::Normal(
                                        interp.create_iter_result_object(JsValue::Undefined, true),
                                    );
                                }

                                match regex_captures(&re, &string[last_index..]) {
                                    None => {
                                        obj.borrow_mut().iterator_state =
                                            Some(IteratorState::RegExpStringIterator {
                                                source: source.clone(),
                                                flags: flags.clone(),
                                                string: string.clone(),
                                                global,
                                                last_index,
                                                done: true,
                                            });
                                        Completion::Normal(
                                            interp.create_iter_result_object(
                                                JsValue::Undefined,
                                                true,
                                            ),
                                        )
                                    }
                                    Some(caps) => {
                                        let full = caps.get(0).unwrap();
                                        let match_start = last_index + full.start;
                                        let match_end = last_index + full.end;

                                        let mut elements: Vec<JsValue> = Vec::new();
                                        elements
                                            .push(JsValue::String(JsString::from_str(&full.text)));
                                        for i in 1..caps.len() {
                                            match caps.get(i) {
                                                Some(m) => elements.push(JsValue::String(
                                                    JsString::from_str(&m.text),
                                                )),
                                                None => elements.push(JsValue::Undefined),
                                            }
                                        }

                                        let result_arr = interp.create_array(elements);
                                        if let JsValue::Object(ref ro) = result_arr
                                            && let Some(robj) = interp.get_object(ro.id)
                                        {
                                            robj.borrow_mut().insert_value(
                                                "index".to_string(),
                                                JsValue::Number(match_start as f64),
                                            );
                                            robj.borrow_mut().insert_value(
                                                "input".to_string(),
                                                JsValue::String(JsString::from_str(string)),
                                            );
                                            robj.borrow_mut().insert_value(
                                                "groups".to_string(),
                                                JsValue::Undefined,
                                            );
                                        }

                                        let new_last_index = if global {
                                            if full.text.is_empty() {
                                                match_end + 1
                                            } else {
                                                match_end
                                            }
                                        } else {
                                            last_index
                                        };

                                        let new_done = !global;

                                        obj.borrow_mut().iterator_state =
                                            Some(IteratorState::RegExpStringIterator {
                                                source: source.clone(),
                                                flags: flags.clone(),
                                                string: string.clone(),
                                                global,
                                                last_index: new_last_index,
                                                done: new_done,
                                            });

                                        Completion::Normal(
                                            interp.create_iter_result_object(result_arr, false),
                                        )
                                    }
                                }
                            } else {
                                Completion::Normal(
                                    interp.create_iter_result_object(JsValue::Undefined, true),
                                )
                            }
                        } else {
                            Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            )
                        }
                    },
                ));
                iter_obj
                    .borrow_mut()
                    .insert_builtin("next".to_string(), next_fn);

                // @@iterator returns self
                let iter_self_fn = interp.create_function(JsFunction::native(
                    "[Symbol.iterator]".to_string(),
                    0,
                    |_interp, this, _args| Completion::Normal(this.clone()),
                ));
                if let Some(key) = interp.get_symbol_iterator_key() {
                    iter_obj.borrow_mut().insert_property(
                        key,
                        PropertyDescriptor::data(iter_self_fn, true, false, true),
                    );
                }

                let id = iter_obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
            },
        ));
        if let Some(key) = get_symbol_key(self, "matchAll") {
            regexp_proto.borrow_mut().insert_property(
                key,
                PropertyDescriptor::data(match_all_fn, true, false, true),
            );
        }

        // RegExp.prototype.flags getter (§22.2.5.3)
        let flags_getter = self.create_function(JsFunction::native(
            "get flags".to_string(),
            0,
            |interp, this_val, _args| {
                let obj_id = match this_val {
                    JsValue::Object(o) => o.id,
                    _ => {
                        let err = interp.create_type_error(
                            "RegExp.prototype.flags requires that 'this' be an Object",
                        );
                        return Completion::Throw(err);
                    }
                };
                if interp.get_object(obj_id).is_none() {
                    let err = interp.create_type_error(
                        "RegExp.prototype.flags requires that 'this' be an Object",
                    );
                    return Completion::Throw(err);
                }
                let mut result = String::new();
                let flags_to_check: &[(&str, char)] = &[
                    ("hasIndices", 'd'),
                    ("global", 'g'),
                    ("ignoreCase", 'i'),
                    ("multiline", 'm'),
                    ("dotAll", 's'),
                    ("unicode", 'u'),
                    ("unicodeSets", 'v'),
                    ("sticky", 'y'),
                ];
                for (prop, ch) in flags_to_check {
                    let val = match interp.get_object_property(obj_id, prop, this_val) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if to_boolean(&val) {
                        result.push(*ch);
                    }
                }
                Completion::Normal(JsValue::String(JsString::from_str(&result)))
            },
        ));
        regexp_proto.borrow_mut().insert_property(
            "flags".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(flags_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );

        // Flag property getters on prototype (§22.2.5.x)
        let flag_props: &[(&str, char)] = &[
            ("global", 'g'),
            ("ignoreCase", 'i'),
            ("multiline", 'm'),
            ("dotAll", 's'),
            ("unicode", 'u'),
            ("unicodeSets", 'v'),
            ("sticky", 'y'),
            ("hasIndices", 'd'),
        ];
        let regexp_proto_id = regexp_proto.borrow().id.unwrap();
        for &(prop_name, flag_char) in flag_props {
            let name = prop_name.to_string();
            let getter = self.create_function(JsFunction::native(
                format!("get {}", name),
                0,
                move |interp, this_val, _args| {
                    let obj_ref = match this_val {
                        JsValue::Object(o) => o,
                        _ => {
                            return Completion::Throw(interp.create_type_error(&format!(
                                "RegExp.prototype.{} requires that 'this' be an Object",
                                name
                            )));
                        }
                    };
                    let obj = match interp.get_object(obj_ref.id) {
                        Some(o) => o,
                        None => return Completion::Normal(JsValue::Undefined),
                    };
                    // Check if this has [[OriginalFlags]] (is a RegExp)
                    if obj.borrow().class_name != "RegExp" {
                        // If this is RegExp.prototype itself, return undefined
                        if obj_ref.id == regexp_proto_id {
                            return Completion::Normal(JsValue::Undefined);
                        }
                        return Completion::Throw(interp.create_type_error(&format!(
                            "RegExp.prototype.{} requires that 'this' be a RegExp object",
                            name
                        )));
                    }
                    let flags_val = obj.borrow().get_property("flags");
                    if let JsValue::String(s) = flags_val {
                        Completion::Normal(JsValue::Boolean(s.to_rust_string().contains(flag_char)))
                    } else {
                        Completion::Normal(JsValue::Undefined)
                    }
                },
            ));
            regexp_proto.borrow_mut().insert_property(
                prop_name.to_string(),
                PropertyDescriptor {
                    value: None,
                    writable: None,
                    get: Some(getter),
                    set: None,
                    enumerable: Some(false),
                    configurable: Some(true),
                },
            );
        }

        // source getter (§22.2.5.10)
        let source_getter = self.create_function(JsFunction::native(
            "get source".to_string(),
            0,
            |interp, this_val, _args| {
                let obj_ref = match this_val {
                    JsValue::Object(o) => o,
                    _ => {
                        let err = interp.create_type_error(
                            "RegExp.prototype.source requires that 'this' be an Object",
                        );
                        return Completion::Throw(err);
                    }
                };
                let obj = match interp.get_object(obj_ref.id) {
                    Some(o) => o,
                    None => return Completion::Normal(JsValue::Undefined),
                };
                let source_val = obj.borrow().get_property("source");
                if let JsValue::String(_) = source_val {
                    Completion::Normal(source_val)
                } else {
                    Completion::Normal(JsValue::String(JsString::from_str("(?:)")))
                }
            },
        ));
        regexp_proto.borrow_mut().insert_property(
            "source".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(source_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );

        let regexp_proto_rc = regexp_proto.clone();

        // RegExp constructor
        let regexp_ctor = self.create_function(JsFunction::constructor(
            "RegExp".to_string(),
            2,
            move |interp, _this, args| {
                let pattern_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let flags_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);

                // Handle RegExp argument: extract source/flags from existing RegExp
                let (pattern_str, flags_str) = if let JsValue::Object(ref o) = pattern_arg
                    && let Some(obj) = interp.get_object(o.id)
                    && obj.borrow().class_name == "RegExp"
                {
                    let src = if let JsValue::String(s) = obj.borrow().get_property("source") {
                        s.to_rust_string()
                    } else {
                        String::new()
                    };
                    let flg = if matches!(flags_arg, JsValue::Undefined) {
                        if let JsValue::String(s) = obj.borrow().get_property("flags") {
                            s.to_rust_string()
                        } else {
                            String::new()
                        }
                    } else {
                        match interp.to_string_value(&flags_arg) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        }
                    };
                    (src, flg)
                } else {
                    let p = if matches!(pattern_arg, JsValue::Undefined) {
                        String::new()
                    } else {
                        match interp.to_string_value(&pattern_arg) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        }
                    };
                    let f = if matches!(flags_arg, JsValue::Undefined) {
                        String::new()
                    } else {
                        match interp.to_string_value(&flags_arg) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        }
                    };
                    (p, f)
                };

                // Validate flags: only dgimsuy allowed, no duplicates
                let valid_flags = "dgimsuyv";
                for c in flags_str.chars() {
                    if !valid_flags.contains(c) {
                        return Completion::Throw(
                            interp.create_error("SyntaxError", &format!(
                                "Invalid regular expression flags '{}'", flags_str
                            )),
                        );
                    }
                }
                let mut seen = std::collections::HashSet::new();
                for c in flags_str.chars() {
                    if !seen.insert(c) {
                        return Completion::Throw(
                            interp.create_error("SyntaxError", &format!(
                                "Invalid regular expression flags '{}'", flags_str
                            )),
                        );
                    }
                }

                // u and v flags are mutually exclusive
                if flags_str.contains('u') && flags_str.contains('v') {
                    return Completion::Throw(
                        interp.create_error("SyntaxError", &format!(
                            "Invalid regular expression flags '{}'", flags_str
                        )),
                    );
                }

                let mut obj = JsObjectData::new();
                obj.prototype = Some(regexp_proto_rc.clone());
                obj.class_name = "RegExp".to_string();
                let regexp_props: &[(&str, JsValue)] = &[
                    ("source", JsValue::String(JsString::from_str(&pattern_str))),
                    ("flags", JsValue::String(JsString::from_str(&flags_str))),
                    ("hasIndices", JsValue::Boolean(flags_str.contains('d'))),
                    ("global", JsValue::Boolean(flags_str.contains('g'))),
                    ("ignoreCase", JsValue::Boolean(flags_str.contains('i'))),
                    ("multiline", JsValue::Boolean(flags_str.contains('m'))),
                    ("dotAll", JsValue::Boolean(flags_str.contains('s'))),
                    ("unicode", JsValue::Boolean(flags_str.contains('u'))),
                    ("sticky", JsValue::Boolean(flags_str.contains('y'))),
                ];
                for (name, val) in regexp_props {
                    obj.insert_property(
                        name.to_string(),
                        PropertyDescriptor::data(val.clone(), false, false, false),
                    );
                }
                obj.insert_property(
                    "lastIndex".to_string(),
                    PropertyDescriptor::data(JsValue::Number(0.0), true, false, false),
                );
                let rc = Rc::new(RefCell::new(obj));
                let id = interp.allocate_object_slot(rc);
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
            },
        ));
        // RegExp.escape (§22.2.5.1)
        let escape_fn = self.create_function(JsFunction::native(
            "escape".to_string(),
            1,
            |interp, _this, args| {
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(arg, JsValue::String(_)) {
                    let err = interp.create_type_error("RegExp.escape requires a string argument");
                    return Completion::Throw(err);
                }
                let s = match interp.to_string_value(&arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let mut result = String::new();
                for (i, c) in s.chars().enumerate() {
                    if i == 0 && (c.is_ascii_alphanumeric()) {
                        result.push_str(&format!("\\x{:02x}", c as u32));
                    } else {
                        result.push_str(&encode_for_regexp_escape(c));
                    }
                }
                Completion::Normal(JsValue::String(JsString::from_str(&result)))
            },
        ));

        if let JsValue::Object(ref o) = regexp_ctor
            && let Some(obj) = self.get_object(o.id)
        {
            obj.borrow_mut().insert_builtin(
                "prototype".to_string(),
                JsValue::Object(crate::types::JsObject {
                    id: regexp_proto.borrow().id.unwrap(),
                }),
            );
            obj.borrow_mut()
                .insert_builtin("escape".to_string(), escape_fn);

            // RegExp[Symbol.species] getter
            let species_getter = self.create_function(JsFunction::native(
                "get [Symbol.species]".to_string(),
                0,
                |_interp, this_val, _args| Completion::Normal(this_val.clone()),
            ));
            obj.borrow_mut().insert_property(
                "Symbol(Symbol.species)".to_string(),
                PropertyDescriptor {
                    value: None,
                    writable: None,
                    get: Some(species_getter),
                    set: None,
                    enumerable: Some(false),
                    configurable: Some(true),
                },
            );

            regexp_proto
                .borrow_mut()
                .insert_builtin("constructor".to_string(), regexp_ctor.clone());
        }
        self.global_env
            .borrow_mut()
            .declare("RegExp", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("RegExp", regexp_ctor);

        self.regexp_prototype = Some(regexp_proto);
    }

    pub(crate) fn get_symbol_key(&self, name: &str) -> Option<String> {
        get_symbol_key(self, name)
    }
}
