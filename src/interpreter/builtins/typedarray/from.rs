//! `%TypedArray%.from` (§23.2.2.1).

use super::*;

/// The collected values, the source object and the target typed array stay
/// rooted across `mapfn` calls and the `valueOf` hooks of ToNumber/ToBigInt.
pub(super) fn typed_array_from(
    interp: &mut Interpreter,
    this_val: &JsValue,
    args: &[JsValue],
) -> Completion {
    interp.with_gc_root_scope(|interp| {
        // Step 1: C = this value
        let source = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
        let map_fn_arg = args.get(1).cloned();
        let this_arg = args.get(2).cloned().unwrap_or(JsValue::UNDEFINED);

        // Step 2: IsConstructor(C)
        let is_ctor = this_val.as_object_id().is_some_and(|o| {
            interp
                .get_object_cell(o)
                .is_some_and(|obj| obj.borrow().callable.is_some())
        });
        if !is_ctor {
            return Completion::Throw(
                interp.create_type_error("TypedArray.from requires a constructor"),
            );
        }

        // Step 3: If mapfn is provided and not undefined, check callable
        let mapping = if let Some(ref mf) = map_fn_arg
            && !mf.is_undefined()
        {
            let is_callable = mf.as_object_id().is_some_and(|o| {
                interp
                    .get_object_cell(o)
                    .is_some_and(|obj| obj.borrow().callable.is_some())
            });
            if !is_callable {
                return Completion::Throw(interp.create_type_error("mapfn is not a function"));
            }
            true
        } else {
            false
        };

        // Step 4: GetMethod(source, @@iterator); GetV keeps a primitive as receiver.
        let using_iterator = if source.is_nullish() {
            return Completion::Throw(
                interp.create_type_error("Cannot convert undefined or null to object"),
            );
        } else {
            match interp
                .access_property_on_value(&source, &JsPropertyKey::well_known_symbol("iterator"))
            {
                Completion::Normal(v) if !v.is_nullish() => Some(v),
                Completion::Throw(e) => return Completion::Throw(e),
                _ => None,
            }
        };

        if let Some(iter_fn) = using_iterator {
            // Step 5: Iterable path — collect all values, then create TA, then set
            let values = match interp.iterate_with_function(&source, &iter_fn) {
                Ok(v) => v,
                Err(c) => return c,
            };
            for v in &values {
                interp.gc_root_value(v);
            }
            let len = values.len();
            let target_obj = match interp.typed_array_create(this_val, len) {
                Completion::Normal(v) => v,
                other => return other,
            };
            interp.gc_root_value(&target_obj);
            if let Err(c) = check_target_ta_writable(interp, &target_obj) {
                return c;
            }
            let ta_kind = if let Some(o) = target_obj.as_object_id() {
                interp
                    .get_object_cell(o)
                    .and_then(|obj| obj.borrow().typed_array_info().map(|ta| ta.kind))
            } else {
                None
            };
            let ta_kind = match ta_kind {
                Some(k) => k,
                None => {
                    return Completion::Throw(
                        interp.create_type_error("TypedArray.from: target is not a TypedArray"),
                    );
                }
            };
            for (k, val) in values.iter().enumerate() {
                let mapped_val = if mapping {
                    let mf = map_fn_arg.as_ref().unwrap();
                    match interp.call_function(
                        mf,
                        &this_arg,
                        &[val.clone(), JsValue::number(k as f64)],
                    ) {
                        Completion::Normal(v) => v,
                        other => return other,
                    }
                } else {
                    val.clone()
                };
                let coerced = match interp.typed_array_coerce_value(ta_kind, &mapped_val) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let key = k.to_string();
                if let Some(o) = target_obj.as_object_id()
                    && let Some(obj) = interp.get_object_cell(o)
                {
                    obj.borrow_mut().set_property_value(&key, coerced);
                }
            }
            Completion::Normal(target_obj)
        } else {
            // Step 6: Array-like path — ToObject, get length, create TA, then get each element
            let array_like = match interp.to_object(&source) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Completion::Throw(e),
                _ => {
                    return Completion::Throw(interp.create_type_error("Cannot convert to object"));
                }
            };
            interp.gc_root_value(&array_like);
            // Step 7: len = LengthOfArrayLike (ToLength calls ToNumber which invokes valueOf)
            let len = if let Some(o) = array_like.as_object_id() {
                let len_val = match interp.get_object_property(o, "length", &array_like) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Completion::Throw(e),
                    _ => JsValue::UNDEFINED,
                };
                let n = match interp.to_number_value(&len_val) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                use crate::interpreter::helpers::to_integer_or_infinity;
                let int_len = to_integer_or_infinity(n);
                if int_len < 0.0 {
                    0
                } else {
                    int_len.min(9007199254740991.0) as usize
                }
            } else {
                0
            };
            // Step 8: Create target TA
            let target_obj = match interp.typed_array_create(this_val, len) {
                Completion::Normal(v) => v,
                other => return other,
            };
            interp.gc_root_value(&target_obj);
            if let Err(c) = check_target_ta_writable(interp, &target_obj) {
                return c;
            }
            let ta_kind = if let Some(o) = target_obj.as_object_id() {
                interp
                    .get_object_cell(o)
                    .and_then(|obj| obj.borrow().typed_array_info().map(|ta| ta.kind))
            } else {
                None
            };
            let ta_kind = match ta_kind {
                Some(k) => k,
                None => {
                    return Completion::Throw(
                        interp.create_type_error("TypedArray.from: target is not a TypedArray"),
                    );
                }
            };
            // Step 9: For each k, get element from source
            for k in 0..len {
                let val = if let Some(o) = array_like.as_object_id() {
                    match interp.get_object_property(o, &k.to_string(), &array_like) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => JsValue::UNDEFINED,
                    }
                } else {
                    JsValue::UNDEFINED
                };
                let mapped_val = if mapping {
                    let mf = map_fn_arg.as_ref().unwrap();
                    match interp.call_function(
                        mf,
                        &this_arg,
                        &[val.clone(), JsValue::number(k as f64)],
                    ) {
                        Completion::Normal(v) => v,
                        other => return other,
                    }
                } else {
                    val
                };
                let coerced = match interp.typed_array_coerce_value(ta_kind, &mapped_val) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let key = k.to_string();
                if let Some(o) = target_obj.as_object_id()
                    && let Some(obj) = interp.get_object_cell(o)
                {
                    obj.borrow_mut().set_property_value(&key, coerced);
                }
            }
            Completion::Normal(target_obj)
        }
    })
}
