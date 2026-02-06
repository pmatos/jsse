use super::super::*;

fn to_object_val(interp: &mut Interpreter, this: &JsValue) -> Result<JsValue, Completion> {
    match interp.to_object(this) {
        Completion::Normal(v) => Ok(v),
        other => Err(other),
    }
}

fn length_of_array_like(interp: &mut Interpreter, o: &JsValue) -> Result<usize, Completion> {
    let len_val = obj_get_simple(interp, o, "length");
    let n = to_number(&len_val);
    let len = to_integer_or_infinity(n);
    if len < 0.0 {
        return Ok(0);
    }
    Ok(len.min(9007199254740991.0) as usize)
}

fn obj_get_simple(interp: &Interpreter, o: &JsValue, key: &str) -> JsValue {
    if let Some(obj) = get_obj(interp, o) {
        obj.borrow().get_property(key)
    } else {
        JsValue::Undefined
    }
}

fn get_obj(interp: &Interpreter, o: &JsValue) -> Option<Rc<RefCell<JsObjectData>>> {
    if let JsValue::Object(obj_ref) = o {
        interp.get_object(obj_ref.id)
    } else {
        None
    }
}

fn obj_get(interp: &mut Interpreter, o: &JsValue, key: &str) -> JsValue {
    if let JsValue::Object(obj_ref) = o {
        match interp.get_object_property(obj_ref.id, key, o) {
            Completion::Normal(v) => v,
            _ => JsValue::Undefined,
        }
    } else {
        JsValue::Undefined
    }
}

fn obj_set(interp: &mut Interpreter, o: &JsValue, key: &str, value: JsValue) {
    if let Some(obj) = get_obj(interp, o) {
        let mut borrow = obj.borrow_mut();
        if let Some(ref mut elems) = borrow.array_elements
            && let Ok(idx) = key.parse::<usize>()
        {
            if idx < elems.len() {
                elems[idx] = value.clone();
            } else {
                while elems.len() < idx {
                    elems.push(JsValue::Undefined);
                }
                elems.push(value.clone());
            }
        }
        borrow.set_property_value(key, value);
    }
}

fn create_data_property(interp: &mut Interpreter, o: &JsValue, key: &str, value: JsValue) {
    if let Some(obj) = get_obj(interp, o) {
        let mut borrow = obj.borrow_mut();
        if let Some(ref mut elems) = borrow.array_elements
            && let Ok(idx) = key.parse::<usize>()
        {
            if idx < elems.len() {
                elems[idx] = value.clone();
            } else {
                while elems.len() < idx {
                    elems.push(JsValue::Undefined);
                }
                elems.push(value.clone());
            }
        }
        borrow.define_own_property(key.to_string(), PropertyDescriptor::data_default(value));
    }
}

fn obj_has(interp: &mut Interpreter, o: &JsValue, key: &str) -> bool {
    if let Some(obj) = get_obj(interp, o) {
        let borrow = obj.borrow();
        if borrow.has_property(key) {
            return true;
        }
        if let Some(ref elems) = borrow.array_elements
            && let Ok(idx) = key.parse::<usize>()
        {
            return idx < elems.len();
        }
        false
    } else {
        false
    }
}

fn obj_delete(interp: &mut Interpreter, o: &JsValue, key: &str) {
    if let Some(obj) = get_obj(interp, o) {
        let mut borrow = obj.borrow_mut();
        borrow.properties.remove(key);
        borrow.property_order.retain(|k| k != key);
        if let Some(ref mut elems) = borrow.array_elements
            && let Ok(idx) = key.parse::<usize>()
            && idx < elems.len()
        {
            elems[idx] = JsValue::Undefined;
        }
    }
}

fn set_length(interp: &mut Interpreter, o: &JsValue, len: usize) {
    if let Some(obj) = get_obj(interp, o) {
        let mut borrow = obj.borrow_mut();
        if let Some(ref mut elems) = borrow.array_elements {
            elems.resize(len, JsValue::Undefined);
        }
        borrow.set_property_value("length", JsValue::Number(len as f64));
    }
}

fn require_callable(interp: &mut Interpreter, val: &JsValue, msg: &str) -> Result<(), Completion> {
    if !interp.is_callable(val) {
        Err(Completion::Throw(interp.create_type_error(msg)))
    } else {
        Ok(())
    }
}

// ArraySpeciesCreate (§23.1.3.7.1)
fn array_species_create(interp: &mut Interpreter, original_array: &JsValue, length: usize) -> Result<JsValue, Completion> {
    let is_array = if let JsValue::Object(o) = original_array
        && let Some(obj) = interp.get_object(o.id)
    {
        obj.borrow().array_elements.is_some()
    } else {
        false
    };
    if !is_array {
        return Ok(interp.create_array(Vec::new()));
    }
    // Get constructor
    let c = if let JsValue::Object(o) = original_array {
        match interp.get_object_property(o.id, "constructor", original_array) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(Completion::Throw(e)),
            other => return Err(other),
        }
    } else {
        JsValue::Undefined
    };
    // If C is Object, get C[@@species]
    let c = if let JsValue::Object(co) = &c {
        let sym_key = interp.get_symbol_key("species");
        let species = if let Some(key) = &sym_key {
            match interp.get_object_property(co.id, key, &c) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(Completion::Throw(e)),
                other => return Err(other),
            }
        } else {
            JsValue::Undefined
        };
        // If species is null, treat as undefined
        if matches!(species, JsValue::Null) {
            JsValue::Undefined
        } else {
            species
        }
    } else {
        c
    };
    // If C is undefined, create a default array
    if matches!(c, JsValue::Undefined) {
        let mut arr = interp.create_array(Vec::new());
        set_length(interp, &arr, length);
        return Ok(arr);
    }
    // If C is not a constructor, throw TypeError
    if !interp.is_constructor(&c) {
        return Err(Completion::Throw(
            interp.create_type_error("Species constructor is not a constructor"),
        ));
    }
    // Construct(C, [length])
    match interp.construct(&c, &[JsValue::Number(length as f64)]) {
        Completion::Normal(v) => Ok(v),
        Completion::Throw(e) => Err(Completion::Throw(e)),
        other => Err(other),
    }
}

impl Interpreter {
    pub(crate) fn setup_array_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Array".to_string();

        // Array.prototype.push
        let push_fn = self.create_function(JsFunction::native(
            "push".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let mut len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                for arg in args {
                    obj_set(interp, &o, &len.to_string(), arg.clone());
                    len += 1;
                }
                set_length(interp, &o, len);
                Completion::Normal(JsValue::Number(len as f64))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("push".to_string(), push_fn);

        // Array.prototype.pop
        let pop_fn = self.create_function(JsFunction::native(
            "pop".to_string(),
            0,
            |interp, this_val, _args: &[JsValue]| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if len == 0 {
                    set_length(interp, &o, 0);
                    return Completion::Normal(JsValue::Undefined);
                }
                let idx = (len - 1).to_string();
                let val = obj_get(interp, &o, &idx);
                obj_delete(interp, &o, &idx);
                set_length(interp, &o, len - 1);
                Completion::Normal(val)
            },
        ));
        proto.borrow_mut().insert_builtin("pop".to_string(), pop_fn);

        // Array.prototype.shift
        let shift_fn = self.create_function(JsFunction::native(
            "shift".to_string(),
            0,
            |interp, this_val, _args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if len == 0 {
                    set_length(interp, &o, 0);
                    return Completion::Normal(JsValue::Undefined);
                }
                let first = obj_get(interp, &o, "0");
                for k in 1..len {
                    let from = k.to_string();
                    let to = (k - 1).to_string();
                    if obj_has(interp, &o, &from) {
                        let val = obj_get(interp, &o, &from);
                        obj_set(interp, &o, &to, val);
                    } else {
                        obj_delete(interp, &o, &to);
                    }
                }
                obj_delete(interp, &o, &(len - 1).to_string());
                set_length(interp, &o, len - 1);
                Completion::Normal(first)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("shift".to_string(), shift_fn);

        // Array.prototype.unshift
        let unshift_fn = self.create_function(JsFunction::native(
            "unshift".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let arg_count = args.len();
                if arg_count > 0 {
                    // Shift existing elements
                    for k in (0..len).rev() {
                        let from = k.to_string();
                        let to = (k + arg_count).to_string();
                        if obj_has(interp, &o, &from) {
                            let val = obj_get(interp, &o, &from);
                            obj_set(interp, &o, &to, val);
                        } else {
                            obj_delete(interp, &o, &to);
                        }
                    }
                    for (j, arg) in args.iter().enumerate() {
                        obj_set(interp, &o, &j.to_string(), arg.clone());
                    }
                }
                let new_len = len + arg_count;
                set_length(interp, &o, new_len);
                Completion::Normal(JsValue::Number(new_len as f64))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("unshift".to_string(), unshift_fn);

        // Array.prototype.indexOf
        let indexof_fn = self.create_function(JsFunction::native(
            "indexOf".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if len == 0 {
                    return Completion::Normal(JsValue::Number(-1.0));
                }
                let search = args.first().cloned().unwrap_or(JsValue::Undefined);
                let n = if args.len() >= 2 {
                    to_integer_or_infinity(to_number(&args[1]))
                } else {
                    0.0
                };
                if n >= len as f64 {
                    return Completion::Normal(JsValue::Number(-1.0));
                }
                let k = if n >= 0.0 {
                    n as usize
                } else {
                    let calc = len as f64 + n;
                    if calc < 0.0 { 0 } else { calc as usize }
                };
                for i in k..len {
                    let pk = i.to_string();
                    if obj_has(interp, &o, &pk) {
                        let elem = obj_get(interp, &o, &pk);
                        if strict_equality(&elem, &search) {
                            return Completion::Normal(JsValue::Number(i as f64));
                        }
                    }
                }
                Completion::Normal(JsValue::Number(-1.0))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("indexOf".to_string(), indexof_fn);

        // Array.prototype.lastIndexOf
        let lastindexof_fn = self.create_function(JsFunction::native(
            "lastIndexOf".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if len == 0 {
                    return Completion::Normal(JsValue::Number(-1.0));
                }
                let search = args.first().cloned().unwrap_or(JsValue::Undefined);
                let n = if args.len() >= 2 {
                    to_integer_or_infinity(to_number(&args[1]))
                } else {
                    len as f64 - 1.0
                };
                let k = if n >= 0.0 {
                    (n as usize).min(len - 1)
                } else {
                    let calc = len as f64 + n;
                    if calc < 0.0 {
                        return Completion::Normal(JsValue::Number(-1.0));
                    }
                    calc as usize
                };
                for i in (0..=k).rev() {
                    let pk = i.to_string();
                    if obj_has(interp, &o, &pk) {
                        let elem = obj_get(interp, &o, &pk);
                        if strict_equality(&elem, &search) {
                            return Completion::Normal(JsValue::Number(i as f64));
                        }
                    }
                }
                Completion::Normal(JsValue::Number(-1.0))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("lastIndexOf".to_string(), lastindexof_fn);

        // Array.prototype.includes
        let includes_fn = self.create_function(JsFunction::native(
            "includes".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if len == 0 {
                    return Completion::Normal(JsValue::Boolean(false));
                }
                let search = args.first().cloned().unwrap_or(JsValue::Undefined);
                let n = if args.len() >= 2 {
                    to_integer_or_infinity(to_number(&args[1]))
                } else {
                    0.0
                };
                let k = if n >= 0.0 {
                    n as usize
                } else {
                    let calc = len as f64 + n;
                    if calc < 0.0 { 0 } else { calc as usize }
                };
                for i in k..len {
                    let elem = obj_get(interp, &o, &i.to_string());
                    if same_value_zero(&elem, &search) {
                        return Completion::Normal(JsValue::Boolean(true));
                    }
                }
                Completion::Normal(JsValue::Boolean(false))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("includes".to_string(), includes_fn);

        // Array.prototype.join
        let join_fn = self.create_function(JsFunction::native(
            "join".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let sep = if let Some(s) = args.first() {
                    if matches!(s, JsValue::Undefined) {
                        ",".to_string()
                    } else {
                        to_js_string(s)
                    }
                } else {
                    ",".to_string()
                };
                let mut parts = Vec::with_capacity(len);
                for i in 0..len {
                    let elem = obj_get(interp, &o, &i.to_string());
                    if elem.is_undefined() || elem.is_null() {
                        parts.push(String::new());
                    } else {
                        parts.push(to_js_string(&elem));
                    }
                }
                Completion::Normal(JsValue::String(JsString::from_str(&parts.join(&sep))))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("join".to_string(), join_fn);

        // Array.prototype.toString
        let tostring_fn = self.create_function(JsFunction::native(
            "toString".to_string(),
            0,
            |interp, this_val, _args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                // Look for a "join" method
                let join_fn = obj_get(interp, &o, "join");
                if interp.is_callable(&join_fn) {
                    return interp.call_function(&join_fn, &o, &[]);
                }
                // Fall back to Object.prototype.toString
                Completion::Normal(JsValue::String(JsString::from_str("[object Array]")))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toString".to_string(), tostring_fn);

        // Array.prototype.toLocaleString
        let to_locale_string_fn = self.create_function(JsFunction::native(
            "toLocaleString".to_string(),
            0,
            |interp, this_val, _args| {
                // Step 1: ToObject(this)
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                // Step 2: LengthOfArrayLike
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                // Step 3: separator = ","
                let separator = ",";
                // Step 4-6: Build result string
                let mut parts: Vec<String> = Vec::with_capacity(len);
                for k in 0..len {
                    // Step 6b: Get element
                    let next_element = obj_get(interp, &o, &k.to_string());
                    // Step 6c: Skip undefined/null, call toLocaleString on others
                    if matches!(next_element, JsValue::Undefined | JsValue::Null) {
                        parts.push(String::new());
                    } else {
                        // Convert to object to get toLocaleString method
                        let element_obj = match interp.to_object(&next_element) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let to_locale_str_method = obj_get(interp, &element_obj, "toLocaleString");
                        if interp.is_callable(&to_locale_str_method) {
                            // Call with NO arguments per spec, using original value as this
                            match interp.call_function(&to_locale_str_method, &next_element, &[]) {
                                Completion::Normal(v) => {
                                    parts.push(to_js_string(&v));
                                }
                                other => return other,
                            }
                        } else {
                            // toLocaleString not callable - throw TypeError per spec
                            let err = interp.create_type_error("toLocaleString is not a function");
                            return Completion::Throw(err);
                        }
                    }
                }
                // Step 7: Return joined string
                Completion::Normal(JsValue::String(JsString::from_str(&parts.join(separator))))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toLocaleString".to_string(), to_locale_string_fn);

        // Array.prototype.concat
        let concat_fn = self.create_function(JsFunction::native(
            "concat".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let a = match array_species_create(interp, &o, 0) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let mut n: usize = 0;
                let items: Vec<JsValue> = std::iter::once(o).chain(args.iter().cloned()).collect();
                for item in &items {
                    // IsConcatSpreadable (§23.1.3.1.1)
                    let spreadable = if let JsValue::Object(obj_ref) = item {
                        let sym_key = interp.get_symbol_key("isConcatSpreadable");
                        let spreadable_val = if let Some(key) = &sym_key {
                            match interp.get_object_property(obj_ref.id, key, item) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            }
                        } else {
                            JsValue::Undefined
                        };
                        if !matches!(spreadable_val, JsValue::Undefined) {
                            to_boolean(&spreadable_val)
                        } else {
                            if let Some(obj) = interp.get_object(obj_ref.id) {
                                obj.borrow().array_elements.is_some()
                            } else {
                                false
                            }
                        }
                    } else {
                        false
                    };
                    if spreadable {
                        let len = match length_of_array_like(interp, item) {
                            Ok(v) => v,
                            Err(c) => return c,
                        };
                        for k in 0..len {
                            let pk = k.to_string();
                            if obj_has(interp, item, &pk) {
                                let val = if let JsValue::Object(obj_ref) = item {
                                    match interp.get_object_property(obj_ref.id, &pk, item) {
                                        Completion::Normal(v) => v,
                                        Completion::Throw(e) => return Completion::Throw(e),
                                        other => return other,
                                    }
                                } else {
                                    obj_get(interp, item, &pk)
                                };
                                create_data_property(interp, &a, &n.to_string(), val);
                            }
                            n += 1;
                        }
                    } else {
                        create_data_property(interp, &a, &n.to_string(), item.clone());
                        n += 1;
                    }
                }
                set_length(interp, &a, n);
                Completion::Normal(a)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("concat".to_string(), concat_fn);

        // Array.prototype.slice
        let slice_fn = self.create_function(JsFunction::native(
            "slice".to_string(),
            2,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                } as i64;
                let relative_start = args
                    .first()
                    .map(|v| to_integer_or_infinity(to_number(v)))
                    .unwrap_or(0.0);
                let k = if relative_start < 0.0 {
                    (len as f64 + relative_start).max(0.0) as usize
                } else {
                    (relative_start as i64).min(len) as usize
                };
                let relative_end = if let Some(v) = args.get(1) {
                    if matches!(v, JsValue::Undefined) {
                        len as f64
                    } else {
                        to_integer_or_infinity(to_number(v))
                    }
                } else {
                    len as f64
                };
                let fin = if relative_end < 0.0 {
                    (len as f64 + relative_end).max(0.0) as usize
                } else {
                    (relative_end as i64).min(len) as usize
                };
                let count = if fin > k { fin - k } else { 0 };
                let a = match array_species_create(interp, &o, count) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let mut n: usize = 0;
                for i in k..fin {
                    let pk = i.to_string();
                    if obj_has(interp, &o, &pk) {
                        let val = obj_get(interp, &o, &pk);
                        create_data_property(interp, &a, &n.to_string(), val);
                    }
                    n += 1;
                }
                set_length(interp, &a, n);
                Completion::Normal(a)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("slice".to_string(), slice_fn);

        // Array.prototype.reverse
        let reverse_fn = self.create_function(JsFunction::native(
            "reverse".to_string(),
            0,
            |interp, this_val, _args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let middle = len / 2;
                for lower in 0..middle {
                    let upper = len - lower - 1;
                    let lower_s = lower.to_string();
                    let upper_s = upper.to_string();
                    let lower_exists = obj_has(interp, &o, &lower_s);
                    let upper_exists = obj_has(interp, &o, &upper_s);
                    let lower_val = if lower_exists {
                        obj_get(interp, &o, &lower_s)
                    } else {
                        JsValue::Undefined
                    };
                    let upper_val = if upper_exists {
                        obj_get(interp, &o, &upper_s)
                    } else {
                        JsValue::Undefined
                    };
                    if lower_exists && upper_exists {
                        obj_set(interp, &o, &lower_s, upper_val);
                        obj_set(interp, &o, &upper_s, lower_val);
                    } else if upper_exists {
                        obj_set(interp, &o, &lower_s, upper_val);
                        obj_delete(interp, &o, &upper_s);
                    } else if lower_exists {
                        obj_delete(interp, &o, &lower_s);
                        obj_set(interp, &o, &upper_s, lower_val);
                    }
                }
                Completion::Normal(o)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("reverse".to_string(), reverse_fn);

        // Array.prototype.toReversed
        let to_reversed_fn = self.create_function(JsFunction::native(
            "toReversed".to_string(),
            0,
            |interp, this_val, _args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let mut result = Vec::with_capacity(len);
                for i in (0..len).rev() {
                    result.push(obj_get(interp, &o, &i.to_string()));
                }
                let arr = interp.create_array(result);
                Completion::Normal(arr)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toReversed".to_string(), to_reversed_fn);

        // Array.prototype.forEach
        let foreach_fn = self.create_function(JsFunction::native(
            "forEach".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "forEach callback is not a function")
                {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for k in 0..len {
                    let pk = k.to_string();
                    if obj_has(interp, &o, &pk) {
                        let kvalue = obj_get(interp, &o, &pk);
                        let call_args = vec![kvalue, JsValue::Number(k as f64), o.clone()];
                        if let result @ Completion::Throw(_) =
                            interp.call_function(&callback, &this_arg, &call_args)
                        {
                            return result;
                        }
                    }
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("forEach".to_string(), foreach_fn);

        // Array.prototype.map
        let map_fn = self.create_function(JsFunction::native(
            "map".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "map callback is not a function")
                {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let a = match array_species_create(interp, &o, len) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                for k in 0..len {
                    let pk = k.to_string();
                    if obj_has(interp, &o, &pk) {
                        let kvalue = obj_get(interp, &o, &pk);
                        match interp.call_function(
                            &callback,
                            &this_arg,
                            &[kvalue, JsValue::Number(k as f64), o.clone()],
                        ) {
                            Completion::Normal(v) => {
                                create_data_property(interp, &a, &pk, v);
                            }
                            other => return other,
                        }
                    }
                }
                set_length(interp, &a, len);
                Completion::Normal(a)
            },
        ));
        proto.borrow_mut().insert_builtin("map".to_string(), map_fn);

        // Array.prototype.filter
        let filter_fn = self.create_function(JsFunction::native(
            "filter".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "filter callback is not a function")
                {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let a = match array_species_create(interp, &o, 0) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let mut to: usize = 0;
                for k in 0..len {
                    let pk = k.to_string();
                    if obj_has(interp, &o, &pk) {
                        let kvalue = obj_get(interp, &o, &pk);
                        match interp.call_function(
                            &callback,
                            &this_arg,
                            &[kvalue.clone(), JsValue::Number(k as f64), o.clone()],
                        ) {
                            Completion::Normal(v) => {
                                if to_boolean(&v) {
                                    create_data_property(interp, &a, &to.to_string(), kvalue);
                                    to += 1;
                                }
                            }
                            other => return other,
                        }
                    }
                }
                set_length(interp, &a, to);
                Completion::Normal(a)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("filter".to_string(), filter_fn);

        // Array.prototype.reduce
        let reduce_fn = self.create_function(JsFunction::native(
            "reduce".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "reduce callback is not a function")
                {
                    return c;
                }
                if len == 0 && args.len() < 2 {
                    return Completion::Throw(
                        interp.create_type_error("Reduce of empty array with no initial value"),
                    );
                }
                let mut k = 0usize;
                let mut acc =
                    if args.len() >= 2 {
                        args[1].clone()
                    } else {
                        // Find first present element
                        loop {
                            if k >= len {
                                return Completion::Throw(interp.create_type_error(
                                    "Reduce of empty array with no initial value",
                                ));
                            }
                            let pk = k.to_string();
                            if obj_has(interp, &o, &pk) {
                                let val = obj_get(interp, &o, &pk);
                                k += 1;
                                break val;
                            }
                            k += 1;
                        }
                    };
                while k < len {
                    let pk = k.to_string();
                    if obj_has(interp, &o, &pk) {
                        let kvalue = obj_get(interp, &o, &pk);
                        match interp.call_function(
                            &callback,
                            &JsValue::Undefined,
                            &[acc, kvalue, JsValue::Number(k as f64), o.clone()],
                        ) {
                            Completion::Normal(v) => acc = v,
                            other => return other,
                        }
                    }
                    k += 1;
                }
                Completion::Normal(acc)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("reduce".to_string(), reduce_fn);

        // Array.prototype.reduceRight
        let reduce_right_fn = self.create_function(JsFunction::native(
            "reduceRight".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "reduceRight callback is not a function")
                {
                    return c;
                }
                if len == 0 && args.len() < 2 {
                    return Completion::Throw(
                        interp.create_type_error("Reduce of empty array with no initial value"),
                    );
                }
                let mut k = len as i64 - 1;
                let mut acc =
                    if args.len() >= 2 {
                        args[1].clone()
                    } else {
                        loop {
                            if k < 0 {
                                return Completion::Throw(interp.create_type_error(
                                    "Reduce of empty array with no initial value",
                                ));
                            }
                            let pk = (k as usize).to_string();
                            if obj_has(interp, &o, &pk) {
                                let val = obj_get(interp, &o, &pk);
                                k -= 1;
                                break val;
                            }
                            k -= 1;
                        }
                    };
                while k >= 0 {
                    let pk = (k as usize).to_string();
                    if obj_has(interp, &o, &pk) {
                        let kvalue = obj_get(interp, &o, &pk);
                        match interp.call_function(
                            &callback,
                            &JsValue::Undefined,
                            &[acc, kvalue, JsValue::Number(k as f64), o.clone()],
                        ) {
                            Completion::Normal(v) => acc = v,
                            other => return other,
                        }
                    }
                    k -= 1;
                }
                Completion::Normal(acc)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("reduceRight".to_string(), reduce_right_fn);

        // Array.prototype.some
        let some_fn = self.create_function(JsFunction::native(
            "some".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "some callback is not a function")
                {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for k in 0..len {
                    let pk = k.to_string();
                    if obj_has(interp, &o, &pk) {
                        let kvalue = obj_get(interp, &o, &pk);
                        match interp.call_function(
                            &callback,
                            &this_arg,
                            &[kvalue, JsValue::Number(k as f64), o.clone()],
                        ) {
                            Completion::Normal(v) => {
                                if to_boolean(&v) {
                                    return Completion::Normal(JsValue::Boolean(true));
                                }
                            }
                            other => return other,
                        }
                    }
                }
                Completion::Normal(JsValue::Boolean(false))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("some".to_string(), some_fn);

        // Array.prototype.every
        let every_fn = self.create_function(JsFunction::native(
            "every".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "every callback is not a function")
                {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for k in 0..len {
                    let pk = k.to_string();
                    if obj_has(interp, &o, &pk) {
                        let kvalue = obj_get(interp, &o, &pk);
                        match interp.call_function(
                            &callback,
                            &this_arg,
                            &[kvalue, JsValue::Number(k as f64), o.clone()],
                        ) {
                            Completion::Normal(v) => {
                                if !to_boolean(&v) {
                                    return Completion::Normal(JsValue::Boolean(false));
                                }
                            }
                            other => return other,
                        }
                    }
                }
                Completion::Normal(JsValue::Boolean(true))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("every".to_string(), every_fn);

        // Array.prototype.find
        let find_fn = self.create_function(JsFunction::native(
            "find".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "find predicate is not a function")
                {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for k in 0..len {
                    let kvalue = obj_get(interp, &o, &k.to_string());
                    match interp.call_function(
                        &callback,
                        &this_arg,
                        &[kvalue.clone(), JsValue::Number(k as f64), o.clone()],
                    ) {
                        Completion::Normal(v) => {
                            if to_boolean(&v) {
                                return Completion::Normal(kvalue);
                            }
                        }
                        other => return other,
                    }
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("find".to_string(), find_fn);

        // Array.prototype.findIndex
        let findindex_fn = self.create_function(JsFunction::native(
            "findIndex".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "findIndex predicate is not a function")
                {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for k in 0..len {
                    let kvalue = obj_get(interp, &o, &k.to_string());
                    match interp.call_function(
                        &callback,
                        &this_arg,
                        &[kvalue, JsValue::Number(k as f64), o.clone()],
                    ) {
                        Completion::Normal(v) => {
                            if to_boolean(&v) {
                                return Completion::Normal(JsValue::Number(k as f64));
                            }
                        }
                        other => return other,
                    }
                }
                Completion::Normal(JsValue::Number(-1.0))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("findIndex".to_string(), findindex_fn);

        // Array.prototype.findLast
        let findlast_fn = self.create_function(JsFunction::native(
            "findLast".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "findLast predicate is not a function")
                {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for k in (0..len).rev() {
                    let kvalue = obj_get(interp, &o, &k.to_string());
                    match interp.call_function(
                        &callback,
                        &this_arg,
                        &[kvalue.clone(), JsValue::Number(k as f64), o.clone()],
                    ) {
                        Completion::Normal(v) => {
                            if to_boolean(&v) {
                                return Completion::Normal(kvalue);
                            }
                        }
                        other => return other,
                    }
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("findLast".to_string(), findlast_fn);

        // Array.prototype.findLastIndex
        let findlastidx_fn = self.create_function(JsFunction::native(
            "findLastIndex".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) = require_callable(
                    interp,
                    &callback,
                    "findLastIndex predicate is not a function",
                ) {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for k in (0..len).rev() {
                    let kvalue = obj_get(interp, &o, &k.to_string());
                    match interp.call_function(
                        &callback,
                        &this_arg,
                        &[kvalue, JsValue::Number(k as f64), o.clone()],
                    ) {
                        Completion::Normal(v) => {
                            if to_boolean(&v) {
                                return Completion::Normal(JsValue::Number(k as f64));
                            }
                        }
                        other => return other,
                    }
                }
                Completion::Normal(JsValue::Number(-1.0))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("findLastIndex".to_string(), findlastidx_fn);

        // Array.prototype.splice
        let splice_fn = self.create_function(JsFunction::native(
            "splice".to_string(),
            2,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v as i64,
                    Err(c) => return c,
                };
                let relative_start = args
                    .first()
                    .map(|v| to_integer_or_infinity(to_number(v)))
                    .unwrap_or(0.0);
                let actual_start = if relative_start < 0.0 {
                    (len as f64 + relative_start).max(0.0) as usize
                } else {
                    (relative_start as i64).min(len) as usize
                };
                let insert_count = if args.len() > 2 { args.len() - 2 } else { 0 };
                let actual_delete_count = if args.is_empty() {
                    0usize
                } else if args.len() == 1 {
                    (len - actual_start as i64) as usize
                } else {
                    let dc = to_integer_or_infinity(to_number(&args[1]));
                    dc.max(0.0).min((len - actual_start as i64) as f64) as usize
                };
                let a = match array_species_create(interp, &o, actual_delete_count) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                for i in 0..actual_delete_count {
                    let from = (actual_start + i).to_string();
                    if obj_has(interp, &o, &from) {
                        let val = obj_get(interp, &o, &from);
                        create_data_property(interp, &a, &i.to_string(), val);
                    }
                }
                set_length(interp, &a, actual_delete_count);
                let items: Vec<JsValue> = args.iter().skip(2).cloned().collect();
                if insert_count < actual_delete_count {
                    for k in actual_start..((len as usize) - actual_delete_count) {
                        let from = (k + actual_delete_count).to_string();
                        let to = (k + insert_count).to_string();
                        if obj_has(interp, &o, &from) {
                            let val = obj_get(interp, &o, &from);
                            obj_set(interp, &o, &to, val);
                        } else {
                            obj_delete(interp, &o, &to);
                        }
                    }
                    for k in
                        ((len as usize - actual_delete_count + insert_count)..(len as usize)).rev()
                    {
                        obj_delete(interp, &o, &k.to_string());
                    }
                } else if insert_count > actual_delete_count {
                    for k in (actual_start..((len as usize) - actual_delete_count)).rev() {
                        let from = (k + actual_delete_count).to_string();
                        let to = (k + insert_count).to_string();
                        if obj_has(interp, &o, &from) {
                            let val = obj_get(interp, &o, &from);
                            obj_set(interp, &o, &to, val);
                        } else {
                            obj_delete(interp, &o, &to);
                        }
                    }
                }
                for (j, item) in items.into_iter().enumerate() {
                    obj_set(interp, &o, &(actual_start + j).to_string(), item);
                }
                let new_len = (len as usize) - actual_delete_count + insert_count;
                set_length(interp, &o, new_len);
                Completion::Normal(a)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("splice".to_string(), splice_fn);

        // Array.prototype.toSpliced
        let to_spliced_fn = self.create_function(JsFunction::native(
            "toSpliced".to_string(),
            2,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v as i64,
                    Err(c) => return c,
                };
                let relative_start = args
                    .first()
                    .map(|v| to_integer_or_infinity(to_number(v)))
                    .unwrap_or(0.0);
                let actual_start = if relative_start < 0.0 {
                    (len as f64 + relative_start).max(0.0) as usize
                } else {
                    (relative_start as i64).min(len) as usize
                };
                let actual_delete_count = if args.is_empty() {
                    0usize
                } else if args.len() == 1 {
                    (len - actual_start as i64) as usize
                } else {
                    let dc = to_integer_or_infinity(to_number(&args[1]));
                    dc.max(0.0).min((len - actual_start as i64) as f64) as usize
                };
                let items: Vec<JsValue> = args.iter().skip(2).cloned().collect();
                let new_len = (len as usize) - actual_delete_count + items.len();
                let mut result = Vec::with_capacity(new_len);
                for i in 0..actual_start {
                    result.push(obj_get(interp, &o, &i.to_string()));
                }
                result.extend(items);
                for i in (actual_start + actual_delete_count)..(len as usize) {
                    result.push(obj_get(interp, &o, &i.to_string()));
                }
                Completion::Normal(interp.create_array(result))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toSpliced".to_string(), to_spliced_fn);

        // Array.prototype.fill
        let fill_fn = self.create_function(JsFunction::native(
            "fill".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v as i64,
                    Err(c) => return c,
                };
                let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                let relative_start = if let Some(v) = args.get(1) {
                    to_integer_or_infinity(to_number(v))
                } else {
                    0.0
                };
                let k = if relative_start < 0.0 {
                    (len as f64 + relative_start).max(0.0) as usize
                } else {
                    (relative_start as i64).min(len) as usize
                };
                let relative_end = if let Some(v) = args.get(2) {
                    if matches!(v, JsValue::Undefined) {
                        len as f64
                    } else {
                        to_integer_or_infinity(to_number(v))
                    }
                } else {
                    len as f64
                };
                let fin = if relative_end < 0.0 {
                    (len as f64 + relative_end).max(0.0) as usize
                } else {
                    (relative_end as i64).min(len) as usize
                };
                for i in k..fin {
                    obj_set(interp, &o, &i.to_string(), value.clone());
                }
                Completion::Normal(o)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("fill".to_string(), fill_fn);

        // Array.prototype.sort
        let sort_fn = self.create_function(JsFunction::native(
            "sort".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let compare_fn = args.first().cloned();
                if let Some(ref cf) = compare_fn
                    && !matches!(cf, JsValue::Undefined)
                    && !interp.is_callable(cf)
                {
                    return Completion::Throw(
                        interp.create_type_error("compareFn is not a function"),
                    );
                }
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let mut items: Vec<JsValue> = Vec::with_capacity(len);
                for i in 0..len {
                    let pk = i.to_string();
                    if obj_has(interp, &o, &pk) {
                        items.push(obj_get(interp, &o, &pk));
                    }
                }
                let cmp_fn = compare_fn.clone();
                items.sort_by(|x, y| {
                    if matches!(x, JsValue::Undefined) && matches!(y, JsValue::Undefined) {
                        return std::cmp::Ordering::Equal;
                    }
                    if matches!(x, JsValue::Undefined) {
                        return std::cmp::Ordering::Greater;
                    }
                    if matches!(y, JsValue::Undefined) {
                        return std::cmp::Ordering::Less;
                    }
                    if let Some(ref cf) = cmp_fn
                        && !matches!(cf, JsValue::Undefined)
                        && interp.is_callable(cf)
                    {
                        let result =
                            interp.call_function(cf, &JsValue::Undefined, &[x.clone(), y.clone()]);
                        if let Completion::Normal(v) = result {
                            let n = to_number(&v);
                            if n < 0.0 {
                                return std::cmp::Ordering::Less;
                            }
                            if n > 0.0 {
                                return std::cmp::Ordering::Greater;
                            }
                            return std::cmp::Ordering::Equal;
                        }
                    }
                    let xs = to_js_string(x);
                    let ys = to_js_string(y);
                    xs.cmp(&ys)
                });
                // Write back
                for (i, v) in items.iter().enumerate() {
                    obj_set(interp, &o, &i.to_string(), v.clone());
                }
                for i in items.len()..len {
                    obj_delete(interp, &o, &i.to_string());
                }
                Completion::Normal(o)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("sort".to_string(), sort_fn);

        // Array.prototype.toSorted
        let to_sorted_fn = self.create_function(JsFunction::native(
            "toSorted".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let compare_fn = args.first().cloned();
                if let Some(ref cf) = compare_fn
                    && !matches!(cf, JsValue::Undefined)
                    && !interp.is_callable(cf)
                {
                    return Completion::Throw(
                        interp.create_type_error("compareFn is not a function"),
                    );
                }
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let mut items: Vec<JsValue> = Vec::with_capacity(len);
                for i in 0..len {
                    items.push(obj_get(interp, &o, &i.to_string()));
                }
                let cmp_fn = compare_fn.clone();
                items.sort_by(|x, y| {
                    if matches!(x, JsValue::Undefined) && matches!(y, JsValue::Undefined) {
                        return std::cmp::Ordering::Equal;
                    }
                    if matches!(x, JsValue::Undefined) {
                        return std::cmp::Ordering::Greater;
                    }
                    if matches!(y, JsValue::Undefined) {
                        return std::cmp::Ordering::Less;
                    }
                    if let Some(ref cf) = cmp_fn
                        && !matches!(cf, JsValue::Undefined)
                        && interp.is_callable(cf)
                    {
                        let result =
                            interp.call_function(cf, &JsValue::Undefined, &[x.clone(), y.clone()]);
                        if let Completion::Normal(v) = result {
                            let n = to_number(&v);
                            if n < 0.0 {
                                return std::cmp::Ordering::Less;
                            }
                            if n > 0.0 {
                                return std::cmp::Ordering::Greater;
                            }
                            return std::cmp::Ordering::Equal;
                        }
                    }
                    let xs = to_js_string(x);
                    let ys = to_js_string(y);
                    xs.cmp(&ys)
                });
                Completion::Normal(interp.create_array(items))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toSorted".to_string(), to_sorted_fn);

        // Array.prototype.flat
        let flat_fn = self.create_function(JsFunction::native(
            "flat".to_string(),
            0,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let depth_num = if let Some(d) = args.first() {
                    if matches!(d, JsValue::Undefined) {
                        1.0
                    } else {
                        to_integer_or_infinity(to_number(d))
                    }
                } else {
                    1.0
                };
                let depth = if depth_num < 0.0 { 0i64 } else { depth_num as i64 };
                fn flatten_into(
                    interp: &mut Interpreter,
                    target: &mut Vec<JsValue>,
                    source: &JsValue,
                    source_len: usize,
                    depth: i64,
                ) {
                    for k in 0..source_len {
                        let pk = k.to_string();
                        if obj_has(interp, source, &pk) {
                            let elem = obj_get(interp, source, &pk);
                            let should_flatten = depth > 0
                                && matches!(&elem, JsValue::Object(eo) if interp.get_object(eo.id).is_some_and(|o| o.borrow().array_elements.is_some()));
                            if should_flatten {
                                let elem_len = length_of_array_like(interp, &elem).unwrap_or(0);
                                flatten_into(interp, target, &elem, elem_len, depth - 1);
                            } else {
                                target.push(elem);
                            }
                        }
                    }
                }
                let mut result = Vec::new();
                flatten_into(interp, &mut result, &o, len, depth);
                let result_len = result.len();
                let a = match array_species_create(interp, &o, 0) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                for (i, val) in result.into_iter().enumerate() {
                    create_data_property(interp, &a, &i.to_string(), val);
                }
                set_length(interp, &a, result_len);
                Completion::Normal(a)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("flat".to_string(), flat_fn);

        // Array.prototype.flatMap
        let flatmap_fn = self.create_function(JsFunction::native(
            "flatMap".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let Err(c) =
                    require_callable(interp, &callback, "flatMap callback is not a function")
                {
                    return c;
                }
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let mut result = Vec::new();
                for k in 0..len {
                    let pk = k.to_string();
                    if obj_has(interp, &o, &pk) {
                        let kvalue = obj_get(interp, &o, &pk);
                        let mapped = interp.call_function(
                            &callback,
                            &this_arg,
                            &[kvalue, JsValue::Number(k as f64), o.clone()],
                        );
                        match mapped {
                            Completion::Normal(v) => {
                                if let JsValue::Object(mo) = &v
                                    && let Some(mobj) = interp.get_object(mo.id)
                                    && mobj.borrow().array_elements.is_some()
                                {
                                    let mlen = length_of_array_like(interp, &v).unwrap_or(0);
                                    for j in 0..mlen {
                                        let jpk = j.to_string();
                                        if obj_has(interp, &v, &jpk) {
                                            result.push(obj_get(interp, &v, &jpk));
                                        }
                                    }
                                    continue;
                                }
                                result.push(v);
                            }
                            other => return other,
                        }
                    }
                }
                let result_len = result.len();
                let a = match array_species_create(interp, &o, 0) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                for (i, val) in result.into_iter().enumerate() {
                    create_data_property(interp, &a, &i.to_string(), val);
                }
                set_length(interp, &a, result_len);
                Completion::Normal(a)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("flatMap".to_string(), flatmap_fn);

        // Array.prototype.copyWithin
        let copywithin_fn = self.create_function(JsFunction::native(
            "copyWithin".to_string(),
            2,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v as i64,
                    Err(c) => return c,
                };
                let relative_target = args
                    .first()
                    .map(|v| to_integer_or_infinity(to_number(v)))
                    .unwrap_or(0.0);
                let to_val = if relative_target < 0.0 {
                    (len as f64 + relative_target).max(0.0) as i64
                } else {
                    (relative_target as i64).min(len)
                };
                let relative_start = args
                    .get(1)
                    .map(|v| to_integer_or_infinity(to_number(v)))
                    .unwrap_or(0.0);
                let from = if relative_start < 0.0 {
                    (len as f64 + relative_start).max(0.0) as i64
                } else {
                    (relative_start as i64).min(len)
                };
                let relative_end = if let Some(v) = args.get(2) {
                    if matches!(v, JsValue::Undefined) {
                        len as f64
                    } else {
                        to_integer_or_infinity(to_number(v))
                    }
                } else {
                    len as f64
                };
                let fin = if relative_end < 0.0 {
                    (len as f64 + relative_end).max(0.0) as i64
                } else {
                    (relative_end as i64).min(len)
                };
                let count = (fin - from).min(len - to_val);
                if count <= 0 {
                    return Completion::Normal(o);
                }
                let count = count as usize;
                let (mut from_idx, mut to_idx, direction): (i64, i64, i64) =
                    if from < to_val && to_val < from + count as i64 {
                        (from + count as i64 - 1, to_val + count as i64 - 1, -1)
                    } else {
                        (from, to_val, 1)
                    };
                for _ in 0..count {
                    let from_s = (from_idx as usize).to_string();
                    let to_s = (to_idx as usize).to_string();
                    if obj_has(interp, &o, &from_s) {
                        let val = obj_get(interp, &o, &from_s);
                        obj_set(interp, &o, &to_s, val);
                    } else {
                        obj_delete(interp, &o, &to_s);
                    }
                    from_idx += direction;
                    to_idx += direction;
                }
                Completion::Normal(o)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("copyWithin".to_string(), copywithin_fn);

        // Array.prototype.at
        let at_fn = self.create_function(JsFunction::native(
            "at".to_string(),
            1,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v as i64,
                    Err(c) => return c,
                };
                let relative_index = args
                    .first()
                    .map(|v| to_integer_or_infinity(to_number(v)) as i64)
                    .unwrap_or(0);
                let k = if relative_index >= 0 {
                    relative_index
                } else {
                    len + relative_index
                };
                if k < 0 || k >= len {
                    return Completion::Normal(JsValue::Undefined);
                }
                Completion::Normal(obj_get(interp, &o, &(k as usize).to_string()))
            },
        ));
        proto.borrow_mut().insert_builtin("at".to_string(), at_fn);

        // Array.prototype.with
        let with_fn = self.create_function(JsFunction::native(
            "with".to_string(),
            2,
            |interp, this_val, args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = match length_of_array_like(interp, &o) {
                    Ok(v) => v as i64,
                    Err(c) => return c,
                };
                if len as u64 > 0xFFFFFFFF {
                    return Completion::Throw(interp.create_range_error("Invalid array length"));
                }
                let relative_index = args
                    .first()
                    .map(|v| to_integer_or_infinity(to_number(v)) as i64)
                    .unwrap_or(0);
                let actual = if relative_index >= 0 {
                    relative_index
                } else {
                    len + relative_index
                };
                if actual < 0 || actual >= len {
                    return Completion::Throw(interp.create_range_error("Invalid index"));
                }
                let value = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let mut result = Vec::with_capacity(len as usize);
                for k in 0..len as usize {
                    if k == actual as usize {
                        result.push(value.clone());
                    } else {
                        result.push(obj_get(interp, &o, &k.to_string()));
                    }
                }
                Completion::Normal(interp.create_array(result))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("with".to_string(), with_fn);

        // Array.isArray
        let is_array_fn = self.create_function(JsFunction::native(
            "isArray".to_string(),
            1,
            |interp, _this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = &val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    return Completion::Normal(JsValue::Boolean(
                        obj.borrow().array_elements.is_some(),
                    ));
                }
                Completion::Normal(JsValue::Boolean(false))
            },
        ));

        // Array.from
        let array_from = self.create_function(JsFunction::native(
            "from".to_string(),
            1,
            |interp, _this, args: &[JsValue]| {
                let source = args.first().cloned().unwrap_or(JsValue::Undefined);
                let map_fn = args.get(1).cloned();
                let this_arg = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                if let Some(ref mf) = map_fn
                    && !matches!(mf, JsValue::Undefined)
                    && !interp.is_callable(mf)
                {
                    return Completion::Throw(
                        interp.create_type_error("Array.from mapFn is not a function"),
                    );
                }
                let mut values = Vec::new();
                match &source {
                    JsValue::String(s) => {
                        for ch in s.to_rust_string().chars() {
                            let v = JsValue::String(JsString::from_str(&ch.to_string()));
                            if let Some(ref mf) = map_fn
                                && !matches!(mf, JsValue::Undefined)
                            {
                                match interp.call_function(
                                    mf,
                                    &this_arg,
                                    &[v, JsValue::Number(values.len() as f64)],
                                ) {
                                    Completion::Normal(mapped) => values.push(mapped),
                                    other => return other,
                                }
                                continue;
                            }
                            values.push(v);
                        }
                    }
                    JsValue::Object(o) => {
                        if let Some(obj) = interp.get_object(o.id) {
                            // Check for iterable (Symbol.iterator)
                            let has_iterator = {
                                let borrow = obj.borrow();
                                let mut found = false;
                                for k in borrow.properties.keys() {
                                    if k.starts_with("Symbol(Symbol.iterator)") {
                                        found = true;
                                        break;
                                    }
                                }
                                found
                            };
                            if has_iterator {
                                // Use iterator protocol
                                let iter_key = {
                                    let borrow = obj.borrow();
                                    borrow
                                        .properties
                                        .keys()
                                        .find(|k| k.starts_with("Symbol(Symbol.iterator)"))
                                        .cloned()
                                };
                                if let Some(ik) = iter_key {
                                    let iter_fn = obj.borrow().get_property(&ik);
                                    let iterator =
                                        match interp.call_function(&iter_fn, &source, &[]) {
                                            Completion::Normal(v) => v,
                                            other => return other,
                                        };
                                    loop {
                                        let next_fn = obj_get(interp, &iterator, "next");
                                        let result =
                                            match interp.call_function(&next_fn, &iterator, &[]) {
                                                Completion::Normal(v) => v,
                                                other => return other,
                                            };
                                        let (done, value) = extract_iter_result(interp, &result);
                                        if done {
                                            break;
                                        }
                                        if let Some(ref mf) = map_fn
                                            && !matches!(mf, JsValue::Undefined)
                                        {
                                            match interp.call_function(
                                                mf,
                                                &this_arg,
                                                &[value, JsValue::Number(values.len() as f64)],
                                            ) {
                                                Completion::Normal(mapped) => values.push(mapped),
                                                other => return other,
                                            }
                                            continue;
                                        }
                                        values.push(value);
                                    }
                                }
                            } else {
                                // Array-like
                                let len_val = obj.borrow().get_property("length");
                                let len =
                                    to_integer_or_infinity(to_number(&len_val)).max(0.0) as usize;
                                for i in 0..len {
                                    let v = obj.borrow().get_property(&i.to_string());
                                    if let Some(ref mf) = map_fn
                                        && !matches!(mf, JsValue::Undefined)
                                    {
                                        match interp.call_function(
                                            mf,
                                            &this_arg,
                                            &[v, JsValue::Number(i as f64)],
                                        ) {
                                            Completion::Normal(mapped) => values.push(mapped),
                                            other => return other,
                                        }
                                        continue;
                                    }
                                    values.push(v);
                                }
                            }
                        }
                    }
                    _ => {}
                }
                Completion::Normal(interp.create_array(values))
            },
        ));

        // Array.of
        let array_of = self.create_function(JsFunction::native(
            "of".to_string(),
            0,
            |interp, _this, args: &[JsValue]| {
                Completion::Normal(interp.create_array(args.to_vec()))
            },
        ));

        // Array.prototype.entries
        let entries_fn = self.create_function(JsFunction::native(
            "entries".to_string(),
            0,
            |interp, this_val, _args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if let JsValue::Object(obj_ref) = &o {
                    return Completion::Normal(
                        interp.create_array_iterator(obj_ref.id, IteratorKind::KeyValue),
                    );
                }
                Completion::Throw(interp.create_type_error("entries called on non-object"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("entries".to_string(), entries_fn);

        // Array.prototype.keys
        let keys_fn = self.create_function(JsFunction::native(
            "keys".to_string(),
            0,
            |interp, this_val, _args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if let JsValue::Object(obj_ref) = &o {
                    return Completion::Normal(
                        interp.create_array_iterator(obj_ref.id, IteratorKind::Key),
                    );
                }
                Completion::Throw(interp.create_type_error("keys called on non-object"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("keys".to_string(), keys_fn);

        // Array.prototype.values
        let values_fn = self.create_function(JsFunction::native(
            "values".to_string(),
            0,
            |interp, this_val, _args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if let JsValue::Object(obj_ref) = &o {
                    return Completion::Normal(
                        interp.create_array_iterator(obj_ref.id, IteratorKind::Value),
                    );
                }
                Completion::Throw(interp.create_type_error("values called on non-object"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("values".to_string(), values_fn);

        // Array.prototype[@@iterator] = Array.prototype.values
        let iter_fn = self.create_function(JsFunction::native(
            "[Symbol.iterator]".to_string(),
            0,
            |interp, this_val, _args| {
                let o = match to_object_val(interp, this_val) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if let JsValue::Object(obj_ref) = &o {
                    return Completion::Normal(
                        interp.create_array_iterator(obj_ref.id, IteratorKind::Value),
                    );
                }
                Completion::Throw(interp.create_type_error("Symbol.iterator called on non-object"))
            },
        ));
        if let Some(key) = self.get_symbol_iterator_key() {
            proto
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(iter_fn, true, false, true));
        }

        // Set Array statics on the Array constructor
        let array_val = self.global_env.borrow().get("Array");
        if let Some(array_val) = array_val
            && let JsValue::Object(o) = &array_val
            && let Some(obj) = self.get_object(o.id)
        {
            obj.borrow_mut()
                .insert_value("isArray".to_string(), is_array_fn);
            obj.borrow_mut()
                .insert_value("from".to_string(), array_from);
            obj.borrow_mut().insert_value("of".to_string(), array_of);
            let proto_val = JsValue::Object(crate::types::JsObject {
                id: proto.borrow().id.unwrap(),
            });
            obj.borrow_mut()
                .insert_value("prototype".to_string(), proto_val);

            // Array[Symbol.species] getter - returns `this`
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

            // Array.prototype.constructor = Array
            proto
                .borrow_mut()
                .insert_builtin("constructor".to_string(), array_val);
        }

        self.array_prototype = Some(proto);
    }

    pub(crate) fn create_array(&mut self, values: Vec<JsValue>) -> JsValue {
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = self
            .array_prototype
            .clone()
            .or(self.object_prototype.clone());
        obj_data.class_name = "Array".to_string();
        for (i, v) in values.iter().enumerate() {
            obj_data.insert_value(i.to_string(), v.clone());
        }
        obj_data.insert_property(
            "length".to_string(),
            PropertyDescriptor::data(JsValue::Number(values.len() as f64), true, false, false),
        );
        obj_data.array_elements = Some(values);
        let obj = Rc::new(RefCell::new(obj_data));
        let id = self.allocate_object_slot(obj);
        JsValue::Object(crate::types::JsObject { id })
    }
}
