use super::super::*;

fn is_ecma_whitespace(ch: char) -> bool {
    matches!(
        ch,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{FEFF}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
    )
}

// §22.1.3 thisStringValue — RequireObjectCoercible + extract primitive from String wrapper
fn this_string_value(interp: &mut Interpreter, this: &JsValue) -> Result<String, Completion> {
    match this {
        JsValue::Null | JsValue::Undefined => Err(Completion::Throw(
            interp.create_type_error("String.prototype method called on null or undefined"),
        )),
        JsValue::String(s) => Ok(s.to_rust_string()),
        JsValue::Object(o) => {
            if let Some(obj) = interp.get_object(o.id)
                && obj.borrow().class_name == "String"
                && let Some(JsValue::String(s)) = &obj.borrow().primitive_value
            {
                return Ok(s.to_rust_string());
            }
            match interp.to_string_value(this) {
                Ok(s) => Ok(s),
                Err(e) => Err(Completion::Throw(e)),
            }
        }
        _ => match interp.to_string_value(this) {
            Ok(s) => Ok(s),
            Err(e) => Err(Completion::Throw(e)),
        },
    }
}

fn to_str(interp: &mut Interpreter, val: &JsValue) -> Result<String, Completion> {
    match interp.to_string_value(val) {
        Ok(s) => Ok(s),
        Err(e) => Err(Completion::Throw(e)),
    }
}

fn to_num(interp: &mut Interpreter, val: &JsValue) -> Result<f64, Completion> {
    match interp.to_number_value(val) {
        Ok(n) => Ok(n),
        Err(e) => Err(Completion::Throw(e)),
    }
}

#[allow(dead_code)]
fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

fn utf16_units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn utf16_substring(units: &[u16], from: usize, to: usize) -> String {
    let from = from.min(units.len());
    let to = to.min(units.len());
    if from >= to {
        return String::new();
    }
    String::from_utf16_lossy(&units[from..to])
}

impl Interpreter {
    pub(crate) fn setup_string_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "String".to_string();

        #[allow(clippy::type_complexity)]
        let methods: Vec<(
            &str,
            usize,
            Rc<dyn Fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion>,
        )> = vec![
            (
                "charAt",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let pos = match args.first() {
                        Some(v) => match to_num(interp, v) {
                            Ok(n) => to_integer_or_infinity(n),
                            Err(c) => return c,
                        },
                        None => 0.0,
                    };
                    let units = utf16_units(&s);
                    let idx = pos as isize;
                    if idx < 0 || idx as usize >= units.len() {
                        return Completion::Normal(JsValue::String(JsString::from_str("")));
                    }
                    let ch = String::from_utf16_lossy(&units[idx as usize..idx as usize + 1]);
                    Completion::Normal(JsValue::String(JsString::from_str(&ch)))
                }),
            ),
            (
                "charCodeAt",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let pos = match args.first() {
                        Some(v) => match to_num(interp, v) {
                            Ok(n) => to_integer_or_infinity(n),
                            Err(c) => return c,
                        },
                        None => 0.0,
                    };
                    let units = utf16_units(&s);
                    let idx = pos as isize;
                    if idx < 0 || idx as usize >= units.len() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    Completion::Normal(JsValue::Number(units[idx as usize] as f64))
                }),
            ),
            (
                "codePointAt",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let pos = match args.first() {
                        Some(v) => match to_num(interp, v) {
                            Ok(n) => to_integer_or_infinity(n),
                            Err(c) => return c,
                        },
                        None => 0.0,
                    };
                    let units = utf16_units(&s);
                    let idx = pos as isize;
                    if idx < 0 || idx as usize >= units.len() {
                        return Completion::Normal(JsValue::Undefined);
                    }
                    let idx = idx as usize;
                    let code = units[idx];
                    if (0xD800..=0xDBFF).contains(&code) && idx + 1 < units.len() {
                        let trail = units[idx + 1];
                        if (0xDC00..=0xDFFF).contains(&trail) {
                            let cp =
                                ((code as u32 - 0xD800) << 10) + (trail as u32 - 0xDC00) + 0x10000;
                            return Completion::Normal(JsValue::Number(cp as f64));
                        }
                    }
                    Completion::Normal(JsValue::Number(code as f64))
                }),
            ),
            (
                "indexOf",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let search = match args.first() {
                        Some(v) => match to_str(interp, v) {
                            Ok(s) => s,
                            Err(c) => return c,
                        },
                        None => "undefined".to_string(),
                    };
                    let pos = match args.get(1) {
                        Some(v) => match to_num(interp, v) {
                            Ok(n) => to_integer_or_infinity(n),
                            Err(c) => return c,
                        },
                        None => 0.0,
                    };
                    let s_units = utf16_units(&s);
                    let search_units = utf16_units(&search);
                    let s_len = s_units.len();
                    let search_len = search_units.len();
                    let start = pos.max(0.0).min(s_len as f64) as usize;
                    if search_len == 0 {
                        return Completion::Normal(JsValue::Number(start.min(s_len) as f64));
                    }
                    if search_len <= s_len {
                        for i in start..=s_len - search_len {
                            if s_units[i..i + search_len] == search_units[..] {
                                return Completion::Normal(JsValue::Number(i as f64));
                            }
                        }
                    }
                    Completion::Normal(JsValue::Number(-1.0))
                }),
            ),
            (
                "lastIndexOf",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let search = match args.first() {
                        Some(v) => match to_str(interp, v) {
                            Ok(s) => s,
                            Err(c) => return c,
                        },
                        None => "undefined".to_string(),
                    };
                    let num_pos = match args.get(1) {
                        Some(v) if !matches!(v, JsValue::Undefined) => match to_num(interp, v) {
                            Ok(n) => n,
                            Err(c) => return c,
                        },
                        _ => f64::NAN,
                    };
                    let s_units = utf16_units(&s);
                    let search_units = utf16_units(&search);
                    let s_len = s_units.len();
                    let search_len = search_units.len();
                    let pos = if num_pos.is_nan() {
                        s_len
                    } else {
                        to_integer_or_infinity(num_pos).max(0.0).min(s_len as f64) as usize
                    };
                    if search_len == 0 {
                        return Completion::Normal(JsValue::Number(pos.min(s_len) as f64));
                    }
                    let max_start = pos.min(s_len.saturating_sub(search_len));
                    for i in (0..=max_start).rev() {
                        if i + search_len <= s_len && s_units[i..i + search_len] == search_units[..]
                        {
                            return Completion::Normal(JsValue::Number(i as f64));
                        }
                    }
                    Completion::Normal(JsValue::Number(-1.0))
                }),
            ),
            (
                "includes",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let search_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    // Throw if search is a RegExp
                    if let JsValue::Object(ref o) = search_arg
                        && let Some(obj) = interp.get_object(o.id)
                        && obj.borrow().class_name == "RegExp"
                    {
                        return Completion::Throw(interp.create_type_error(
                                    "First argument to String.prototype.includes must not be a regular expression",
                                ));
                    }
                    let search = match to_str(interp, &search_arg) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let pos = match args.get(1) {
                        Some(v) => match to_num(interp, v) {
                            Ok(n) => to_integer_or_infinity(n).max(0.0) as usize,
                            Err(c) => return c,
                        },
                        None => 0,
                    };
                    let s_units = utf16_units(&s);
                    let search_units = utf16_units(&search);
                    let s_len = s_units.len();
                    let search_len = search_units.len();
                    if search_len == 0 {
                        return Completion::Normal(JsValue::Boolean(true));
                    }
                    if search_len <= s_len {
                        for i in pos..=s_len - search_len {
                            if s_units[i..i + search_len] == search_units[..] {
                                return Completion::Normal(JsValue::Boolean(true));
                            }
                        }
                    }
                    Completion::Normal(JsValue::Boolean(false))
                }),
            ),
            (
                "startsWith",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let search_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref o) = search_arg
                        && let Some(obj) = interp.get_object(o.id)
                        && obj.borrow().class_name == "RegExp"
                    {
                        return Completion::Throw(interp.create_type_error(
                                    "First argument to String.prototype.startsWith must not be a regular expression",
                                ));
                    }
                    let search = match to_str(interp, &search_arg) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let pos = match args.get(1) {
                        Some(v) if !matches!(v, JsValue::Undefined) => match to_num(interp, v) {
                            Ok(n) => to_integer_or_infinity(n).max(0.0) as usize,
                            Err(c) => return c,
                        },
                        _ => 0,
                    };
                    let s_units = utf16_units(&s);
                    let search_units = utf16_units(&search);
                    let s_len = s_units.len();
                    let start = pos.min(s_len);
                    if start + search_units.len() > s_len {
                        return Completion::Normal(JsValue::Boolean(false));
                    }
                    let result = s_units[start..start + search_units.len()] == search_units[..];
                    Completion::Normal(JsValue::Boolean(result))
                }),
            ),
            (
                "endsWith",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let search_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref o) = search_arg
                        && let Some(obj) = interp.get_object(o.id)
                        && obj.borrow().class_name == "RegExp"
                    {
                        return Completion::Throw(interp.create_type_error(
                                    "First argument to String.prototype.endsWith must not be a regular expression",
                                ));
                    }
                    let search = match to_str(interp, &search_arg) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let s_units = utf16_units(&s);
                    let search_units = utf16_units(&search);
                    let s_len = s_units.len();
                    let end_pos = match args.get(1) {
                        Some(v) if !matches!(v, JsValue::Undefined) => match to_num(interp, v) {
                            Ok(n) => to_integer_or_infinity(n).max(0.0).min(s_len as f64) as usize,
                            Err(c) => return c,
                        },
                        _ => s_len,
                    };
                    let search_len = search_units.len();
                    if search_len > end_pos {
                        return Completion::Normal(JsValue::Boolean(false));
                    }
                    let start = end_pos - search_len;
                    let result = s_units[start..end_pos] == search_units[..];
                    Completion::Normal(JsValue::Boolean(result))
                }),
            ),
            (
                "slice",
                2,
                Rc::new(|interp, this_val, args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let units = utf16_units(&s);
                    let len = units.len() as f64;
                    let int_start = match args.first() {
                        Some(v) => match to_num(interp, v) {
                            Ok(n) => to_integer_or_infinity(n),
                            Err(c) => return c,
                        },
                        None => 0.0,
                    };
                    let int_end = match args.get(1) {
                        Some(v) if !matches!(v, JsValue::Undefined) => match to_num(interp, v) {
                            Ok(n) => to_integer_or_infinity(n),
                            Err(c) => return c,
                        },
                        _ => len,
                    };
                    let from = if int_start < 0.0 {
                        (len + int_start).max(0.0) as usize
                    } else {
                        int_start.min(len) as usize
                    };
                    let to = if int_end < 0.0 {
                        (len + int_end).max(0.0) as usize
                    } else {
                        int_end.min(len) as usize
                    };
                    let result = utf16_substring(&units, from, to);
                    Completion::Normal(JsValue::String(JsString::from_str(&result)))
                }),
            ),
            (
                "substring",
                2,
                Rc::new(|interp, this_val, args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let units = utf16_units(&s);
                    let len = units.len() as f64;
                    let int_start = match args.first() {
                        Some(v) => match to_num(interp, v) {
                            Ok(n) => {
                                let i = to_integer_or_infinity(n);
                                if n.is_nan() { 0.0 } else { i }
                            }
                            Err(c) => return c,
                        },
                        None => 0.0,
                    };
                    let int_end = match args.get(1) {
                        Some(v) if !matches!(v, JsValue::Undefined) => match to_num(interp, v) {
                            Ok(n) => {
                                let i = to_integer_or_infinity(n);
                                if n.is_nan() { 0.0 } else { i }
                            }
                            Err(c) => return c,
                        },
                        _ => len,
                    };
                    let final_start = int_start.max(0.0).min(len) as usize;
                    let final_end = int_end.max(0.0).min(len) as usize;
                    let (from, to) = if final_start <= final_end {
                        (final_start, final_end)
                    } else {
                        (final_end, final_start)
                    };
                    let result = utf16_substring(&units, from, to);
                    Completion::Normal(JsValue::String(JsString::from_str(&result)))
                }),
            ),
            (
                "toLowerCase",
                0,
                Rc::new(|interp, this_val, _args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    Completion::Normal(JsValue::String(JsString::from_str(&s.to_lowercase())))
                }),
            ),
            (
                "toUpperCase",
                0,
                Rc::new(|interp, this_val, _args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    Completion::Normal(JsValue::String(JsString::from_str(&s.to_uppercase())))
                }),
            ),
            (
                "toLocaleLowerCase",
                0,
                Rc::new(|interp, this_val, _args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    Completion::Normal(JsValue::String(JsString::from_str(&s.to_lowercase())))
                }),
            ),
            (
                "toLocaleUpperCase",
                0,
                Rc::new(|interp, this_val, _args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    Completion::Normal(JsValue::String(JsString::from_str(&s.to_uppercase())))
                }),
            ),
            (
                "trim",
                0,
                Rc::new(|interp, this_val, _args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    Completion::Normal(JsValue::String(JsString::from_str(
                        s.trim_matches(is_ecma_whitespace),
                    )))
                }),
            ),
            (
                "trimStart",
                0,
                Rc::new(|interp, this_val, _args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    Completion::Normal(JsValue::String(JsString::from_str(
                        s.trim_start_matches(is_ecma_whitespace),
                    )))
                }),
            ),
            (
                "trimEnd",
                0,
                Rc::new(|interp, this_val, _args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    Completion::Normal(JsValue::String(JsString::from_str(
                        s.trim_end_matches(is_ecma_whitespace),
                    )))
                }),
            ),
            (
                "repeat",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let n = match args.first() {
                        Some(v) => match to_num(interp, v) {
                            Ok(n) => to_integer_or_infinity(n),
                            Err(c) => return c,
                        },
                        None => 0.0,
                    };
                    if n < 0.0 || n == f64::INFINITY {
                        return Completion::Throw(interp.create_range_error("Invalid count value"));
                    }
                    let count = n as usize;
                    Completion::Normal(JsValue::String(JsString::from_str(&s.repeat(count))))
                }),
            ),
            (
                "padStart",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let max_length = match args.first() {
                        Some(v) => match to_num(interp, v) {
                            Ok(n) => to_integer_or_infinity(n),
                            Err(c) => return c,
                        },
                        None => 0.0,
                    };
                    let fill = match args.get(1) {
                        Some(v) if !matches!(v, JsValue::Undefined) => match to_str(interp, v) {
                            Ok(s) => s,
                            Err(c) => return c,
                        },
                        _ => " ".to_string(),
                    };
                    let s_units = utf16_units(&s);
                    let s_len = s_units.len();
                    let int_max = max_length as usize;
                    if int_max <= s_len || fill.is_empty() {
                        return Completion::Normal(JsValue::String(JsString::from_str(&s)));
                    }
                    let fill_units = utf16_units(&fill);
                    let fill_len = int_max - s_len;
                    let pad: Vec<u16> = fill_units.iter().copied().cycle().take(fill_len).collect();
                    let mut result = pad;
                    result.extend_from_slice(&s_units);
                    Completion::Normal(JsValue::String(JsString::from_str(
                        &String::from_utf16_lossy(&result),
                    )))
                }),
            ),
            (
                "padEnd",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let max_length = match args.first() {
                        Some(v) => match to_num(interp, v) {
                            Ok(n) => to_integer_or_infinity(n),
                            Err(c) => return c,
                        },
                        None => 0.0,
                    };
                    let fill = match args.get(1) {
                        Some(v) if !matches!(v, JsValue::Undefined) => match to_str(interp, v) {
                            Ok(s) => s,
                            Err(c) => return c,
                        },
                        _ => " ".to_string(),
                    };
                    let s_units = utf16_units(&s);
                    let s_len = s_units.len();
                    let int_max = max_length as usize;
                    if int_max <= s_len || fill.is_empty() {
                        return Completion::Normal(JsValue::String(JsString::from_str(&s)));
                    }
                    let fill_units = utf16_units(&fill);
                    let fill_len = int_max - s_len;
                    let pad: Vec<u16> = fill_units.iter().copied().cycle().take(fill_len).collect();
                    let mut result = s_units;
                    result.extend_from_slice(&pad);
                    Completion::Normal(JsValue::String(JsString::from_str(
                        &String::from_utf16_lossy(&result),
                    )))
                }),
            ),
            (
                "concat",
                1,
                Rc::new(|interp, this_val, args| {
                    let mut s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    for arg in args {
                        match to_str(interp, arg) {
                            Ok(a) => s.push_str(&a),
                            Err(c) => return c,
                        }
                    }
                    Completion::Normal(JsValue::String(JsString::from_str(&s)))
                }),
            ),
            (
                "toString",
                0,
                Rc::new(|interp, this_val, _args| {
                    // Must be a string primitive or a String wrapper object
                    match this_val {
                        JsValue::String(s) => Completion::Normal(JsValue::String(s.clone())),
                        JsValue::Object(o) => {
                            if let Some(obj) = interp.get_object(o.id)
                                && obj.borrow().class_name == "String"
                                && let Some(ref pv) = obj.borrow().primitive_value
                            {
                                return Completion::Normal(pv.clone());
                            }
                            Completion::Throw(interp.create_type_error(
                                "String.prototype.toString requires that 'this' be a String",
                            ))
                        }
                        _ => Completion::Throw(interp.create_type_error(
                            "String.prototype.toString requires that 'this' be a String",
                        )),
                    }
                }),
            ),
            (
                "valueOf",
                0,
                Rc::new(|interp, this_val, _args| match this_val {
                    JsValue::String(s) => Completion::Normal(JsValue::String(s.clone())),
                    JsValue::Object(o) => {
                        if let Some(obj) = interp.get_object(o.id)
                            && obj.borrow().class_name == "String"
                            && let Some(ref pv) = obj.borrow().primitive_value
                        {
                            return Completion::Normal(pv.clone());
                        }
                        Completion::Throw(interp.create_type_error(
                            "String.prototype.valueOf requires that 'this' be a String",
                        ))
                    }
                    _ => Completion::Throw(interp.create_type_error(
                        "String.prototype.valueOf requires that 'this' be a String",
                    )),
                }),
            ),
            (
                "split",
                2,
                Rc::new(|interp, this_val, args| {
                    // RequireObjectCoercible
                    if matches!(this_val, JsValue::Null | JsValue::Undefined) {
                        return Completion::Throw(interp.create_type_error(
                            "String.prototype.split called on null or undefined",
                        ));
                    }
                    let separator = args.first().cloned().unwrap_or(JsValue::Undefined);
                    // Check for Symbol.split on the separator
                    if let JsValue::Object(ref o) = separator
                        && let Some(key) = interp.get_symbol_key("split")
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        let method = obj.borrow().get_property(&key);
                        if !matches!(method, JsValue::Undefined | JsValue::Null) {
                            let this_str = this_val.clone();
                            let limit = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                            return interp.call_function(&method, &separator, &[this_str, limit]);
                        }
                    }
                    let s = match to_str(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let limit_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let limit: u32 = if matches!(limit_arg, JsValue::Undefined) {
                        u32::MAX
                    } else {
                        match to_num(interp, &limit_arg) {
                            Ok(n) => n as u32,
                            Err(c) => return c,
                        }
                    };
                    if limit == 0 {
                        return Completion::Normal(interp.create_array(vec![]));
                    }
                    if matches!(separator, JsValue::Undefined) {
                        return Completion::Normal(
                            interp.create_array(vec![JsValue::String(JsString::from_str(&s))]),
                        );
                    }
                    let sep_str = match to_str(interp, &separator) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let s_units = utf16_units(&s);
                    let sep_units = utf16_units(&sep_str);
                    let s_len = s_units.len();
                    let sep_len = sep_units.len();

                    if sep_len == 0 {
                        // Split into individual UTF-16 code units
                        let mut parts = Vec::new();
                        for i in 0..s_len {
                            if parts.len() >= limit as usize {
                                break;
                            }
                            let ch = String::from_utf16_lossy(&s_units[i..i + 1]);
                            parts.push(JsValue::String(JsString::from_str(&ch)));
                        }
                        return Completion::Normal(interp.create_array(parts));
                    }

                    // SplitMatch algorithm
                    let mut parts = Vec::new();
                    let mut p = 0usize; // start of unmatched portion
                    let mut q = p;
                    while q + sep_len <= s_len {
                        if s_units[q..q + sep_len] == sep_units[..] {
                            let seg = utf16_substring(&s_units, p, q);
                            parts.push(JsValue::String(JsString::from_str(&seg)));
                            if parts.len() >= limit as usize {
                                return Completion::Normal(interp.create_array(parts));
                            }
                            p = q + sep_len;
                            q = p;
                        } else {
                            q += 1;
                        }
                    }
                    // Add remaining
                    let seg = utf16_substring(&s_units, p, s_len);
                    parts.push(JsValue::String(JsString::from_str(&seg)));
                    parts.truncate(limit as usize);
                    Completion::Normal(interp.create_array(parts))
                }),
            ),
            (
                "replace",
                2,
                Rc::new(|interp, this_val, args| {
                    if matches!(this_val, JsValue::Null | JsValue::Undefined) {
                        return Completion::Throw(interp.create_type_error(
                            "String.prototype.replace called on null or undefined",
                        ));
                    }
                    let search_value = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref o) = search_value
                        && let Some(key) = interp.get_symbol_key("replace")
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        let method = obj.borrow().get_property(&key);
                        if !matches!(method, JsValue::Undefined | JsValue::Null) {
                            let replace_val = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                            return interp.call_function(
                                &method,
                                &search_value,
                                &[this_val.clone(), replace_val],
                            );
                        }
                    }
                    let s = match to_str(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let search = match to_str(interp, &search_value) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let replace_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let is_fn = if let JsValue::Object(ref o) = replace_arg {
                        interp
                            .get_object(o.id)
                            .map(|obj| obj.borrow().callable.is_some())
                            .unwrap_or(false)
                    } else {
                        false
                    };
                    if is_fn {
                        // Find first occurrence using UTF-16
                        let s_units = utf16_units(&s);
                        let search_units = utf16_units(&search);
                        let s_len = s_units.len();
                        let search_len = search_units.len();
                        let mut pos = None;
                        if search_len == 0 {
                            pos = Some(0);
                        } else if search_len <= s_len {
                            for i in 0..=s_len - search_len {
                                if s_units[i..i + search_len] == search_units[..] {
                                    pos = Some(i);
                                    break;
                                }
                            }
                        }
                        if let Some(p) = pos {
                            let matched = utf16_substring(&s_units, p, p + search_len);
                            let r = interp.call_function(
                                &replace_arg,
                                &JsValue::Undefined,
                                &[
                                    JsValue::String(JsString::from_str(&matched)),
                                    JsValue::Number(p as f64),
                                    JsValue::String(JsString::from_str(&s)),
                                ],
                            );
                            let replacement = match r {
                                Completion::Normal(v) => match to_str(interp, &v) {
                                    Ok(s) => s,
                                    Err(c) => return c,
                                },
                                other => return other,
                            };
                            let before = utf16_substring(&s_units, 0, p);
                            let after = utf16_substring(&s_units, p + search_len, s_len);
                            let result = format!("{before}{replacement}{after}");
                            Completion::Normal(JsValue::String(JsString::from_str(&result)))
                        } else {
                            Completion::Normal(JsValue::String(JsString::from_str(&s)))
                        }
                    } else {
                        let replacement = match to_str(interp, &replace_arg) {
                            Ok(s) => s,
                            Err(c) => return c,
                        };
                        // Handle replacement patterns ($&, $$, $`, $', $<n>)
                        let s_units = utf16_units(&s);
                        let search_units = utf16_units(&search);
                        let s_len = s_units.len();
                        let search_len = search_units.len();
                        let mut match_pos = None;
                        if search_len == 0 {
                            match_pos = Some(0);
                        } else if search_len <= s_len {
                            for i in 0..=s_len - search_len {
                                if s_units[i..i + search_len] == search_units[..] {
                                    match_pos = Some(i);
                                    break;
                                }
                            }
                        }
                        if let Some(pos) = match_pos {
                            let before = utf16_substring(&s_units, 0, pos);
                            let after = utf16_substring(&s_units, pos + search_len, s_len);
                            let matched = utf16_substring(&s_units, pos, pos + search_len);
                            let rep = apply_replacement_pattern(&replacement, &matched, &s, pos);
                            let result = format!("{before}{rep}{after}");
                            Completion::Normal(JsValue::String(JsString::from_str(&result)))
                        } else {
                            Completion::Normal(JsValue::String(JsString::from_str(&s)))
                        }
                    }
                }),
            ),
            (
                "replaceAll",
                2,
                Rc::new(|interp, this_val, args| {
                    if matches!(this_val, JsValue::Null | JsValue::Undefined) {
                        return Completion::Throw(interp.create_type_error(
                            "String.prototype.replaceAll called on null or undefined",
                        ));
                    }
                    let search_value = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref o) = search_value {
                        // Step 2a: IsRegExp check
                        let is_regexp = if let Some(match_key) = interp.get_symbol_key("match") {
                            match interp.get_object_property(o.id, &match_key, &search_value) {
                                Completion::Normal(v) if !matches!(v, JsValue::Undefined) => {
                                    to_boolean(&v)
                                }
                                Completion::Normal(_) => interp
                                    .get_object(o.id)
                                    .map(|obj| obj.borrow().class_name == "RegExp")
                                    .unwrap_or(false),
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            }
                        } else {
                            interp
                                .get_object(o.id)
                                .map(|obj| obj.borrow().class_name == "RegExp")
                                .unwrap_or(false)
                        };
                        if is_regexp {
                            // Step 2b: Get flags via getter
                            let flags_val =
                                match interp.get_object_property(o.id, "flags", &search_value) {
                                    Completion::Normal(v) => v,
                                    Completion::Throw(e) => return Completion::Throw(e),
                                    other => return other,
                                };
                            let flags = match interp.to_string_value(&flags_val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            if !flags.contains('g') {
                                return Completion::Throw(interp.create_type_error(
                                    "String.prototype.replaceAll called with a non-global RegExp argument",
                                ));
                            }
                        }
                        // Step 2c-d: GetMethod(searchValue, @@replace)
                        if let Some(key) = interp.get_symbol_key("replace") {
                            let method = match interp.get_object_property(o.id, &key, &search_value)
                            {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            };
                            if !matches!(method, JsValue::Undefined | JsValue::Null) {
                                let replace_val =
                                    args.get(1).cloned().unwrap_or(JsValue::Undefined);
                                return interp.call_function(
                                    &method,
                                    &search_value,
                                    &[this_val.clone(), replace_val],
                                );
                            }
                        }
                    }
                    let s = match to_str(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let search = match to_str(interp, &search_value) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let replace_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let is_fn = if let JsValue::Object(ref o) = replace_arg {
                        interp
                            .get_object(o.id)
                            .map(|obj| obj.borrow().callable.is_some())
                            .unwrap_or(false)
                    } else {
                        false
                    };
                    let s_units = utf16_units(&s);
                    let search_units = utf16_units(&search);
                    let s_len = s_units.len();
                    let search_len = search_units.len();

                    // Find all match positions
                    let mut positions = Vec::new();
                    if search_len == 0 {
                        for i in 0..=s_len {
                            positions.push(i);
                        }
                    } else {
                        let mut i = 0;
                        while i + search_len <= s_len {
                            if s_units[i..i + search_len] == search_units[..] {
                                positions.push(i);
                                i += search_len;
                            } else {
                                i += 1;
                            }
                        }
                    }

                    if positions.is_empty() {
                        return Completion::Normal(JsValue::String(JsString::from_str(&s)));
                    }

                    let mut result = Vec::new();
                    let mut last_end = 0;
                    for &pos in &positions {
                        result.extend_from_slice(&s_units[last_end..pos]);
                        let matched = utf16_substring(&s_units, pos, pos + search_len);
                        if is_fn {
                            let r = interp.call_function(
                                &replace_arg,
                                &JsValue::Undefined,
                                &[
                                    JsValue::String(JsString::from_str(&matched)),
                                    JsValue::Number(pos as f64),
                                    JsValue::String(JsString::from_str(&s)),
                                ],
                            );
                            let rep = match r {
                                Completion::Normal(v) => match to_str(interp, &v) {
                                    Ok(s) => s,
                                    Err(c) => return c,
                                },
                                other => return other,
                            };
                            result.extend(rep.encode_utf16());
                        } else {
                            let rep_str = match to_str(interp, &replace_arg) {
                                Ok(s) => s,
                                Err(c) => return c,
                            };
                            let rep = apply_replacement_pattern(&rep_str, &matched, &s, pos);
                            result.extend(rep.encode_utf16());
                        }
                        last_end = pos + search_len;
                    }
                    result.extend_from_slice(&s_units[last_end..]);
                    Completion::Normal(JsValue::String(JsString::from_str(
                        &String::from_utf16_lossy(&result),
                    )))
                }),
            ),
            (
                "at",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let units = utf16_units(&s);
                    let len = units.len() as i64;
                    let idx = match args.first() {
                        Some(v) => match to_num(interp, v) {
                            Ok(n) => to_integer_or_infinity(n) as i64,
                            Err(c) => return c,
                        },
                        None => 0,
                    };
                    let actual = if idx < 0 { len + idx } else { idx };
                    if actual < 0 || actual >= len {
                        return Completion::Normal(JsValue::Undefined);
                    }
                    let ch = String::from_utf16_lossy(&units[actual as usize..actual as usize + 1]);
                    Completion::Normal(JsValue::String(JsString::from_str(&ch)))
                }),
            ),
            (
                "search",
                1,
                Rc::new(|interp, this_val, args| {
                    if matches!(this_val, JsValue::Null | JsValue::Undefined) {
                        return Completion::Throw(interp.create_type_error(
                            "String.prototype.search called on null or undefined",
                        ));
                    }
                    let regexp = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref o) = regexp
                        && let Some(key) = interp.get_symbol_key("search")
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        let method = obj.borrow().get_property(&key);
                        if !matches!(method, JsValue::Undefined | JsValue::Null) {
                            let this_str = this_val.clone();
                            return interp.call_function(&method, &regexp, &[this_str]);
                        }
                    }
                    let s = match to_str(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let source = if args.is_empty() {
                        String::new()
                    } else {
                        match to_str(interp, &regexp) {
                            Ok(s) => s,
                            Err(c) => return c,
                        }
                    };
                    // Create a RegExp and call @@search
                    let rx = interp.create_regexp(&source, "");
                    if let JsValue::Object(ref ro) = rx
                        && let Some(key) = interp.get_symbol_key("search")
                        && let Some(obj) = interp.get_object(ro.id)
                    {
                        let method = obj.borrow().get_property(&key);
                        if !matches!(method, JsValue::Undefined | JsValue::Null) {
                            let this_str = JsValue::String(JsString::from_str(&s));
                            return interp.call_function(&method, &rx, &[this_str]);
                        }
                    }
                    // Fallback
                    if let Ok(re) = regex::Regex::new(&source)
                        && let Some(m) = re.find(&s)
                    {
                        return Completion::Normal(JsValue::Number(m.start() as f64));
                    }
                    Completion::Normal(JsValue::Number(-1.0))
                }),
            ),
            (
                "match",
                1,
                Rc::new(|interp, this_val, args| {
                    if matches!(this_val, JsValue::Null | JsValue::Undefined) {
                        return Completion::Throw(interp.create_type_error(
                            "String.prototype.match called on null or undefined",
                        ));
                    }
                    let regexp = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref o) = regexp
                        && let Some(key) = interp.get_symbol_key("match")
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        let method = obj.borrow().get_property(&key);
                        if !matches!(method, JsValue::Undefined | JsValue::Null) {
                            let this_str = this_val.clone();
                            return interp.call_function(&method, &regexp, &[this_str]);
                        }
                    }
                    let s = match to_str(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let source = if args.is_empty() || matches!(regexp, JsValue::Undefined) {
                        String::new()
                    } else {
                        match to_str(interp, &regexp) {
                            Ok(s) => s,
                            Err(c) => return c,
                        }
                    };
                    // Create a RegExp and call @@match
                    let rx = interp.create_regexp(&source, "");
                    if let JsValue::Object(ref ro) = rx
                        && let Some(key) = interp.get_symbol_key("match")
                        && let Some(obj) = interp.get_object(ro.id)
                    {
                        let method = obj.borrow().get_property(&key);
                        if !matches!(method, JsValue::Undefined | JsValue::Null) {
                            let this_str = JsValue::String(JsString::from_str(&s));
                            return interp.call_function(&method, &rx, &[this_str]);
                        }
                    }
                    // Fallback
                    if let Ok(re) = regex::Regex::new(&source) {
                        if let Some(m) = re.find(&s) {
                            let matched = JsValue::String(JsString::from_str(m.as_str()));
                            let result = interp.create_array(vec![matched]);
                            if let JsValue::Object(ro) = &result
                                && let Some(robj) = interp.get_object(ro.id)
                            {
                                robj.borrow_mut().insert_value(
                                    "index".to_string(),
                                    JsValue::Number(m.start() as f64),
                                );
                                robj.borrow_mut().insert_value(
                                    "input".to_string(),
                                    JsValue::String(JsString::from_str(&s)),
                                );
                            }
                            Completion::Normal(result)
                        } else {
                            Completion::Normal(JsValue::Null)
                        }
                    } else {
                        Completion::Normal(JsValue::Null)
                    }
                }),
            ),
            (
                "matchAll",
                1,
                Rc::new(|interp, this_val, args| {
                    if matches!(this_val, JsValue::Null | JsValue::Undefined) {
                        return Completion::Throw(interp.create_type_error(
                            "String.prototype.matchAll called on null or undefined",
                        ));
                    }
                    let regexp = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref o) = regexp {
                        // IsRegExp check
                        let is_regexp = if let Some(match_key) = interp.get_symbol_key("match") {
                            match interp.get_object_property(o.id, &match_key, &regexp) {
                                Completion::Normal(v) if !matches!(v, JsValue::Undefined) => {
                                    to_boolean(&v)
                                }
                                Completion::Normal(_) => interp
                                    .get_object(o.id)
                                    .map(|obj| obj.borrow().class_name == "RegExp")
                                    .unwrap_or(false),
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            }
                        } else {
                            interp
                                .get_object(o.id)
                                .map(|obj| obj.borrow().class_name == "RegExp")
                                .unwrap_or(false)
                        };
                        if is_regexp {
                            let flags_val = match interp.get_object_property(o.id, "flags", &regexp)
                            {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            };
                            let flags = match interp.to_string_value(&flags_val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            if !flags.contains('g') {
                                return Completion::Throw(interp.create_type_error(
                                    "String.prototype.matchAll called with a non-global RegExp argument",
                                ));
                            }
                        }
                        if let Some(key) = interp.get_symbol_key("matchAll") {
                            let method = match interp.get_object_property(o.id, &key, &regexp) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            };
                            if !matches!(method, JsValue::Undefined | JsValue::Null) {
                                let this_str = this_val.clone();
                                return interp.call_function(&method, &regexp, &[this_str]);
                            }
                        }
                    }
                    // Create a RegExp with 'g' flag and call @@matchAll
                    let source = match to_str(interp, &regexp) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let rx = interp.create_regexp(&source, "g");
                    if let JsValue::Object(ref ro) = rx
                        && let Some(key) = interp.get_symbol_key("matchAll")
                        && let Some(obj) = interp.get_object(ro.id)
                    {
                        let method = obj.borrow().get_property(&key);
                        if !matches!(method, JsValue::Undefined | JsValue::Null) {
                            let this_str = this_val.clone();
                            return interp.call_function(&method, &rx, &[this_str]);
                        }
                    }
                    Completion::Throw(interp.create_type_error("matchAll requires a global RegExp"))
                }),
            ),
            (
                "normalize",
                0,
                Rc::new(|interp, this_val, args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let form = match args.first() {
                        Some(v) if !matches!(v, JsValue::Undefined) => match to_str(interp, v) {
                            Ok(s) => s,
                            Err(c) => return c,
                        },
                        _ => "NFC".to_string(),
                    };
                    match form.as_str() {
                        "NFC" | "NFD" | "NFKC" | "NFKD" => {}
                        _ => {
                            return Completion::Throw(interp.create_range_error(
                                &format!("The normalization form should be one of NFC, NFD, NFKC, NFKD. Got: {form}"),
                            ));
                        }
                    }
                    // For now, return the string as-is (proper Unicode normalization would need a crate)
                    Completion::Normal(JsValue::String(JsString::from_str(&s)))
                }),
            ),
            (
                "localeCompare",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let that = match args.first() {
                        Some(v) => match to_str(interp, v) {
                            Ok(s) => s,
                            Err(c) => return c,
                        },
                        None => "undefined".to_string(),
                    };
                    let result = s.cmp(&that);
                    Completion::Normal(JsValue::Number(match result {
                        std::cmp::Ordering::Less => -1.0,
                        std::cmp::Ordering::Equal => 0.0,
                        std::cmp::Ordering::Greater => 1.0,
                    }))
                }),
            ),
            (
                "isWellFormed",
                0,
                Rc::new(|interp, this_val, _args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let units = utf16_units(&s);
                    let mut i = 0;
                    while i < units.len() {
                        let cu = units[i];
                        if (0xD800..=0xDBFF).contains(&cu) {
                            if i + 1 >= units.len() || !(0xDC00..=0xDFFF).contains(&units[i + 1]) {
                                return Completion::Normal(JsValue::Boolean(false));
                            }
                            i += 2;
                        } else if (0xDC00..=0xDFFF).contains(&cu) {
                            return Completion::Normal(JsValue::Boolean(false));
                        } else {
                            i += 1;
                        }
                    }
                    Completion::Normal(JsValue::Boolean(true))
                }),
            ),
            (
                "toWellFormed",
                0,
                Rc::new(|interp, this_val, _args| {
                    let s = match this_string_value(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let units = utf16_units(&s);
                    let mut result = Vec::with_capacity(units.len());
                    let mut i = 0;
                    while i < units.len() {
                        let cu = units[i];
                        if (0xD800..=0xDBFF).contains(&cu) {
                            if i + 1 < units.len() && (0xDC00..=0xDFFF).contains(&units[i + 1]) {
                                result.push(cu);
                                result.push(units[i + 1]);
                                i += 2;
                            } else {
                                result.push(0xFFFD);
                                i += 1;
                            }
                        } else if (0xDC00..=0xDFFF).contains(&cu) {
                            result.push(0xFFFD);
                            i += 1;
                        } else {
                            result.push(cu);
                            i += 1;
                        }
                    }
                    Completion::Normal(JsValue::String(JsString::from_str(
                        &String::from_utf16_lossy(&result),
                    )))
                }),
            ),
        ];

        for (name, arity, func) in methods {
            let fn_val =
                self.create_function(JsFunction::Native(name.to_string(), arity, func, false));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        // Annex B: HTML methods
        let html_no_arg: Vec<(&str, &str, &str)> = vec![
            ("big", "big", "big"),
            ("blink", "blink", "blink"),
            ("bold", "b", "b"),
            ("fixed", "tt", "tt"),
            ("italics", "i", "i"),
            ("small", "small", "small"),
            ("strike", "strike", "strike"),
            ("sub", "sub", "sub"),
            ("sup", "sup", "sup"),
        ];
        for (name, tag_open, tag_close) in html_no_arg {
            let tag_o = tag_open.to_string();
            let tag_c = tag_close.to_string();
            let fn_val = self.create_function(JsFunction::Native(
                name.to_string(),
                0,
                Rc::new(move |interp, this, _args| {
                    let s = match this_string_value(interp, this) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    Completion::Normal(JsValue::String(JsString::from_str(&format!(
                        "<{tag_o}>{s}</{tag_c}>"
                    ))))
                }),
                false,
            ));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        let html_one_arg: Vec<(&str, &str, &str, &str)> = vec![
            ("anchor", "a", "name", "a"),
            ("fontcolor", "font", "color", "font"),
            ("fontsize", "font", "size", "font"),
            ("link", "a", "href", "a"),
        ];
        for (name, tag_open, attr, tag_close) in html_one_arg {
            let tag_o = tag_open.to_string();
            let attr_name = attr.to_string();
            let tag_c = tag_close.to_string();
            let fn_val = self.create_function(JsFunction::Native(
                name.to_string(),
                1,
                Rc::new(move |interp, this, args| {
                    let s = match this_string_value(interp, this) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let attr_val = match interp.to_string_value(&arg) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };
                    let escaped = attr_val.replace('"', "&quot;");
                    Completion::Normal(JsValue::String(JsString::from_str(&format!(
                        "<{tag_o} {attr_name}=\"{escaped}\">{s}</{tag_c}>"
                    ))))
                }),
                false,
            ));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        // Annex B: substr(start, length)
        {
            let fn_val = self.create_function(JsFunction::Native(
                "substr".to_string(),
                2,
                Rc::new(|interp, this, args| {
                    let s = match this_string_value(interp, this) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let units = utf16_units(&s);
                    let len = units.len() as f64;
                    let int_start = match args.first() {
                        Some(v) => match to_num(interp, v) {
                            Ok(n) => n,
                            Err(c) => return c,
                        },
                        None => 0.0,
                    };
                    let int_start = int_start as i64;
                    let start = if int_start < 0 {
                        (len as i64 + int_start).max(0) as usize
                    } else {
                        (int_start as usize).min(units.len())
                    };
                    let result_len = match args.get(1) {
                        Some(v) if !matches!(v, JsValue::Undefined) => {
                            let l = match to_num(interp, v) {
                                Ok(n) => n,
                                Err(c) => return c,
                            };
                            let l = l as i64;
                            l.max(0) as usize
                        }
                        _ => units.len(),
                    };
                    let end = (start + result_len).min(units.len());
                    Completion::Normal(JsValue::String(JsString::from_str(&utf16_substring(
                        &units, start, end,
                    ))))
                }),
                false,
            ));
            proto
                .borrow_mut()
                .insert_builtin("substr".to_string(), fn_val);
        }

        // Aliases
        let trim_start_fn = proto.borrow().get_property("trimStart");
        proto
            .borrow_mut()
            .insert_builtin("trimLeft".to_string(), trim_start_fn);
        let trim_end_fn = proto.borrow().get_property("trimEnd");
        proto
            .borrow_mut()
            .insert_builtin("trimRight".to_string(), trim_end_fn);

        // String.prototype[@@iterator]
        let str_iter_fn = self.create_function(JsFunction::native(
            "[Symbol.iterator]".to_string(),
            0,
            |interp, this_val, _args| {
                if matches!(this_val, JsValue::Null | JsValue::Undefined) {
                    return Completion::Throw(interp.create_type_error(
                        "String.prototype[Symbol.iterator] called on null or undefined",
                    ));
                }
                let s = match this_val {
                    JsValue::String(s) => s.clone(),
                    _ => match interp.to_string_value(this_val) {
                        Ok(converted) => JsString::from_str(&converted),
                        Err(e) => return Completion::Throw(e),
                    },
                };
                Completion::Normal(interp.create_string_iterator(s))
            },
        ));
        if let Some(key) = self.get_symbol_iterator_key() {
            proto.borrow_mut().insert_property(
                key,
                PropertyDescriptor::data(str_iter_fn, true, false, true),
            );
        }

        // Set String.prototype on the String constructor and wire constructor back
        if let Some(str_val) = self.global_env.borrow().get("String")
            && let JsValue::Object(o) = &str_val
            && let Some(str_obj) = self.get_object(o.id)
        {
            let proto_val = JsValue::Object(crate::types::JsObject {
                id: proto.borrow().id.unwrap(),
            });
            str_obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val.clone(), false, false, false),
            );
            proto
                .borrow_mut()
                .insert_builtin("constructor".to_string(), str_val.clone());
        }

        self.string_prototype = Some(proto);
    }
}

// Apply replacement patterns like $&, $$, $`, $'
// position is in UTF-16 code units
fn apply_replacement_pattern(
    replacement: &str,
    matched: &str,
    original: &str,
    position: usize,
) -> String {
    let orig_units = utf16_units(original);
    let matched_units = utf16_units(matched);
    let chars: Vec<char> = replacement.chars().collect();
    let mut result = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() {
            match chars[i + 1] {
                '$' => {
                    result.push('$');
                    i += 2;
                }
                '&' => {
                    result.push_str(matched);
                    i += 2;
                }
                '`' => {
                    let before = utf16_substring(&orig_units, 0, position);
                    result.push_str(&before);
                    i += 2;
                }
                '\'' => {
                    let end = position + matched_units.len();
                    let after = utf16_substring(&orig_units, end, orig_units.len());
                    result.push_str(&after);
                    i += 2;
                }
                _ => {
                    result.push('$');
                    i += 1;
                }
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}
