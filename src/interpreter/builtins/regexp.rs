use super::super::*;

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

fn extract_source_flags(
    interp: &Interpreter,
    this_val: &JsValue,
) -> Option<(String, String, u64)> {
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
            .insert_value("lastIndex".to_string(), JsValue::Number(val));
    }
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

    let last_index = if global || sticky {
        let li = get_last_index(interp, this_id);
        let li_int = li as i64;
        if li_int < 0 || li_int as usize > input.len() {
            set_last_index(interp, this_id, 0.0);
            return Completion::Normal(JsValue::Null);
        }
        li_int as usize
    } else {
        0
    };

    let re = match build_rust_regex(source, flags) {
        Ok(r) => r,
        Err(_) => return Completion::Normal(JsValue::Null),
    };

    let captures = re.captures(&input[last_index..]);
    let caps = match captures {
        Some(c) => c,
        None => {
            if global || sticky {
                set_last_index(interp, this_id, 0.0);
            }
            return Completion::Normal(JsValue::Null);
        }
    };

    let full_match = caps.get(0).unwrap();
    let match_start = last_index + full_match.start();
    let match_end = last_index + full_match.end();

    if sticky && full_match.start() != 0 {
        set_last_index(interp, this_id, 0.0);
        return Completion::Normal(JsValue::Null);
    }

    if global || sticky {
        set_last_index(interp, this_id, match_end as f64);
    }

    let mut elements: Vec<JsValue> = Vec::new();
    elements.push(JsValue::String(JsString::from_str(full_match.as_str())));
    for i in 1..caps.len() {
        match caps.get(i) {
            Some(m) => elements.push(JsValue::String(JsString::from_str(m.as_str()))),
            None => elements.push(JsValue::Undefined),
        }
    }

    let result = interp.create_array(elements);
    if let JsValue::Object(ref ro) = result
        && let Some(robj) = interp.get_object(ro.id)
    {
        robj.borrow_mut().insert_value(
            "index".to_string(),
            JsValue::Number(match_start as f64),
        );
        robj.borrow_mut().insert_value(
            "input".to_string(),
            JsValue::String(JsString::from_str(input)),
        );
        robj.borrow_mut()
            .insert_value("groups".to_string(), JsValue::Undefined);
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

fn apply_replacement_pattern(
    template: &str,
    matched: &str,
    captures: &[String],
    position: usize,
    input: &str,
) -> String {
    let mut result = String::new();
    let bytes = template.as_bytes();
    let len = bytes.len();
    let mut i = 0;
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
                    result.push_str(&input[..position]);
                    i += 2;
                }
                b'\'' => {
                    let end = position + matched.len();
                    if end < input.len() {
                        result.push_str(&input[end..]);
                    }
                    i += 2;
                }
                c if c.is_ascii_digit() => {
                    let d1 = (c - b'0') as usize;
                    // Check for two-digit reference
                    if i + 2 < len && bytes[i + 2].is_ascii_digit() {
                        let d2 = (bytes[i + 2] - b'0') as usize;
                        let nn = d1 * 10 + d2;
                        if nn >= 1 && nn <= captures.len() {
                            result.push_str(&captures[nn - 1]);
                            i += 3;
                            continue;
                        }
                    }
                    if d1 >= 1 && d1 <= captures.len() {
                        result.push_str(&captures[d1 - 1]);
                    } else {
                        result.push('$');
                        result.push(c as char);
                    }
                    i += 2;
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
    result
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
                let input = args.first().map(to_js_string).unwrap_or_default();
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
                let input = args.first().map(to_js_string).unwrap_or_default();
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
                let s = args.first().map(to_js_string).unwrap_or_default();
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
            regexp_proto.borrow_mut().insert_property(
                key,
                PropertyDescriptor::data(match_fn, true, false, true),
            );
        }

        // [@@search] (§22.2.5.9)
        let search_fn = self.create_function(JsFunction::native(
            "[Symbol.search]".to_string(),
            1,
            |interp, this_val, args| {
                let s = args.first().map(to_js_string).unwrap_or_default();
                let (source, flags, obj_id) = match extract_source_flags(interp, this_val) {
                    Some(v) => v,
                    None => return Completion::Normal(JsValue::Number(-1.0)),
                };
                let prev_last_index = get_last_index(interp, obj_id);
                set_last_index(interp, obj_id, 0.0);
                let result = regexp_exec_raw(interp, obj_id, &source, &flags, &s);
                set_last_index(interp, obj_id, prev_last_index);
                match result {
                    Completion::Normal(JsValue::Null) => {
                        Completion::Normal(JsValue::Number(-1.0))
                    }
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
            regexp_proto.borrow_mut().insert_property(
                key,
                PropertyDescriptor::data(search_fn, true, false, true),
            );
        }

        // [@@replace] (§22.2.5.8)
        let replace_fn = self.create_function(JsFunction::native(
            "[Symbol.replace]".to_string(),
            2,
            |interp, this_val, args| {
                let s = args.first().map(to_js_string).unwrap_or_default();
                let replace_value = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let (source, flags, obj_id) = match extract_source_flags(interp, this_val) {
                    Some(v) => v,
                    None => return Completion::Normal(JsValue::String(JsString::from_str(&s))),
                };
                let global = flags.contains('g');
                let func_replacer = if let JsValue::Object(ref o) = replace_value {
                    interp
                        .get_object(o.id)
                        .map(|obj| obj.borrow().callable.is_some())
                        .unwrap_or(false)
                } else {
                    false
                };

                if global {
                    set_last_index(interp, obj_id, 0.0);
                }

                let mut results: Vec<(usize, usize, String)> = Vec::new();
                loop {
                    let exec_result =
                        regexp_exec_raw(interp, obj_id, &source, &flags, &s);
                    match exec_result {
                        Completion::Normal(JsValue::Null) => break,
                        Completion::Normal(JsValue::Object(ref o)) => {
                            if let Some(arr) = interp.get_object(o.id) {
                                let matched_val = arr.borrow().get_property("0");
                                let matched = to_js_string(&matched_val);
                                let index_val = arr.borrow().get_property("index");
                                let position = to_number(&index_val) as usize;

                                // Collect captures
                                let mut captures: Vec<String> = Vec::new();
                                let length_val = arr.borrow().get_property("length");
                                let n_captures = to_number(&length_val) as usize;
                                for i in 1..n_captures {
                                    let cap = arr.borrow().get_property(&i.to_string());
                                    captures.push(to_js_string(&cap));
                                }

                                let replacement = if func_replacer {
                                    let mut call_args = vec![matched_val.clone()];
                                    for cap in &captures {
                                        call_args.push(JsValue::String(JsString::from_str(cap)));
                                    }
                                    call_args.push(JsValue::Number(position as f64));
                                    call_args.push(JsValue::String(JsString::from_str(&s)));
                                    let r = interp.call_function(
                                        &replace_value,
                                        &JsValue::Undefined,
                                        &call_args,
                                    );
                                    match r {
                                        Completion::Normal(v) => to_js_string(&v),
                                        other => return other,
                                    }
                                } else {
                                    let template = to_js_string(&replace_value);
                                    apply_replacement_pattern(
                                        &template, &matched, &captures, position, &s,
                                    )
                                };

                                results.push((position, matched.len(), replacement));

                                if matched.is_empty() {
                                    let li = get_last_index(interp, obj_id);
                                    set_last_index(interp, obj_id, li + 1.0);
                                }
                            } else {
                                break;
                            }
                        }
                        other => return other,
                    }
                    if !global {
                        break;
                    }
                }

                // Build result string
                let mut output = String::new();
                let mut last_end = 0;
                for (pos, match_len, replacement) in &results {
                    if *pos >= last_end {
                        output.push_str(&s[last_end..*pos]);
                    }
                    output.push_str(replacement);
                    last_end = pos + match_len;
                }
                if last_end < s.len() {
                    output.push_str(&s[last_end..]);
                }
                Completion::Normal(JsValue::String(JsString::from_str(&output)))
            },
        ));
        if let Some(key) = get_symbol_key(self, "replace") {
            regexp_proto.borrow_mut().insert_property(
                key,
                PropertyDescriptor::data(replace_fn, true, false, true),
            );
        }

        // [@@split] (§22.2.5.11)
        let split_fn = self.create_function(JsFunction::native(
            "[Symbol.split]".to_string(),
            2,
            |interp, this_val, args| {
                let s = args.first().map(to_js_string).unwrap_or_default();
                let limit = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let lim = if matches!(limit, JsValue::Undefined) {
                    u32::MAX
                } else {
                    to_number(&limit) as u32
                };
                let (source, flags, _obj_id) = match extract_source_flags(interp, this_val) {
                    Some(v) => v,
                    None => {
                        return Completion::Normal(
                            interp
                                .create_array(vec![JsValue::String(JsString::from_str(&s))]),
                        )
                    }
                };
                if lim == 0 {
                    return Completion::Normal(interp.create_array(vec![]));
                }

                if s.is_empty() {
                    let re = match build_rust_regex(&source, &flags) {
                        Ok(r) => r,
                        Err(_) => {
                            return Completion::Normal(
                                interp.create_array(vec![JsValue::String(JsString::from_str(""))]),
                            )
                        }
                    };
                    if re.is_match("") {
                        return Completion::Normal(interp.create_array(vec![]));
                    } else {
                        return Completion::Normal(
                            interp.create_array(vec![JsValue::String(JsString::from_str(""))]),
                        );
                    }
                }

                let re = match build_rust_regex(&source, &flags) {
                    Ok(r) => r,
                    Err(_) => {
                        return Completion::Normal(
                            interp.create_array(vec![JsValue::String(JsString::from_str(&s))]),
                        )
                    }
                };

                let mut result: Vec<JsValue> = Vec::new();
                let mut last_end = 0;

                for caps in re.captures_iter(&s) {
                    let full = caps.get(0).unwrap();
                    if full.start() == full.end() && full.start() == last_end {
                        continue;
                    }
                    result.push(JsValue::String(JsString::from_str(
                        &s[last_end..full.start()],
                    )));
                    if result.len() as u32 >= lim {
                        return Completion::Normal(interp.create_array(result));
                    }
                    // Add capture groups
                    for i in 1..caps.len() {
                        match caps.get(i) {
                            Some(m) => result
                                .push(JsValue::String(JsString::from_str(m.as_str()))),
                            None => result.push(JsValue::Undefined),
                        }
                        if result.len() as u32 >= lim {
                            return Completion::Normal(interp.create_array(result));
                        }
                    }
                    last_end = full.end();
                }
                result.push(JsValue::String(JsString::from_str(&s[last_end..])));
                Completion::Normal(interp.create_array(result))
            },
        ));
        if let Some(key) = get_symbol_key(self, "split") {
            regexp_proto.borrow_mut().insert_property(
                key,
                PropertyDescriptor::data(split_fn, true, false, true),
            );
        }

        // [@@matchAll] (§22.2.5.7)
        let match_all_fn = self.create_function(JsFunction::native(
            "[Symbol.matchAll]".to_string(),
            1,
            |interp, this_val, args| {
                let s = args.first().map(to_js_string).unwrap_or_default();
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

                iter_obj.borrow_mut().iterator_state =
                    Some(IteratorState::RegExpStringIterator {
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

                                let re = match build_rust_regex(source, flags) {
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
                                        interp.create_iter_result_object(
                                            JsValue::Undefined,
                                            true,
                                        ),
                                    );
                                }

                                let captures = re.captures(&string[last_index..]);
                                match captures {
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
                                        let match_start = last_index + full.start();
                                        let match_end = last_index + full.end();

                                        let mut elements: Vec<JsValue> = Vec::new();
                                        elements.push(JsValue::String(JsString::from_str(
                                            full.as_str(),
                                        )));
                                        for i in 1..caps.len() {
                                            match caps.get(i) {
                                                Some(m) => elements.push(JsValue::String(
                                                    JsString::from_str(m.as_str()),
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
                                            if full.as_str().is_empty() {
                                                match_end + 1
                                            } else {
                                                match_end
                                            }
                                        } else {
                                            last_index // non-global: always same position -> will mark done next
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

        let regexp_proto_rc = regexp_proto.clone();

        // RegExp constructor
        let regexp_ctor = self.create_function(JsFunction::native(
            "RegExp".to_string(),
            2,
            move |interp, _this, args| {
                let pattern_str = args.first().map(to_js_string).unwrap_or_default();
                let flags_str = args.get(1).map(to_js_string).unwrap_or_default();
                let mut obj = JsObjectData::new();
                obj.prototype = Some(regexp_proto_rc.clone());
                obj.class_name = "RegExp".to_string();
                obj.insert_value(
                    "source".to_string(),
                    JsValue::String(JsString::from_str(&pattern_str)),
                );
                obj.insert_value(
                    "flags".to_string(),
                    JsValue::String(JsString::from_str(&flags_str)),
                );
                obj.insert_value(
                    "global".to_string(),
                    JsValue::Boolean(flags_str.contains('g')),
                );
                obj.insert_value(
                    "ignoreCase".to_string(),
                    JsValue::Boolean(flags_str.contains('i')),
                );
                obj.insert_value(
                    "multiline".to_string(),
                    JsValue::Boolean(flags_str.contains('m')),
                );
                obj.insert_value(
                    "dotAll".to_string(),
                    JsValue::Boolean(flags_str.contains('s')),
                );
                obj.insert_value(
                    "unicode".to_string(),
                    JsValue::Boolean(flags_str.contains('u')),
                );
                obj.insert_value(
                    "sticky".to_string(),
                    JsValue::Boolean(flags_str.contains('y')),
                );
                obj.insert_value("lastIndex".to_string(), JsValue::Number(0.0));
                let rc = Rc::new(RefCell::new(obj));
                let id = interp.allocate_object_slot(rc);
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
            },
        ));
        if let JsValue::Object(ref o) = regexp_ctor
            && let Some(obj) = self.get_object(o.id)
        {
            obj.borrow_mut().insert_value(
                "prototype".to_string(),
                JsValue::Object(crate::types::JsObject {
                    id: regexp_proto.borrow().id.unwrap(),
                }),
            );
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
