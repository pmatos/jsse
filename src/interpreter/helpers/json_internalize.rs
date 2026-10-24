use super::*;

fn json_internalize_apply<K: PropertyKeyLike + ?Sized>(
    interp: &mut Interpreter,
    obj_id: u64,
    key: &K,
    new_val: JsValue,
) -> Result<(), JsValue> {
    let is_proxy = interp
        .get_object_cell(obj_id)
        .map(|c| c.borrow().is_proxy() || c.borrow().is_proxy_revoked())
        .unwrap_or(false);

    if is_proxy {
        let target_val = interp.get_proxy_target_val(obj_id);
        if new_val.is_undefined() {
            // Delete via proxy deleteProperty trap
            let key_val = JsValue::string(key.to_js_property_key().to_js_string());
            match interp.invoke_proxy_trap(obj_id, "deleteProperty", vec![target_val, key_val]) {
                Ok(Some(v)) => {
                    if !interp.to_boolean_val(&v) {
                        return Err(interp.create_type_error(
                            "'deleteProperty' on proxy: trap returned falsish",
                        ));
                    }
                }
                Ok(None) => {
                    // No trap, delete on target directly
                    if let Some(target_id) = interp.get_proxy_target_val(obj_id).as_object_id()
                        && let Some(tobj) = interp.get_object_cell(target_id)
                    {
                        tobj.borrow_mut().remove_property(key);
                    }
                }
                Err(e) => return Err(e),
            }
        } else {
            // CreateDataProperty via proxy defineProperty trap
            let key_val = JsValue::string(key.to_js_property_key().to_js_string());
            let desc_obj_id = interp.create_object_id();
            interp
                .get_object_cell_expect(desc_obj_id)
                .borrow_mut()
                .insert_value("value".to_string(), new_val.clone());
            interp
                .get_object_cell_expect(desc_obj_id)
                .borrow_mut()
                .insert_value("writable".to_string(), JsValue::TRUE);
            interp
                .get_object_cell_expect(desc_obj_id)
                .borrow_mut()
                .insert_value("enumerable".to_string(), JsValue::TRUE);
            interp
                .get_object_cell_expect(desc_obj_id)
                .borrow_mut()
                .insert_value("configurable".to_string(), JsValue::TRUE);
            let desc_val = JsValue::object(desc_obj_id);
            match interp.invoke_proxy_trap(
                obj_id,
                "defineProperty",
                vec![target_val, key_val, desc_val],
            ) {
                Ok(Some(v)) => {
                    if !interp.to_boolean_val(&v) {
                        return Err(interp.create_type_error(
                            "'defineProperty' on proxy: trap returned falsish",
                        ));
                    }
                }
                Ok(None) => {
                    // No trap, define on target directly
                    if let Some(target_id) = interp.get_proxy_target_val(obj_id).as_object_id()
                        && let Some(tobj) = interp.get_object_cell(target_id)
                    {
                        tobj.borrow_mut()
                            .insert_value(key.to_js_property_key(), new_val);
                    }
                }
                Err(e) => return Err(e),
            }
        }
        return Ok(());
    }

    // Non-proxy path
    if let Some(cell) = interp.get_object_cell(obj_id) {
        let configurable = cell
            .borrow()
            .properties
            .get(key)
            .and_then(|d| d.configurable())
            .unwrap_or(true);
        if !configurable {
            return Ok(());
        }
        if new_val.is_undefined() {
            cell.borrow_mut().remove_property(key);
            // Also clear dense array storage so get_property doesn't find stale values
            if let Some(key_str) = key.as_property_key_str()
                && let Some(idx) = crate::interpreter::types::array_element_index(key_str)
            {
                let mut b = cell.borrow_mut();
                if let Some(elems) = b.array_elements_mut()
                    && idx < elems.len()
                {
                    elems[idx] = JsValue::UNDEFINED;
                }
            }
        } else {
            cell.borrow_mut()
                .insert_value(key.to_js_property_key(), new_val);
        }
    }
    Ok(())
}

pub(crate) fn json_internalize<K: PropertyKeyLike + ?Sized>(
    interp: &mut Interpreter,
    holder: &JsValue,
    name: &K,
    reviver: &JsValue,
    source_map: &Option<SourceTextMap>,
    stand_ins: bool,
) -> Completion {
    interp.with_gc_root_scope(|interp| {
        interp.gc_root_value(holder);
        json_internalize_walk(interp, holder, name, reviver, source_map, stand_ins)
    })
}

fn json_internalize_walk<K: PropertyKeyLike + ?Sized>(
    interp: &mut Interpreter,
    holder: &JsValue,
    name: &K,
    reviver: &JsValue,
    source_map: &Option<SourceTextMap>,
    stand_ins: bool,
) -> Completion {
    let val = if let Some(holder_id) = holder.as_object_id() {
        match interp.get_object_property(holder_id, name, holder) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Completion::Throw(e),
            _ => JsValue::UNDEFINED,
        }
    } else {
        JsValue::UNDEFINED
    };
    interp.gc_root_value(&val);

    let walked = if let Some(obj_id) = val.as_object_id() {
        let is_array = match is_array_value(interp, obj_id) {
            Ok(v) => v,
            Err(e) => return Completion::Throw(e),
        };
        if is_array {
            let len_val = match interp.get_object_property(obj_id, "length", &val) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Completion::Throw(e),
                _ => JsValue::UNDEFINED,
            };
            let len = match interp.to_number_value(&len_val) {
                Ok(n) => n as usize,
                Err(e) => return Completion::Throw(e),
            };
            for i in 0..len {
                let key = i.to_string();
                match interp.with_gc_root_scope(|interp| {
                    json_internalize_walk(interp, &val, &key, reviver, source_map, stand_ins)
                }) {
                    Completion::Normal(new_val) => {
                        if let Err(e) = json_internalize_apply(interp, obj_id, &key, new_val) {
                            return Completion::Throw(e);
                        }
                    }
                    Completion::Throw(e) => return Completion::Throw(e),
                    _ => {}
                }
            }
        } else {
            let keys = match enumerable_own_keys(interp, obj_id) {
                Ok(k) => k,
                Err(e) => return Completion::Throw(e),
            };
            for key in keys {
                match interp.with_gc_root_scope(|interp| {
                    json_internalize_walk(interp, &val, &key, reviver, source_map, stand_ins)
                }) {
                    Completion::Normal(new_val) => {
                        if let Err(e) = json_internalize_apply(interp, obj_id, &key, new_val) {
                            return Completion::Throw(e);
                        }
                    }
                    Completion::Throw(e) => return Completion::Throw(e),
                    _ => {}
                }
            }
        }
        val.clone()
    } else {
        val.clone()
    };

    // Build context argument for reviver
    let context = {
        let ctx_id = interp.create_object_id();
        if is_json_primitive(&walked)
            && let Some(smap) = source_map
            && let Some(holder_id) = holder.as_object_id()
            && let Some(src) = smap.get(&(holder_id, name.to_js_property_key()))
        {
            // Verify the source text matches the actual value
            // (forward modifications make source invalid)
            let source_matches = if walked.is_null() {
                src == "null"
            } else if let Some(b) = walked.as_boolean() {
                src == if b { "true" } else { "false" }
            } else if let Some(n) = walked.as_number() {
                src.parse::<f64>()
                    .is_ok_and(|parsed| (parsed.is_nan() && n.is_nan()) || parsed == n)
            } else {
                walked
                    .with_string(|units| {
                        if !(src.starts_with('"') && src.ends_with('"')) {
                            return false;
                        }
                        // Source includes quotes, parse it to compare
                        let inner = &src[1..src.len() - 1];
                        json_unescape_js_string(inner, stand_ins).as_units() == units
                    })
                    .unwrap_or(false)
            };
            if source_matches {
                interp
                    .get_object_cell_expect(ctx_id)
                    .borrow_mut()
                    .insert_value(
                        "source".to_string(),
                        JsValue::string(if stand_ins {
                            crate::interpreter::builtins::regexp::regex_output_to_js_string(src)
                        } else {
                            JsString::from_str(src)
                        }),
                    );
            }
        }
        JsValue::object(ctx_id)
    };
    let key_val = JsValue::string(name.to_js_property_key().to_js_string());
    interp.call_function(reviver, holder, &[key_val, walked, context])
}
