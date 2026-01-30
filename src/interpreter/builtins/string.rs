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
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
    )
}

impl Interpreter {
    pub(crate) fn setup_string_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "String".to_string();

        let methods: Vec<(
            &str,
            usize,
            Rc<dyn Fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion>,
        )> = vec![
            (
                "charAt",
                1,
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let idx = args.first().map(|v| to_number(v) as usize).unwrap_or(0);
                    let ch = s
                        .chars()
                        .nth(idx)
                        .map(|c| c.to_string())
                        .unwrap_or_default();
                    Completion::Normal(JsValue::String(JsString::from_str(&ch)))
                }),
            ),
            (
                "charCodeAt",
                1,
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let idx = args.first().map(|v| to_number(v) as usize).unwrap_or(0);
                    let code = s
                        .encode_utf16()
                        .nth(idx)
                        .map(|c| c as f64)
                        .unwrap_or(f64::NAN);
                    Completion::Normal(JsValue::Number(code))
                }),
            ),
            (
                "codePointAt",
                1,
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let idx = args.first().map(|v| to_number(v) as usize).unwrap_or(0);
                    // Use UTF-16 code units for indexing
                    let utf16: Vec<u16> = s.encode_utf16().collect();
                    if idx >= utf16.len() {
                        return Completion::Normal(JsValue::Undefined);
                    }
                    let code = utf16[idx];
                    if (0xD800..=0xDBFF).contains(&code) && idx + 1 < utf16.len() {
                        let trail = utf16[idx + 1];
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
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let search = args.first().map(to_js_string).unwrap_or_default();
                    let from = args.get(1).map(|v| to_number(v) as usize).unwrap_or(0);
                    let result = s[from..]
                        .find(&search)
                        .map(|i| (i + from) as f64)
                        .unwrap_or(-1.0);
                    Completion::Normal(JsValue::Number(result))
                }),
            ),
            (
                "lastIndexOf",
                1,
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let search = args.first().map(to_js_string).unwrap_or_default();
                    let result = s.rfind(&search).map(|i| i as f64).unwrap_or(-1.0);
                    Completion::Normal(JsValue::Number(result))
                }),
            ),
            (
                "includes",
                1,
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let search = args.first().map(to_js_string).unwrap_or_default();
                    Completion::Normal(JsValue::Boolean(s.contains(&search)))
                }),
            ),
            (
                "startsWith",
                1,
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let search = args.first().map(to_js_string).unwrap_or_default();
                    Completion::Normal(JsValue::Boolean(s.starts_with(&search)))
                }),
            ),
            (
                "endsWith",
                1,
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let search = args.first().map(to_js_string).unwrap_or_default();
                    Completion::Normal(JsValue::Boolean(s.ends_with(&search)))
                }),
            ),
            (
                "slice",
                2,
                Rc::new(|_interp, this_val, args| {
                    let js_str = match this_val {
                        JsValue::String(s) => s.clone(),
                        _ => JsString::from_str(&to_js_string(this_val)),
                    };
                    let len = js_str.len() as f64;
                    let raw_start = args.first().map(|v| to_number(v)).unwrap_or(0.0);
                    let int_start = if raw_start.is_nan() { 0.0 } else if raw_start == f64::NEG_INFINITY { f64::NEG_INFINITY } else { raw_start.trunc() };
                    let from = if int_start < 0.0 {
                        (len + int_start).max(0.0) as usize
                    } else {
                        int_start.min(len) as usize
                    };
                    let raw_end = args.get(1).map(|v| {
                        if matches!(v, JsValue::Undefined) {
                            len
                        } else {
                            to_number(v)
                        }
                    }).unwrap_or(len);
                    let int_end = if raw_end.is_nan() { 0.0 } else if raw_end == f64::NEG_INFINITY { f64::NEG_INFINITY } else { raw_end.trunc() };
                    let to = if int_end < 0.0 {
                        (len + int_end).max(0.0) as usize
                    } else {
                        int_end.min(len) as usize
                    };
                    let result = js_str.slice_utf16(from, to);
                    Completion::Normal(JsValue::String(result))
                }),
            ),
            (
                "substring",
                2,
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let len = s.len();
                    let mut start = args
                        .first()
                        .map(|v| (to_number(v) as usize).min(len))
                        .unwrap_or(0);
                    let mut end = args
                        .get(1)
                        .map(|v| {
                            if matches!(v, JsValue::Undefined) {
                                len
                            } else {
                                (to_number(v) as usize).min(len)
                            }
                        })
                        .unwrap_or(len);
                    if start > end {
                        std::mem::swap(&mut start, &mut end);
                    }
                    Completion::Normal(JsValue::String(JsString::from_str(&s[start..end])))
                }),
            ),
            (
                "toLowerCase",
                0,
                Rc::new(|_interp, this_val, _args| {
                    Completion::Normal(JsValue::String(JsString::from_str(
                        &to_js_string(this_val).to_lowercase(),
                    )))
                }),
            ),
            (
                "toUpperCase",
                0,
                Rc::new(|_interp, this_val, _args| {
                    Completion::Normal(JsValue::String(JsString::from_str(
                        &to_js_string(this_val).to_uppercase(),
                    )))
                }),
            ),
            (
                "trim",
                0,
                Rc::new(|_interp, this_val, _args| {
                    Completion::Normal(JsValue::String(JsString::from_str(
                        to_js_string(this_val).trim_matches(is_ecma_whitespace),
                    )))
                }),
            ),
            (
                "trimStart",
                0,
                Rc::new(|_interp, this_val, _args| {
                    Completion::Normal(JsValue::String(JsString::from_str(
                        to_js_string(this_val).trim_start_matches(is_ecma_whitespace),
                    )))
                }),
            ),
            (
                "trimEnd",
                0,
                Rc::new(|_interp, this_val, _args| {
                    Completion::Normal(JsValue::String(JsString::from_str(
                        to_js_string(this_val).trim_end_matches(is_ecma_whitespace),
                    )))
                }),
            ),
            (
                "repeat",
                1,
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let count = args.first().map(|v| to_number(v) as usize).unwrap_or(0);
                    Completion::Normal(JsValue::String(JsString::from_str(&s.repeat(count))))
                }),
            ),
            (
                "padStart",
                1,
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let target_len = args.first().map(|v| to_number(v) as usize).unwrap_or(0);
                    let fill = args
                        .get(1)
                        .map(to_js_string)
                        .unwrap_or_else(|| " ".to_string());
                    if s.len() >= target_len || fill.is_empty() {
                        return Completion::Normal(JsValue::String(JsString::from_str(&s)));
                    }
                    let pad_len = target_len - s.len();
                    let pad: String = fill.chars().cycle().take(pad_len).collect();
                    Completion::Normal(JsValue::String(JsString::from_str(&format!("{pad}{s}"))))
                }),
            ),
            (
                "padEnd",
                1,
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let target_len = args.first().map(|v| to_number(v) as usize).unwrap_or(0);
                    let fill = args
                        .get(1)
                        .map(to_js_string)
                        .unwrap_or_else(|| " ".to_string());
                    if s.len() >= target_len || fill.is_empty() {
                        return Completion::Normal(JsValue::String(JsString::from_str(&s)));
                    }
                    let pad_len = target_len - s.len();
                    let pad: String = fill.chars().cycle().take(pad_len).collect();
                    Completion::Normal(JsValue::String(JsString::from_str(&format!("{s}{pad}"))))
                }),
            ),
            (
                "concat",
                1,
                Rc::new(|_interp, this_val, args| {
                    let mut s = to_js_string(this_val);
                    for arg in args {
                        s.push_str(&to_js_string(arg));
                    }
                    Completion::Normal(JsValue::String(JsString::from_str(&s)))
                }),
            ),
            (
                "toString",
                0,
                Rc::new(|_interp, this_val, _args| {
                    Completion::Normal(JsValue::String(JsString::from_str(&to_js_string(this_val))))
                }),
            ),
            (
                "valueOf",
                0,
                Rc::new(|_interp, this_val, _args| {
                    Completion::Normal(JsValue::String(JsString::from_str(&to_js_string(this_val))))
                }),
            ),
            (
                "split",
                2,
                Rc::new(|interp, this_val, args| {
                    let separator = args.first().cloned().unwrap_or(JsValue::Undefined);
                    // Check for Symbol.split on the separator
                    if let JsValue::Object(ref o) = separator {
                        if let Some(key) = interp.get_symbol_key("split") {
                            if let Some(obj) = interp.get_object(o.id) {
                                let method = obj.borrow().get_property(&key);
                                if !matches!(method, JsValue::Undefined | JsValue::Null) {
                                    let this_str = JsValue::String(JsString::from_str(&to_js_string(this_val)));
                                    let limit = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                                    return interp.call_function(&method, &separator, &[this_str, limit]);
                                }
                            }
                        }
                    }
                    let s = to_js_string(this_val);
                    let limit_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let limit: u32 = if matches!(limit_arg, JsValue::Undefined) {
                        u32::MAX
                    } else {
                        to_number(&limit_arg) as u32
                    };
                    if limit == 0 {
                        return Completion::Normal(interp.create_array(vec![]));
                    }
                    let mut parts: Vec<JsValue> = if matches!(separator, JsValue::Undefined) {
                        vec![JsValue::String(JsString::from_str(&s))]
                    } else {
                        let sep_str = to_js_string(&separator);
                        if sep_str.is_empty() {
                            s.chars()
                                .map(|c| JsValue::String(JsString::from_str(&c.to_string())))
                                .collect()
                        } else {
                            s.split(&sep_str)
                                .map(|p| JsValue::String(JsString::from_str(p)))
                                .collect()
                        }
                    };
                    parts.truncate(limit as usize);
                    Completion::Normal(interp.create_array(parts))
                }),
            ),
            (
                "replace",
                2,
                Rc::new(|interp, this_val, args| {
                    let search_value = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref o) = search_value {
                        if let Some(key) = interp.get_symbol_key("replace") {
                            if let Some(obj) = interp.get_object(o.id) {
                                let method = obj.borrow().get_property(&key);
                                if !matches!(method, JsValue::Undefined | JsValue::Null) {
                                    let this_str = JsValue::String(JsString::from_str(&to_js_string(this_val)));
                                    let replace_val = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                                    return interp.call_function(&method, &search_value, &[this_str, replace_val]);
                                }
                            }
                        }
                    }
                    let s = to_js_string(this_val);
                    let search = to_js_string(&search_value);
                    let replace_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    // Check if replacer is a function
                    let is_fn = if let JsValue::Object(ref o) = replace_arg {
                        interp.get_object(o.id).map(|obj| obj.borrow().callable.is_some()).unwrap_or(false)
                    } else {
                        false
                    };
                    if is_fn {
                        if let Some(pos) = s.find(&search) {
                            let matched = &s[pos..pos + search.len()];
                            let r = interp.call_function(&replace_arg, &JsValue::Undefined, &[
                                JsValue::String(JsString::from_str(matched)),
                                JsValue::Number(pos as f64),
                                JsValue::String(JsString::from_str(&s)),
                            ]);
                            let replacement = match r {
                                Completion::Normal(v) => to_js_string(&v),
                                other => return other,
                            };
                            let mut result = String::new();
                            result.push_str(&s[..pos]);
                            result.push_str(&replacement);
                            result.push_str(&s[pos + search.len()..]);
                            Completion::Normal(JsValue::String(JsString::from_str(&result)))
                        } else {
                            Completion::Normal(JsValue::String(JsString::from_str(&s)))
                        }
                    } else {
                        let replacement = to_js_string(&replace_arg);
                        let result = s.replacen(&search, &replacement, 1);
                        Completion::Normal(JsValue::String(JsString::from_str(&result)))
                    }
                }),
            ),
            (
                "replaceAll",
                2,
                Rc::new(|interp, this_val, args| {
                    let search_value = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref o) = search_value {
                        // If regexp, must have global flag
                        if let Some(obj) = interp.get_object(o.id) {
                            let is_regexp = obj.borrow().class_name == "RegExp";
                            if is_regexp {
                                let flags_val = obj.borrow().get_property("flags");
                                let flags = to_js_string(&flags_val);
                                if !flags.contains('g') {
                                    let err = interp.create_type_error(
                                        "String.prototype.replaceAll called with a non-global RegExp argument",
                                    );
                                    return Completion::Throw(err);
                                }
                            }
                        }
                        if let Some(key) = interp.get_symbol_key("replace") {
                            if let Some(obj) = interp.get_object(o.id) {
                                let method = obj.borrow().get_property(&key);
                                if !matches!(method, JsValue::Undefined | JsValue::Null) {
                                    let this_str = JsValue::String(JsString::from_str(&to_js_string(this_val)));
                                    let replace_val = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                                    return interp.call_function(&method, &search_value, &[this_str, replace_val]);
                                }
                            }
                        }
                    }
                    let s = to_js_string(this_val);
                    let search = to_js_string(&search_value);
                    let replacement = args.get(1).map(to_js_string).unwrap_or_default();
                    Completion::Normal(JsValue::String(JsString::from_str(
                        &s.replace(&search, &replacement),
                    )))
                }),
            ),
            (
                "at",
                1,
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let len = s.len() as i64;
                    let idx = args.first().map(|v| to_number(v) as i64).unwrap_or(0);
                    let actual = if idx < 0 { len + idx } else { idx };
                    if actual < 0 || actual >= len {
                        return Completion::Normal(JsValue::Undefined);
                    }
                    let ch = s
                        .chars()
                        .nth(actual as usize)
                        .map(|c| c.to_string())
                        .unwrap_or_default();
                    Completion::Normal(JsValue::String(JsString::from_str(&ch)))
                }),
            ),
            (
                "search",
                1,
                Rc::new(|interp, this_val, args| {
                    let regexp = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref o) = regexp {
                        if let Some(key) = interp.get_symbol_key("search") {
                            if let Some(obj) = interp.get_object(o.id) {
                                let method = obj.borrow().get_property(&key);
                                if !matches!(method, JsValue::Undefined | JsValue::Null) {
                                    let this_str = JsValue::String(JsString::from_str(&to_js_string(this_val)));
                                    return interp.call_function(&method, &regexp, &[this_str]);
                                }
                            }
                        }
                    }
                    let s = to_js_string(this_val);
                    let source = if args.is_empty() {
                        String::new()
                    } else {
                        to_js_string(&regexp)
                    };
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
                    let regexp = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref o) = regexp {
                        if let Some(key) = interp.get_symbol_key("match") {
                            if let Some(obj) = interp.get_object(o.id) {
                                let method = obj.borrow().get_property(&key);
                                if !matches!(method, JsValue::Undefined | JsValue::Null) {
                                    let this_str = JsValue::String(JsString::from_str(&to_js_string(this_val)));
                                    return interp.call_function(&method, &regexp, &[this_str]);
                                }
                            }
                        }
                    }
                    // Fallback: create a RegExp and call @@match
                    let s = to_js_string(this_val);
                    let source = if args.is_empty() {
                        String::new()
                    } else {
                        to_js_string(&regexp)
                    };
                    if let Ok(re) = regex::Regex::new(&source)
                        && let Some(m) = re.find(&s)
                    {
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
                }),
            ),
            (
                "matchAll",
                1,
                Rc::new(|interp, this_val, args| {
                    let regexp = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if let JsValue::Object(ref o) = regexp {
                        // Check if it's a regexp and if so, require global flag
                        if let Some(obj) = interp.get_object(o.id) {
                            let is_regexp = obj.borrow().class_name == "RegExp";
                            if is_regexp {
                                let flags_val = obj.borrow().get_property("flags");
                                let flags = to_js_string(&flags_val);
                                if !flags.contains('g') {
                                    let err = interp.create_type_error(
                                        "String.prototype.matchAll called with a non-global RegExp argument",
                                    );
                                    return Completion::Throw(err);
                                }
                            }
                        }
                        if let Some(key) = interp.get_symbol_key("matchAll") {
                            if let Some(obj) = interp.get_object(o.id) {
                                let method = obj.borrow().get_property(&key);
                                if !matches!(method, JsValue::Undefined | JsValue::Null) {
                                    let this_str = JsValue::String(JsString::from_str(&to_js_string(this_val)));
                                    return interp.call_function(&method, &regexp, &[this_str]);
                                }
                            }
                        }
                    }
                    let err = interp.create_type_error("matchAll requires a global RegExp");
                    Completion::Throw(err)
                }),
            ),
        ];

        for (name, arity, func) in methods {
            let fn_val = self.create_function(JsFunction::Native(name.to_string(), arity, func));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        // String.prototype[@@iterator]
        let str_iter_fn = self.create_function(JsFunction::native(
            "[Symbol.iterator]".to_string(),
            0,
            |interp, this_val, _args| {
                let s = match this_val {
                    JsValue::String(s) => s.clone(),
                    _ => {
                        let converted = to_js_string(this_val);
                        JsString::from_str(&converted)
                    }
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

        // Set String.prototype on the String constructor
        if let Some(str_val) = self.global_env.borrow().get("String")
            && let JsValue::Object(o) = &str_val
            && let Some(str_obj) = self.get_object(o.id)
        {
            let proto_val = JsValue::Object(crate::types::JsObject {
                id: proto.borrow().id.unwrap(),
            });
            str_obj
                .borrow_mut()
                .insert_value("prototype".to_string(), proto_val);
        }

        self.string_prototype = Some(proto);
    }

}
