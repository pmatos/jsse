use super::super::*;

impl Interpreter {
    pub(crate) fn setup_array_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Array".to_string();

        // Array.prototype.push
        let push_fn = self.create_function(JsFunction::native(
            "push".to_string(),
            1,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let mut obj_mut = obj.borrow_mut();
                    if let Some(ref mut elems) = obj_mut.array_elements {
                        for arg in args {
                            elems.push(arg.clone());
                        }
                        let len = elems.len() as f64;
                        obj_mut.insert_value("length".to_string(), JsValue::Number(len));
                        return Completion::Normal(JsValue::Number(len));
                    }
                }
                Completion::Normal(JsValue::Undefined)
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
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let mut obj_mut = obj.borrow_mut();
                    if let Some(ref mut elems) = obj_mut.array_elements {
                        let val = elems.pop().unwrap_or(JsValue::Undefined);
                        let len = elems.len() as f64;
                        obj_mut.insert_value("length".to_string(), JsValue::Number(len));
                        return Completion::Normal(val);
                    }
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto.borrow_mut().insert_builtin("pop".to_string(), pop_fn);

        // Array.prototype.shift
        let shift_fn = self.create_function(JsFunction::native(
            "shift".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let mut obj_mut = obj.borrow_mut();
                    if let Some(ref mut elems) = obj_mut.array_elements {
                        if elems.is_empty() {
                            return Completion::Normal(JsValue::Undefined);
                        }
                        let val = elems.remove(0);
                        let len = elems.len() as f64;
                        obj_mut.insert_value("length".to_string(), JsValue::Number(len));
                        return Completion::Normal(val);
                    }
                }
                Completion::Normal(JsValue::Undefined)
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
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let mut obj_mut = obj.borrow_mut();
                    if let Some(ref mut elems) = obj_mut.array_elements {
                        for (i, arg) in args.iter().rev().enumerate() {
                            let _ = i;
                            elems.insert(0, arg.clone());
                        }
                        let len = elems.len() as f64;
                        obj_mut.insert_value("length".to_string(), JsValue::Number(len));
                        return Completion::Normal(JsValue::Number(len));
                    }
                }
                Completion::Normal(JsValue::Undefined)
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
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if let Some(ref elems) = obj_ref.array_elements {
                        let search = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let from = args.get(1).map(|v| to_number(v) as i64).unwrap_or(0);
                        let start = if from < 0 {
                            (elems.len() as i64 + from).max(0) as usize
                        } else {
                            from as usize
                        };
                        for i in start..elems.len() {
                            if strict_equality(&elems[i], &search) {
                                return Completion::Normal(JsValue::Number(i as f64));
                            }
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
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if let Some(ref elems) = obj_ref.array_elements {
                        let search = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let len = elems.len() as i64;
                        let from = args.get(1).map(|v| to_number(v) as i64).unwrap_or(len - 1);
                        let start = if from < 0 {
                            (len + from) as usize
                        } else {
                            from.min(len - 1) as usize
                        };
                        for i in (0..=start).rev() {
                            if strict_equality(&elems[i], &search) {
                                return Completion::Normal(JsValue::Number(i as f64));
                            }
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
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if let Some(ref elems) = obj_ref.array_elements {
                        let search = args.first().cloned().unwrap_or(JsValue::Undefined);
                        for elem in elems {
                            if strict_equality(elem, &search) || (elem.is_nan() && search.is_nan())
                            {
                                return Completion::Normal(JsValue::Boolean(true));
                            }
                        }
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
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if let Some(ref elems) = obj_ref.array_elements {
                        let sep = if let Some(s) = args.first() {
                            if matches!(s, JsValue::Undefined) {
                                ",".to_string()
                            } else {
                                to_js_string(s)
                            }
                        } else {
                            ",".to_string()
                        };
                        let parts: Vec<String> = elems
                            .iter()
                            .map(|v| {
                                if v.is_undefined() || v.is_null() {
                                    String::new()
                                } else {
                                    to_js_string(v)
                                }
                            })
                            .collect();
                        return Completion::Normal(JsValue::String(JsString::from_str(
                            &parts.join(&sep),
                        )));
                    }
                }
                Completion::Normal(JsValue::String(JsString::from_str("")))
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
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if let Some(ref elems) = obj_ref.array_elements {
                        let parts: Vec<String> = elems
                            .iter()
                            .map(|v| {
                                if v.is_undefined() || v.is_null() {
                                    String::new()
                                } else {
                                    to_js_string(v)
                                }
                            })
                            .collect();
                        return Completion::Normal(JsValue::String(JsString::from_str(
                            &parts.join(","),
                        )));
                    }
                }
                Completion::Normal(JsValue::String(JsString::from_str("[object Object]")))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toString".to_string(), tostring_fn);

        // Array.prototype.concat
        let concat_fn = self.create_function(JsFunction::native(
            "concat".to_string(),
            1,
            |interp, this_val, args| {
                let mut result = Vec::new();
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                    && let Some(ref elems) = obj.borrow().array_elements
                {
                    result.extend(elems.clone());
                }
                for arg in args {
                    if let JsValue::Object(o) = arg
                        && let Some(obj) = interp.get_object(o.id)
                        && let Some(ref elems) = obj.borrow().array_elements
                    {
                        result.extend(elems.clone());
                        continue;
                    }
                    result.push(arg.clone());
                }
                let arr = interp.create_array(result);
                Completion::Normal(arr)
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
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if let Some(ref elems) = obj_ref.array_elements {
                        let len = elems.len() as i64;
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
                        let sliced: Vec<JsValue> = if start < end {
                            elems[start..end].to_vec()
                        } else {
                            Vec::new()
                        };
                        drop(obj_ref);
                        let arr = interp.create_array(sliced);
                        return Completion::Normal(arr);
                    }
                }
                let arr = interp.create_array(Vec::new());
                Completion::Normal(arr)
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
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let mut obj_mut = obj.borrow_mut();
                    if let Some(ref mut elems) = obj_mut.array_elements {
                        elems.reverse();
                    }
                }
                Completion::Normal(this_val.clone())
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
                if matches!(this_val, JsValue::Null | JsValue::Undefined) {
                    let e = interp.create_type_error("Cannot convert undefined or null to object");
                    return Completion::Throw(e);
                }
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                    let mut reversed = elems;
                    reversed.reverse();
                    let arr = interp.create_array(reversed);
                    return Completion::Normal(arr);
                }
                Completion::Normal(JsValue::Undefined)
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
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                    for (i, elem) in elems.iter().enumerate() {
                        let call_args =
                            vec![elem.clone(), JsValue::Number(i as f64), this_val.clone()];
                        if let result @ Completion::Throw(_) =
                            interp.call_function(&callback, &JsValue::Undefined, &call_args)
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
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                let mut result = Vec::new();
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                    for (i, elem) in elems.iter().enumerate() {
                        let call_args =
                            vec![elem.clone(), JsValue::Number(i as f64), this_val.clone()];
                        match interp.call_function(&callback, &JsValue::Undefined, &call_args) {
                            Completion::Normal(v) => result.push(v),
                            other => return other,
                        }
                    }
                }
                let arr = interp.create_array(result);
                Completion::Normal(arr)
            },
        ));
        proto.borrow_mut().insert_builtin("map".to_string(), map_fn);

        // Array.prototype.filter
        let filter_fn = self.create_function(JsFunction::native(
            "filter".to_string(),
            1,
            |interp, this_val, args| {
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                let mut result = Vec::new();
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                    for (i, elem) in elems.iter().enumerate() {
                        let call_args =
                            vec![elem.clone(), JsValue::Number(i as f64), this_val.clone()];
                        match interp.call_function(&callback, &JsValue::Undefined, &call_args) {
                            Completion::Normal(v) => {
                                if to_boolean(&v) {
                                    result.push(elem.clone());
                                }
                            }
                            other => return other,
                        }
                    }
                }
                let arr = interp.create_array(result);
                Completion::Normal(arr)
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
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                    let (mut acc, start) = if args.len() > 1 {
                        (args[1].clone(), 0)
                    } else if !elems.is_empty() {
                        (elems[0].clone(), 1)
                    } else {
                        let err =
                            interp.create_type_error("Reduce of empty array with no initial value");
                        return Completion::Throw(err);
                    };
                    for i in start..elems.len() {
                        let call_args = vec![
                            acc,
                            elems[i].clone(),
                            JsValue::Number(i as f64),
                            this_val.clone(),
                        ];
                        match interp.call_function(&callback, &JsValue::Undefined, &call_args) {
                            Completion::Normal(v) => acc = v,
                            other => return other,
                        }
                    }
                    return Completion::Normal(acc);
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("reduce".to_string(), reduce_fn);

        // Array.prototype.some
        let some_fn = self.create_function(JsFunction::native(
            "some".to_string(),
            1,
            |interp, this_val, args| {
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                    for (i, elem) in elems.iter().enumerate() {
                        let call_args =
                            vec![elem.clone(), JsValue::Number(i as f64), this_val.clone()];
                        match interp.call_function(&callback, &JsValue::Undefined, &call_args) {
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
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                    for (i, elem) in elems.iter().enumerate() {
                        let call_args =
                            vec![elem.clone(), JsValue::Number(i as f64), this_val.clone()];
                        match interp.call_function(&callback, &JsValue::Undefined, &call_args) {
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
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                    for (i, elem) in elems.iter().enumerate() {
                        let call_args =
                            vec![elem.clone(), JsValue::Number(i as f64), this_val.clone()];
                        match interp.call_function(&callback, &JsValue::Undefined, &call_args) {
                            Completion::Normal(v) => {
                                if to_boolean(&v) {
                                    return Completion::Normal(elem.clone());
                                }
                            }
                            other => return other,
                        }
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
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                    for (i, elem) in elems.iter().enumerate() {
                        let call_args =
                            vec![elem.clone(), JsValue::Number(i as f64), this_val.clone()];
                        match interp.call_function(&callback, &JsValue::Undefined, &call_args) {
                            Completion::Normal(v) => {
                                if to_boolean(&v) {
                                    return Completion::Normal(JsValue::Number(i as f64));
                                }
                            }
                            other => return other,
                        }
                    }
                }
                Completion::Normal(JsValue::Number(-1.0))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("findIndex".to_string(), findindex_fn);

        // Array.prototype.splice
        let splice_fn = self.create_function(JsFunction::native(
            "splice".to_string(),
            2,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let mut obj_mut = obj.borrow_mut();
                    if let Some(ref mut elems) = obj_mut.array_elements {
                        let len = elems.len() as i64;
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
                        let delete_count = args
                            .get(1)
                            .map(|v| (to_number(v) as i64).max(0).min(len - start as i64) as usize)
                            .unwrap_or((len - start as i64) as usize);
                        let removed: Vec<JsValue> =
                            elems.drain(start..start + delete_count).collect();
                        let items: Vec<JsValue> = args.iter().skip(2).cloned().collect();
                        for (i, item) in items.into_iter().enumerate() {
                            elems.insert(start + i, item);
                        }
                        let new_len = elems.len() as f64;
                        obj_mut.insert_value("length".to_string(), JsValue::Number(new_len));
                        drop(obj_mut);
                        let arr = interp.create_array(removed);
                        return Completion::Normal(arr);
                    }
                }
                let arr = interp.create_array(Vec::new());
                Completion::Normal(arr)
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
                if matches!(this_val, JsValue::Null | JsValue::Undefined) {
                    let e = interp.create_type_error("Cannot convert undefined or null to object");
                    return Completion::Throw(e);
                }
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                    let len = elems.len() as i64;
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
                    let delete_count = args
                        .get(1)
                        .map(|v| (to_number(v) as i64).max(0).min(len - start as i64) as usize)
                        .unwrap_or((len - start as i64) as usize);
                    let items: Vec<JsValue> = args.iter().skip(2).cloned().collect();
                    let mut result = Vec::with_capacity(
                        start + items.len() + elems.len() - start - delete_count,
                    );
                    result.extend_from_slice(&elems[..start]);
                    result.extend(items);
                    if start + delete_count < elems.len() {
                        result.extend_from_slice(&elems[start + delete_count..]);
                    }
                    let arr = interp.create_array(result);
                    return Completion::Normal(arr);
                }
                Completion::Normal(JsValue::Undefined)
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
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let mut obj_mut = obj.borrow_mut();
                    if let Some(ref mut elems) = obj_mut.array_elements {
                        let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let len = elems.len() as i64;
                        let start = args
                            .get(1)
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
                            .get(2)
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
                        for i in start..end {
                            elems[i] = value.clone();
                        }
                    }
                }
                Completion::Normal(this_val.clone())
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("fill".to_string(), fill_fn);

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
                let mut values = Vec::new();
                match &source {
                    JsValue::String(s) => {
                        for ch in s.to_rust_string().chars() {
                            let v = JsValue::String(JsString::from_str(&ch.to_string()));
                            if let Some(ref mf) = map_fn {
                                match interp.call_function(
                                    mf,
                                    &JsValue::Undefined,
                                    &[v, JsValue::Number(values.len() as f64)],
                                ) {
                                    Completion::Normal(mapped) => values.push(mapped),
                                    other => return other,
                                }
                            } else {
                                values.push(v);
                            }
                        }
                    }
                    JsValue::Object(o) => {
                        if let Some(obj) = interp.get_object(o.id) {
                            let len = if let Some(JsValue::Number(n)) =
                                obj.borrow().get_property_value("length")
                            {
                                n as usize
                            } else {
                                0
                            };
                            for i in 0..len {
                                let v = obj.borrow().get_property(&i.to_string());
                                if let Some(ref mf) = map_fn {
                                    match interp.call_function(
                                        mf,
                                        &JsValue::Undefined,
                                        &[v, JsValue::Number(i as f64)],
                                    ) {
                                        Completion::Normal(mapped) => values.push(mapped),
                                        other => return other,
                                    }
                                } else {
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

        // Array.prototype.reduceRight
        let reduce_right_fn = self.create_function(JsFunction::native(
            "reduceRight".to_string(),
            1,
            |interp, this_val, args| {
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                    let len = elems.len();
                    let (mut acc, start) = if args.len() > 1 {
                        (args[1].clone(), len)
                    } else if len > 0 {
                        (elems[len - 1].clone(), len - 1)
                    } else {
                        return Completion::Throw(
                            interp.create_type_error("Reduce of empty array with no initial value"),
                        );
                    };
                    for i in (0..start).rev() {
                        let result = interp.call_function(
                            &callback,
                            &JsValue::Undefined,
                            &[
                                acc,
                                elems[i].clone(),
                                JsValue::Number(i as f64),
                                this_val.clone(),
                            ],
                        );
                        match result {
                            Completion::Normal(v) => acc = v,
                            other => return other,
                        }
                    }
                    return Completion::Normal(acc);
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("reduceRight".to_string(), reduce_right_fn);

        // Array.prototype.at
        let at_fn = self.create_function(JsFunction::native(
            "at".to_string(),
            1,
            |interp, this_val, args| {
                let idx = args.first().map(|v| to_number(v) as i64).unwrap_or(0);
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                    && let Some(elems) = &obj.borrow().array_elements
                {
                    let len = elems.len() as i64;
                    let actual = if idx < 0 { len + idx } else { idx };
                    if actual >= 0 && actual < len {
                        return Completion::Normal(elems[actual as usize].clone());
                    }
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto.borrow_mut().insert_builtin("at".to_string(), at_fn);

        // Array.prototype.with
        let with_fn = self.create_function(JsFunction::native(
            "with".to_string(),
            2,
            |interp, this_val, args| {
                if matches!(this_val, JsValue::Null | JsValue::Undefined) {
                    let e = interp.create_type_error("Cannot convert undefined or null to object");
                    return Completion::Throw(e);
                }
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                    let len = elems.len() as i64;
                    let idx = args.first().map(|v| to_number(v) as i64).unwrap_or(0);
                    let actual = if idx < 0 { len + idx } else { idx };
                    if actual < 0 || actual >= len {
                        let e = interp.create_range_error("Invalid index");
                        return Completion::Throw(e);
                    }
                    let value = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let mut new_elems = elems;
                    new_elems[actual as usize] = value;
                    let arr = interp.create_array(new_elems);
                    return Completion::Normal(arr);
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("with".to_string(), with_fn);

        // Array.prototype.sort
        let sort_fn = self.create_function(JsFunction::native(
            "sort".to_string(),
            1,
            |interp, this_val, args| {
                let compare_fn = args.first().cloned();
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                    && let Some(ref mut elems) = obj.borrow_mut().array_elements
                {
                    let mut pairs: Vec<(usize, JsValue)> = elems.drain(..).enumerate().collect();
                    pairs.sort_by(|a, b| {
                        let x = &a.1;
                        let y = &b.1;
                        if matches!(x, JsValue::Undefined) && matches!(y, JsValue::Undefined) {
                            return std::cmp::Ordering::Equal;
                        }
                        if matches!(x, JsValue::Undefined) {
                            return std::cmp::Ordering::Greater;
                        }
                        if matches!(y, JsValue::Undefined) {
                            return std::cmp::Ordering::Less;
                        }
                        if let Some(JsValue::Object(fo)) = &compare_fn
                            && let Some(fobj) = interp.get_object(fo.id)
                            && fobj.borrow().callable.is_some()
                        {
                            let result = interp.call_function(
                                compare_fn.as_ref().unwrap(),
                                &JsValue::Undefined,
                                &[x.clone(), y.clone()],
                            );
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
                    *elems = pairs.into_iter().map(|(_, v)| v).collect();
                }
                Completion::Normal(this_val.clone())
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
                if matches!(this_val, JsValue::Null | JsValue::Undefined) {
                    let e = interp.create_type_error("Cannot convert undefined or null to object");
                    return Completion::Throw(e);
                }
                let compare_fn = args.first().cloned();
                if let Some(ref cf) = compare_fn {
                    if !matches!(cf, JsValue::Undefined) {
                        let is_callable = if let JsValue::Object(fo) = cf
                            && let Some(fobj) = interp.get_object(fo.id)
                        {
                            fobj.borrow().callable.is_some()
                        } else {
                            false
                        };
                        if !is_callable {
                            let e = interp.create_type_error("compareFn is not a function");
                            return Completion::Throw(e);
                        }
                    }
                }
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                    let mut pairs: Vec<(usize, JsValue)> =
                        elems.into_iter().enumerate().collect();
                    pairs.sort_by(|a, b| {
                        let x = &a.1;
                        let y = &b.1;
                        if matches!(x, JsValue::Undefined) && matches!(y, JsValue::Undefined) {
                            return std::cmp::Ordering::Equal;
                        }
                        if matches!(x, JsValue::Undefined) {
                            return std::cmp::Ordering::Greater;
                        }
                        if matches!(y, JsValue::Undefined) {
                            return std::cmp::Ordering::Less;
                        }
                        if let Some(JsValue::Object(fo)) = &compare_fn
                            && let Some(fobj) = interp.get_object(fo.id)
                            && fobj.borrow().callable.is_some()
                        {
                            let result = interp.call_function(
                                compare_fn.as_ref().unwrap(),
                                &JsValue::Undefined,
                                &[x.clone(), y.clone()],
                            );
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
                    let sorted: Vec<JsValue> = pairs.into_iter().map(|(_, v)| v).collect();
                    let arr = interp.create_array(sorted);
                    return Completion::Normal(arr);
                }
                Completion::Normal(JsValue::Undefined)
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
                let depth = args.first().map(|v| to_number(v) as i64).unwrap_or(1);
                fn flatten(
                    interp: &Interpreter,
                    val: &JsValue,
                    depth: i64,
                    result: &mut Vec<JsValue>,
                ) {
                    if let JsValue::Object(o) = val
                        && let Some(obj) = interp.get_object(o.id)
                        && let Some(elems) = &obj.borrow().array_elements
                    {
                        for elem in elems {
                            if depth > 0
                                && let JsValue::Object(eo) = elem
                                && let Some(eobj) = interp.get_object(eo.id)
                                && eobj.borrow().array_elements.is_some()
                            {
                                flatten(interp, elem, depth - 1, result);
                                continue;
                            }
                            result.push(elem.clone());
                        }
                        return;
                    }
                    result.push(val.clone());
                }
                let mut result = Vec::new();
                flatten(interp, this_val, depth, &mut result);
                Completion::Normal(interp.create_array(result))
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
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let mut result = Vec::new();
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                    for (i, elem) in elems.iter().enumerate() {
                        let mapped = interp.call_function(
                            &callback,
                            &this_arg,
                            &[elem.clone(), JsValue::Number(i as f64), this_val.clone()],
                        );
                        if let Completion::Normal(v) = mapped {
                            if let JsValue::Object(mo) = &v
                                && let Some(mobj) = interp.get_object(mo.id)
                                && let Some(melems) = &mobj.borrow().array_elements
                            {
                                result.extend(melems.iter().cloned());
                                continue;
                            }
                            result.push(v);
                        } else {
                            return mapped;
                        }
                    }
                }
                Completion::Normal(interp.create_array(result))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("flatMap".to_string(), flatmap_fn);

        // Array.prototype.findLast / findLastIndex
        let findlast_fn = self.create_function(JsFunction::native(
            "findLast".to_string(),
            1,
            |interp, this_val, args| {
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                    for i in (0..elems.len()).rev() {
                        let result = interp.call_function(
                            &callback,
                            &this_arg,
                            &[
                                elems[i].clone(),
                                JsValue::Number(i as f64),
                                this_val.clone(),
                            ],
                        );
                        if let Completion::Normal(v) = result {
                            if to_boolean(&v) {
                                return Completion::Normal(elems[i].clone());
                            }
                        } else {
                            return result;
                        }
                    }
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("findLast".to_string(), findlast_fn);

        let findlastidx_fn = self.create_function(JsFunction::native(
            "findLastIndex".to_string(),
            1,
            |interp, this_val, args| {
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                    for i in (0..elems.len()).rev() {
                        let result = interp.call_function(
                            &callback,
                            &this_arg,
                            &[
                                elems[i].clone(),
                                JsValue::Number(i as f64),
                                this_val.clone(),
                            ],
                        );
                        if let Completion::Normal(v) = result {
                            if to_boolean(&v) {
                                return Completion::Normal(JsValue::Number(i as f64));
                            }
                        } else {
                            return result;
                        }
                    }
                }
                Completion::Normal(JsValue::Number(-1.0))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("findLastIndex".to_string(), findlastidx_fn);

        // Array.prototype.copyWithin
        let copywithin_fn = self.create_function(JsFunction::native(
            "copyWithin".to_string(),
            2,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let mut obj_mut = obj.borrow_mut();
                    if let Some(ref mut elems) = obj_mut.array_elements {
                        let len = elems.len() as i64;
                        let target = {
                            let t = args.first().map(|v| to_number(v) as i64).unwrap_or(0);
                            if t < 0 {
                                (len + t).max(0) as usize
                            } else {
                                t.min(len) as usize
                            }
                        };
                        let start = {
                            let s = args.get(1).map(|v| to_number(v) as i64).unwrap_or(0);
                            if s < 0 {
                                (len + s).max(0) as usize
                            } else {
                                s.min(len) as usize
                            }
                        };
                        let end = {
                            let e = args
                                .get(2)
                                .map(|v| {
                                    if matches!(v, JsValue::Undefined) {
                                        len
                                    } else {
                                        to_number(v) as i64
                                    }
                                })
                                .unwrap_or(len);
                            if e < 0 {
                                (len + e).max(0) as usize
                            } else {
                                e.min(len) as usize
                            }
                        };
                        let count = (end - start).min(len as usize - target);
                        let src: Vec<JsValue> = elems[start..start + count].to_vec();
                        for (i, v) in src.into_iter().enumerate() {
                            elems[target + i] = v;
                        }
                    }
                }
                Completion::Normal(this_val.clone())
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("copyWithin".to_string(), copywithin_fn);

        // Array.prototype.entries — returns lazy ArrayIterator (KeyValue)
        let entries_fn = self.create_function(JsFunction::native(
            "entries".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val {
                    return Completion::Normal(
                        interp.create_array_iterator(o.id, IteratorKind::KeyValue),
                    );
                }
                let err = interp.create_type_error("entries called on non-object");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("entries".to_string(), entries_fn);

        // Array.prototype.keys — returns lazy ArrayIterator (Key)
        let keys_fn = self.create_function(JsFunction::native(
            "keys".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val {
                    return Completion::Normal(
                        interp.create_array_iterator(o.id, IteratorKind::Key),
                    );
                }
                let err = interp.create_type_error("keys called on non-object");
                Completion::Throw(err)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("keys".to_string(), keys_fn);

        // Array.prototype.values — returns lazy ArrayIterator (Value)
        let values_fn = self.create_function(JsFunction::native(
            "values".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val {
                    return Completion::Normal(
                        interp.create_array_iterator(o.id, IteratorKind::Value),
                    );
                }
                let err = interp.create_type_error("values called on non-object");
                Completion::Throw(err)
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
                if let JsValue::Object(o) = this_val {
                    return Completion::Normal(
                        interp.create_array_iterator(o.id, IteratorKind::Value),
                    );
                }
                let err = interp.create_type_error("Symbol.iterator called on non-object");
                Completion::Throw(err)
            },
        ));
        if let Some(key) = self.get_symbol_iterator_key() {
            proto
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(iter_fn, true, false, true));
        }

        // Set Array statics on the Array constructor
        if let Some(array_val) = self.global_env.borrow().get("Array")
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
        obj_data.insert_value("length".to_string(), JsValue::Number(values.len() as f64));
        obj_data.array_elements = Some(values);
        let obj = Rc::new(RefCell::new(obj_data));
        let id = self.allocate_object_slot(obj);
        JsValue::Object(crate::types::JsObject { id })
    }

}
