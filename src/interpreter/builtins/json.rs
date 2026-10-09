use super::super::*;

impl Interpreter {
    pub(super) fn setup_json(&mut self) {
        let json_obj_id = self.create_object_id();
        self.add_json_stringify(json_obj_id);
        self.add_json_parse(json_obj_id);
        self.add_json_raw_json(json_obj_id);
        self.add_json_is_raw_json(json_obj_id);
        // @@toStringTag
        self.define_to_string_tag(json_obj_id, "JSON");
        let json_val = JsValue::object(json_obj_id);
        self.realm()
            .global_env
            .borrow_mut()
            .declare("JSON", BindingKind::Var);
        let env = self.realm().global_env.clone();
        let _ = self.env_set(&env, "JSON", json_val);
    }

    fn add_json_stringify(&mut self, json_obj_id: u64) {
        let json_stringify = self.create_function(JsFunction::native(
            "stringify".to_string(),
            3,
            |interp, _this, args: &[JsValue]| {
                let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let replacer_arg = args.get(1).cloned();
                let space_arg = args.get(2).cloned().unwrap_or(JsValue::UNDEFINED);

                // Process space argument per spec (ToNumber for Number objects, ToString for String objects)
                let mut space_val = space_arg;
                if let Some(space_id) = space_val.as_object_id()
                    && let Some(obj) = interp.get_object_cell(space_id)
                {
                    let cn = obj.borrow().class_name.clone();
                    if cn == "Number" {
                        match interp.to_number_value(&space_val) {
                            Ok(n) => space_val = JsValue::number(n),
                            Err(e) => return Completion::Throw(e),
                        }
                    } else if cn == "String" {
                        match interp.to_string_value(&space_val) {
                            Ok(s) => space_val = JsValue::string(JsString::from_str(&s)),
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                }
                let gap = if let Some(n) = space_val.as_number() {
                    let count = (n as i64).clamp(0, 10) as usize;
                    " ".repeat(count)
                } else if let Some(s) = space_val.as_string() {
                    let rs = s.to_rust_string();
                    if rs.len() > 10 {
                        rs[..10].to_string()
                    } else {
                        rs
                    }
                } else {
                    String::new()
                };

                let replacer_is_undefined = match &replacer_arg {
                    Some(value) => value.is_undefined(),
                    None => true,
                };
                let replacer = if replacer_is_undefined {
                    None
                } else {
                    replacer_arg
                };

                match json_stringify_full(interp, &val, &replacer, &gap) {
                    Ok(Some(s)) => Completion::Normal(JsValue::string(JsString::from_str(&s))),
                    Ok(None) => Completion::Normal(JsValue::UNDEFINED),
                    Err(e) => Completion::Throw(e),
                }
            },
        ));
        self.get_object_cell_expect(json_obj_id)
            .borrow_mut()
            .insert_builtin("stringify".to_string(), json_stringify);
    }

    fn add_json_parse(&mut self, json_obj_id: u64) {
        let json_parse = self.create_function(JsFunction::native(
            "parse".to_string(),
            2,
            |interp, _this, args: &[JsValue]| {
                let raw = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let s = match interp.to_string_value(&raw) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let reviver = args.get(1).cloned();

                let has_reviver =
                    if let Some(reviver_id) = reviver.as_ref().and_then(JsValue::as_object_id) {
                        interp
                            .get_object_cell(reviver_id)
                            .map(|o| o.borrow().callable.is_some())
                            .unwrap_or(false)
                    } else {
                        false
                    };

                if has_reviver {
                    let (result, smap) = json_parse_value_with_source(interp, &s);
                    match result {
                        Completion::Normal(parsed) => {
                            let wrapper_id = interp.create_object_id();
                            interp
                                .get_object_cell_expect(wrapper_id)
                                .borrow_mut()
                                .insert_value("".to_string(), parsed.clone());
                            // Store source text for top-level primitive
                            let source_map = if is_json_primitive(&parsed) {
                                let mut sm = smap;
                                sm.insert(
                                    (wrapper_id, JsPropertyKey::from_str("")),
                                    s.trim().to_string(),
                                );
                                Some(sm)
                            } else {
                                Some(smap)
                            };
                            let wrapper_val = JsValue::object(wrapper_id);
                            json_internalize(
                                interp,
                                &wrapper_val,
                                "",
                                reviver.as_ref().unwrap(),
                                &source_map,
                            )
                        }
                        other => other,
                    }
                } else {
                    json_parse_value(interp, &s)
                }
            },
        ));
        self.get_object_cell_expect(json_obj_id)
            .borrow_mut()
            .insert_builtin("parse".to_string(), json_parse);
    }

    fn add_json_raw_json(&mut self, json_obj_id: u64) {
        let json_raw_json = self.create_function(JsFunction::native(
            "rawJSON".to_string(),
            1,
            |interp, _this, args: &[JsValue]| {
                let raw = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let text = match interp.to_string_value(&raw) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                // Reject empty, leading/trailing whitespace
                if text.is_empty() {
                    let err = interp.create_error(
                        "SyntaxError",
                        "JSON.rawJSON cannot be called with an empty string",
                    );
                    return Completion::Throw(err);
                }
                let first = text.as_bytes()[0];
                let last = text.as_bytes()[text.len() - 1];
                if matches!(first, b'\t' | b'\n' | b'\r' | b' ')
                    || matches!(last, b'\t' | b'\n' | b'\r' | b' ')
                {
                    let err = interp.create_error(
                        "SyntaxError",
                        "JSON.rawJSON text must not start or end with whitespace",
                    );
                    return Completion::Throw(err);
                }
                // Must be a valid JSON primitive (not object/array)
                if text.starts_with('{') || text.starts_with('[') {
                    let err = interp
                        .create_error("SyntaxError", "JSON.rawJSON only accepts JSON primitives");
                    return Completion::Throw(err);
                }
                // Validate it's valid JSON
                if let Completion::Throw(e) = json_parse_value(interp, &text) {
                    return Completion::Throw(e);
                }
                let obj_id = interp.create_object_id();
                interp
                    .get_object_cell_expect(obj_id)
                    .borrow_mut()
                    .prototype_id = None;
                {
                    let mut o = interp.get_object_cell_expect(obj_id).borrow_mut();
                    let desc = PropertyDescriptor::data(
                        JsValue::string(JsString::from_str(&text)),
                        false,
                        true,
                        false,
                    );
                    let key = crate::interpreter::key_intern::intern_key("rawJSON");
                    o.property_order.push(key.clone());
                    o.properties.insert(key, desc);
                }
                interp
                    .get_object_cell_expect(obj_id)
                    .borrow_mut()
                    .extensible = false;
                interp
                    .get_object_cell_expect(obj_id)
                    .borrow_mut()
                    .is_raw_json = true;
                let id = obj_id;
                Completion::Normal(JsValue::object(id))
            },
        ));
        self.get_object_cell_expect(json_obj_id)
            .borrow_mut()
            .insert_builtin("rawJSON".to_string(), json_raw_json);
    }

    fn add_json_is_raw_json(&mut self, json_obj_id: u64) {
        let json_is_raw_json = self.create_function(JsFunction::native(
            "isRawJSON".to_string(),
            1,
            |interp, _this, args: &[JsValue]| {
                let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if let Some(value_id) = val.as_object_id()
                    && let Some(obj) = interp.get_object_cell(value_id)
                {
                    return Completion::Normal(JsValue::boolean(obj.borrow().is_raw_json));
                }
                Completion::Normal(JsValue::boolean(false))
            },
        ));
        self.get_object_cell_expect(json_obj_id)
            .borrow_mut()
            .insert_builtin("isRawJSON".to_string(), json_is_raw_json);
    }
}
