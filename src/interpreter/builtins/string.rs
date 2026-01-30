use super::super::*;

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
                    let s = to_js_string(this_val);
                    let len = s.len() as i64;
                    let start = args
                        .first()
                        .map(|v| {
                            let n = to_number(v) as i64;
                            if n < 0 {
                                (len + n).max(0) as usize
                            } else {
                                n.min(len) as usize
                            }
                        })
                        .unwrap_or(0);
                    let end = args
                        .get(1)
                        .map(|v| {
                            if matches!(v, JsValue::Undefined) {
                                len as usize
                            } else {
                                let n = to_number(v) as i64;
                                if n < 0 {
                                    (len + n).max(0) as usize
                                } else {
                                    n.min(len) as usize
                                }
                            }
                        })
                        .unwrap_or(len as usize);
                    let result = if start < end { &s[start..end] } else { "" };
                    Completion::Normal(JsValue::String(JsString::from_str(result)))
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
                        to_js_string(this_val).trim(),
                    )))
                }),
            ),
            (
                "trimStart",
                0,
                Rc::new(|_interp, this_val, _args| {
                    Completion::Normal(JsValue::String(JsString::from_str(
                        to_js_string(this_val).trim_start(),
                    )))
                }),
            ),
            (
                "trimEnd",
                0,
                Rc::new(|_interp, this_val, _args| {
                    Completion::Normal(JsValue::String(JsString::from_str(
                        to_js_string(this_val).trim_end(),
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
                    let s = to_js_string(this_val);
                    let separator = args.first();
                    let parts: Vec<JsValue> = if let Some(sep) = separator {
                        if matches!(sep, JsValue::Undefined) {
                            vec![JsValue::String(JsString::from_str(&s))]
                        } else {
                            let sep_str = to_js_string(sep);
                            if sep_str.is_empty() {
                                s.chars()
                                    .map(|c| JsValue::String(JsString::from_str(&c.to_string())))
                                    .collect()
                            } else {
                                s.split(&sep_str)
                                    .map(|p| JsValue::String(JsString::from_str(p)))
                                    .collect()
                            }
                        }
                    } else {
                        vec![JsValue::String(JsString::from_str(&s))]
                    };
                    let arr = interp.create_array(parts);
                    Completion::Normal(arr)
                }),
            ),
            (
                "replace",
                2,
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let search = args.first().map(to_js_string).unwrap_or_default();
                    let replacement = args.get(1).map(to_js_string).unwrap_or_default();
                    let result = s.replacen(&search, &replacement, 1);
                    Completion::Normal(JsValue::String(JsString::from_str(&result)))
                }),
            ),
            (
                "replaceAll",
                2,
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let search = args.first().map(to_js_string).unwrap_or_default();
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
                    let s = to_js_string(this_val);
                    let (source, flags) = match args.first() {
                        Some(JsValue::Object(o)) => {
                            let obj = interp.get_object(o.id);
                            let src = obj
                                .as_ref()
                                .map(|ob| {
                                    if let JsValue::String(sv) = ob.borrow().get_property("source")
                                    {
                                        sv.to_rust_string()
                                    } else {
                                        String::new()
                                    }
                                })
                                .unwrap_or_default();
                            let fl = obj
                                .as_ref()
                                .map(|ob| {
                                    if let JsValue::String(sv) = ob.borrow().get_property("flags") {
                                        sv.to_rust_string()
                                    } else {
                                        String::new()
                                    }
                                })
                                .unwrap_or_default();
                            (src, fl)
                        }
                        Some(v) => (to_js_string(v), String::new()),
                        None => (String::new(), String::new()),
                    };
                    let pat = if flags.contains('i') {
                        format!("(?i){}", source)
                    } else {
                        source
                    };
                    if let Ok(re) = regex::Regex::new(&pat)
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
                    let s = to_js_string(this_val);
                    let (source, flags) = match args.first() {
                        Some(JsValue::Object(o)) => {
                            let obj = interp.get_object(o.id);
                            let src = obj
                                .as_ref()
                                .map(|ob| {
                                    if let JsValue::String(sv) = ob.borrow().get_property("source")
                                    {
                                        sv.to_rust_string()
                                    } else {
                                        String::new()
                                    }
                                })
                                .unwrap_or_default();
                            let fl = obj
                                .as_ref()
                                .map(|ob| {
                                    if let JsValue::String(sv) = ob.borrow().get_property("flags") {
                                        sv.to_rust_string()
                                    } else {
                                        String::new()
                                    }
                                })
                                .unwrap_or_default();
                            (src, fl)
                        }
                        Some(v) => (to_js_string(v), String::new()),
                        None => (String::new(), String::new()),
                    };
                    let pat = if flags.contains('i') {
                        format!("(?i){}", source)
                    } else {
                        source
                    };
                    let re = match regex::Regex::new(&pat) {
                        Ok(r) => r,
                        Err(_) => return Completion::Normal(JsValue::Null),
                    };
                    if flags.contains('g') {
                        let matches: Vec<JsValue> = re
                            .find_iter(&s)
                            .map(|m| JsValue::String(JsString::from_str(m.as_str())))
                            .collect();
                        if matches.is_empty() {
                            Completion::Normal(JsValue::Null)
                        } else {
                            Completion::Normal(interp.create_array(matches))
                        }
                    } else if let Some(m) = re.find(&s) {
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
