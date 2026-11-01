use super::*;

fn push_ascii(out: &mut Vec<u16>, text: &str) {
    out.extend(text.bytes().map(u16::from));
}

fn push_unicode_escape(out: &mut Vec<u16>, unit: u16) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    push_ascii(out, "\\u");
    for shift in [12, 8, 4, 0] {
        out.push(u16::from(HEX[usize::from((unit >> shift) & 0xF)]));
    }
}

// §25.5.2.3 QuoteJSONString over code units: a paired surrogate is copied, a
// lone one is escaped so the output is well-formed.
fn json_quote_units_into(result: &mut Vec<u16>, units: &[u16]) {
    result.reserve(units.len() + 2);
    result.push(0x22);
    let mut i = 0;
    while i < units.len() {
        let cu = units[i];
        match cu {
            0x0022 => push_ascii(result, "\\\""),
            0x005C => push_ascii(result, "\\\\"),
            0x0008 => push_ascii(result, "\\b"),
            0x000C => push_ascii(result, "\\f"),
            0x000A => push_ascii(result, "\\n"),
            0x000D => push_ascii(result, "\\r"),
            0x0009 => push_ascii(result, "\\t"),
            c if c < 0x0020 => push_unicode_escape(result, c),
            c if (0xD800..=0xDBFF).contains(&c) => {
                if i + 1 < units.len() && (0xDC00..=0xDFFF).contains(&units[i + 1]) {
                    result.push(c);
                    result.push(units[i + 1]);
                    i += 1;
                } else {
                    push_unicode_escape(result, c);
                }
            }
            c if (0xDC00..=0xDFFF).contains(&c) => push_unicode_escape(result, c),
            c => result.push(c),
        }
        i += 1;
    }
    result.push(0x22);
}

// `val` and `replacer` are the native call's arguments, which the call
// dispatch keeps rooted for the whole call.
pub(crate) fn json_stringify_full(
    interp: &mut Interpreter,
    val: &JsValue,
    replacer: &Option<JsValue>,
    space: &[u16],
) -> Result<Option<Vec<u16>>, JsValue> {
    let mut stack = Vec::new();
    let mut property_list: Option<Vec<JsPropertyKey>> = None;
    let mut replacer_fn: Option<JsValue> = None;

    if let Some(rep) = replacer
        && let Some(rep_id) = rep.as_object_id()
        && let Some(obj) = interp.get_object_cell(rep_id)
    {
        if obj.borrow().callable.is_some() {
            replacer_fn = Some(rep.clone());
        } else if is_array_value(interp, rep_id)? {
            let mut keys = Vec::new();
            let obj_val = JsValue::object(rep_id);
            let len_val = match interp.get_object_property(rep_id, "length", &obj_val) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::UNDEFINED,
            };
            let len = {
                let n = interp.to_number_value(&len_val)?;
                n as usize
            };
            for i in 0..len {
                let item = match interp.get_object_property(rep_id, &i.to_string(), &obj_val) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                };
                let key_str = if let Some(s) = item.as_string() {
                    Some(JsPropertyKey::from_js_string(&s))
                } else if let Some(n) = item.as_number() {
                    Some(JsPropertyKey::from(number_ops::to_string(n)))
                } else if let Some(item_id) = item.as_object_id() {
                    if let Some(inner) = interp.get_object_cell(item_id) {
                        let cn = inner.borrow().class_name;
                        if cn == "String" || cn == "Number" {
                            let s = interp.with_gc_root_scope(|interp| {
                                interp.gc_root_value(&item);
                                interp.to_js_string(&item)
                            })?;
                            Some(JsPropertyKey::from(s))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(k) = key_str
                    && !keys.contains(&k)
                {
                    keys.push(k);
                }
            }
            property_list = Some(keys);
        }
    }

    let wrapper_id = interp.create_object_id();
    interp
        .get_object_cell_expect(wrapper_id)
        .borrow_mut()
        .insert_value("".to_string(), val.clone());
    let holder_id = wrapper_id;

    let root_key = JsPropertyKey::from_str("");
    let mut out = Vec::new();
    let produced = json_stringify_internal(
        interp,
        holder_id,
        &root_key,
        val,
        &mut stack,
        &replacer_fn,
        &property_list,
        space,
        &[],
        &mut out,
    )?;
    Ok(produced.then_some(out))
}

#[allow(clippy::too_many_arguments)]
fn json_stringify_internal(
    interp: &mut Interpreter,
    holder_id: u64,
    key: &JsPropertyKey,
    val: &JsValue,
    stack: &mut Vec<u64>,
    replacer_fn: &Option<JsValue>,
    property_list: &Option<Vec<JsPropertyKey>>,
    gap: &[u16],
    indent: &[u16],
    out: &mut Vec<u16>,
) -> Result<bool, JsValue> {
    interp.with_gc_root_scope(|interp| {
        interp.gc_root_id(holder_id);
        interp.gc_root_value(val);
        json_stringify_walk(
            interp,
            holder_id,
            key,
            val,
            stack,
            replacer_fn,
            property_list,
            gap,
            indent,
            out,
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn json_stringify_walk(
    interp: &mut Interpreter,
    holder_id: u64,
    key: &JsPropertyKey,
    val: &JsValue,
    stack: &mut Vec<u64>,
    replacer_fn: &Option<JsValue>,
    property_list: &Option<Vec<JsPropertyKey>>,
    gap: &[u16],
    indent: &[u16],
    out: &mut Vec<u16>,
) -> Result<bool, JsValue> {
    let mut value = val.clone();

    // Step 2: If Type(value) is Object or BigInt, check for toJSON
    let check_tojson = value.is_object() || value.is_bigint();
    if check_tojson {
        let to_json = if let Some(value_id) = value.as_object_id() {
            match interp.get_object_property(value_id, "toJSON", &value) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::UNDEFINED,
            }
        } else if value.is_bigint() {
            let obj_val = match interp.to_object(&value) {
                Completion::Normal(v) => v,
                _ => JsValue::UNDEFINED,
            };
            if let Some(wrapper_id) = obj_val.as_object_id() {
                // Use original BigInt value as receiver for proper getter behavior
                match interp.get_object_property(wrapper_id, "toJSON", &value) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                }
            } else {
                JsValue::UNDEFINED
            }
        } else {
            JsValue::UNDEFINED
        };
        if let Some(to_json_id) = to_json.as_object_id()
            && let Some(fdata) = interp.get_object_cell(to_json_id)
            && fdata.borrow().callable.is_some()
        {
            let key_val = JsValue::string(key.to_js_string());
            interp.gc_root_value(&to_json);
            match interp.call_function(&to_json, &value, &[key_val]) {
                Completion::Normal(v) => {
                    interp.gc_root_value(&v);
                    value = v;
                }
                Completion::Throw(e) => return Err(e),
                _ => {}
            }
        }
    }

    // Step 3: Apply replacer function
    if let Some(rep) = replacer_fn {
        let holder_val = JsValue::object(holder_id);
        let key_val = JsValue::string(key.to_js_string());
        match interp.call_function(rep, &holder_val, &[key_val, value.clone()]) {
            Completion::Normal(v) => {
                interp.gc_root_value(&v);
                value = v;
            }
            Completion::Throw(e) => return Err(e),
            _ => {}
        }
    }

    // Step 4: Unwrap wrapper objects per spec (ToNumber/ToString trigger valueOf/toString)
    if let Some(value_id) = value.as_object_id() {
        let class = if let Some(cell) = interp.get_object_cell(value_id) {
            cell.borrow().class_name
        } else {
            ""
        };
        match class {
            "Number" => {
                let n = interp.to_number_value(&value)?;
                value = JsValue::number(n)
            }
            "String" => value = JsValue::string(interp.to_js_string(&value)?),
            "Boolean" => {
                if let Some(cell) = interp.get_object_cell(value_id)
                    && let Some(pv) = cell.borrow().primitive_value().cloned()
                {
                    value = pv;
                }
            }
            "BigInt" => {
                if let Some(cell) = interp.get_object_cell(value_id)
                    && let Some(pv) = cell.borrow().primitive_value().cloned()
                {
                    value = pv;
                }
            }
            _ => {}
        }
    }

    match value.kind() {
        ValueKind::Null => push_ascii(out, "null"),
        ValueKind::Boolean => push_ascii(
            out,
            if value.as_boolean().unwrap() {
                "true"
            } else {
                "false"
            },
        ),
        ValueKind::Number => {
            let n = value.as_number().unwrap();
            if n.is_nan() || n.is_infinite() {
                push_ascii(out, "null");
            } else {
                push_ascii(out, &number_ops::to_string(n));
            }
        }
        ValueKind::String => {
            value.with_string(|units| json_quote_units_into(out, units));
        }
        ValueKind::BigInt => {
            return Err(interp.create_error("TypeError", "Do not know how to serialize a BigInt"));
        }
        ValueKind::Object => {
            let Some(obj) = interp.get_object_cell(value.as_object_id().unwrap()) else {
                push_ascii(out, "null");
                return Ok(true);
            };
            if obj.borrow().is_raw_json
                && let Some(raw) = obj.borrow().get_property_value("rawJSON")
                && let Some(text) = raw.as_string()
            {
                out.extend_from_slice(text.as_units());
                return Ok(true);
            }
            if obj.borrow().callable.is_some() {
                return Ok(false);
            }
            let obj_id = obj.id();
            if stack.contains(&obj_id) {
                return Err(interp.json_cycle_error(stack, obj_id, key));
            }
            stack.push(obj_id);
            interp.json_key_push(key);

            let is_array = is_array_value(interp, obj_id)?;
            let obj_val = JsValue::object(obj_id);
            let new_indent = [indent, gap].concat();
            // Each member starts on its own line, indented, when there is a gap.
            let begin_member = |out: &mut Vec<u16>, first: bool| {
                if !first {
                    out.push(0x2C);
                }
                if !gap.is_empty() {
                    out.push(0x0A);
                    out.extend_from_slice(&new_indent);
                }
            };
            let end_members = |out: &mut Vec<u16>, any: bool| {
                if any && !gap.is_empty() {
                    out.push(0x0A);
                    out.extend_from_slice(indent);
                }
            };

            let result: Result<(), JsValue> = (|| {
                if is_array {
                    let len_val = match interp.get_object_property(obj_id, "length", &obj_val) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Err(e),
                        _ => JsValue::UNDEFINED,
                    };
                    let len = interp.to_number_value(&len_val)? as usize;
                    out.push(0x5B);
                    for i in 0..len {
                        let ikey = JsPropertyKey::from(i.to_string());
                        let v = match interp.get_object_property(obj_id, &ikey, &obj_val) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Err(e),
                            _ => JsValue::UNDEFINED,
                        };
                        begin_member(out, i == 0);
                        let produced = json_stringify_internal(
                            interp,
                            obj_id,
                            &ikey,
                            &v,
                            stack,
                            replacer_fn,
                            property_list,
                            gap,
                            &new_indent,
                            out,
                        )?;
                        if !produced {
                            push_ascii(out, "null");
                        }
                    }
                    end_members(out, len > 0);
                    out.push(0x5D);
                } else {
                    let keys: Vec<JsPropertyKey> = if let Some(pl) = property_list {
                        pl.clone()
                    } else {
                        enumerable_own_keys(interp, obj_id)?
                    };
                    out.push(0x7B);
                    let mut any = false;
                    for k in &keys {
                        let v = match interp.get_object_property(obj_id, k, &obj_val) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Err(e),
                            _ => JsValue::UNDEFINED,
                        };
                        let mark = out.len();
                        begin_member(out, !any);
                        json_quote_units_into(out, k.to_js_string().as_units());
                        out.push(0x3A);
                        if !gap.is_empty() {
                            out.push(0x20);
                        }
                        let produced = json_stringify_internal(
                            interp,
                            obj_id,
                            k,
                            &v,
                            stack,
                            replacer_fn,
                            property_list,
                            gap,
                            &new_indent,
                            out,
                        )?;
                        if produced {
                            any = true;
                        } else {
                            out.truncate(mark);
                        }
                    }
                    end_members(out, any);
                    out.push(0x7D);
                }
                Ok(())
            })();
            stack.pop();
            interp.json_key_pop();
            result?;
        }
        ValueKind::Undefined | ValueKind::Symbol => return Ok(false),
    }
    Ok(true)
}
