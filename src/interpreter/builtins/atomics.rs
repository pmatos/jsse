use super::super::*;
use crate::types::{JsBigInt, JsObject, JsString, JsValue};

impl Interpreter {
    pub(crate) fn setup_atomics(&mut self) {
        let atomics_obj = self.create_object();
        let atomics_id = atomics_obj.borrow().id.unwrap();

        // Atomics.add
        let add_fn = self.create_function(JsFunction::native(
            "add".to_string(),
            3,
            |interp, _this, args| {
                atomics_rmw(
                    interp,
                    args,
                    |old, val| old.wrapping_add(val),
                    |old, val| old.wrapping_add(val),
                )
            },
        ));
        atomics_obj
            .borrow_mut()
            .insert_builtin("add".to_string(), add_fn);

        // Atomics.sub
        let sub_fn = self.create_function(JsFunction::native(
            "sub".to_string(),
            3,
            |interp, _this, args| {
                atomics_rmw(
                    interp,
                    args,
                    |old, val| old.wrapping_sub(val),
                    |old, val| old.wrapping_sub(val),
                )
            },
        ));
        atomics_obj
            .borrow_mut()
            .insert_builtin("sub".to_string(), sub_fn);

        // Atomics.and
        let and_fn = self.create_function(JsFunction::native(
            "and".to_string(),
            3,
            |interp, _this, args| {
                atomics_rmw(interp, args, |old, val| old & val, |old, val| old & val)
            },
        ));
        atomics_obj
            .borrow_mut()
            .insert_builtin("and".to_string(), and_fn);

        // Atomics.or
        let or_fn = self.create_function(JsFunction::native(
            "or".to_string(),
            3,
            |interp, _this, args| {
                atomics_rmw(interp, args, |old, val| old | val, |old, val| old | val)
            },
        ));
        atomics_obj
            .borrow_mut()
            .insert_builtin("or".to_string(), or_fn);

        // Atomics.xor
        let xor_fn = self.create_function(JsFunction::native(
            "xor".to_string(),
            3,
            |interp, _this, args| {
                atomics_rmw(interp, args, |old, val| old ^ val, |old, val| old ^ val)
            },
        ));
        atomics_obj
            .borrow_mut()
            .insert_builtin("xor".to_string(), xor_fn);

        // Atomics.exchange
        let exchange_fn = self.create_function(JsFunction::native(
            "exchange".to_string(),
            3,
            |interp, _this, args| atomics_rmw(interp, args, |_old, val| val, |_old, val| val),
        ));
        atomics_obj
            .borrow_mut()
            .insert_builtin("exchange".to_string(), exchange_fn);

        // Atomics.load
        let load_fn = self.create_function(JsFunction::native(
            "load".to_string(),
            2,
            |interp, _this, args| {
                let ta_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let index_val = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let (kind, buffer, byte_offset, element_size, is_bigint) =
                    match validate_integer_typed_array(interp, &ta_val, false) {
                        Ok(info) => info,
                        Err(e) => return Completion::Throw(e),
                    };
                let byte_index =
                    match validate_atomic_access(interp, &ta_val, &index_val, element_size) {
                        Ok(i) => i,
                        Err(e) => return Completion::Throw(e),
                    };
                let offset = byte_offset + byte_index;
                let buf = buffer.borrow();
                if is_bigint {
                    let val = read_bigint_from_buffer(&buf, offset, kind);
                    Completion::Normal(val)
                } else {
                    let val = read_number_from_buffer(&buf, offset, kind);
                    Completion::Normal(val)
                }
            },
        ));
        atomics_obj
            .borrow_mut()
            .insert_builtin("load".to_string(), load_fn);

        // Atomics.store
        let store_fn = self.create_function(JsFunction::native(
            "store".to_string(),
            3,
            |interp, _this, args| {
                let ta_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let index_val = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let value = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                let (kind, buffer, byte_offset, element_size, is_bigint) =
                    match validate_integer_typed_array(interp, &ta_val, false) {
                        Ok(info) => info,
                        Err(e) => return Completion::Throw(e),
                    };
                // Convert value BEFORE access validation per spec
                // For Number: use ToIntegerOrInfinity (normalizes -0 to +0)
                // For BigInt: use ToBigInt
                let (converted, return_val) = if is_bigint {
                    let v = match interp.to_bigint_value(&value) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };
                    (v.clone(), v)
                } else {
                    let n = match interp.to_number_value(&value) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    // ToIntegerOrInfinity
                    let int_val = if n.is_nan() || n == 0.0 {
                        0.0
                    } else if n.is_infinite() {
                        n
                    } else {
                        n.trunc()
                    };
                    (JsValue::Number(n), JsValue::Number(int_val))
                };
                let byte_index =
                    match validate_atomic_access(interp, &ta_val, &index_val, element_size) {
                        Ok(i) => i,
                        Err(e) => return Completion::Throw(e),
                    };
                let offset = byte_offset + byte_index;
                let mut buf = buffer.borrow_mut();
                if is_bigint {
                    write_bigint_to_buffer(&mut buf, offset, kind, &converted);
                } else {
                    write_number_to_buffer(&mut buf, offset, kind, &converted);
                }
                Completion::Normal(return_val)
            },
        ));
        atomics_obj
            .borrow_mut()
            .insert_builtin("store".to_string(), store_fn);

        // Atomics.compareExchange
        let cmpxchg_fn = self.create_function(JsFunction::native(
            "compareExchange".to_string(),
            4,
            |interp, _this, args| {
                let ta_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let index_val = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let expected_val = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                let replacement_val = args.get(3).cloned().unwrap_or(JsValue::Undefined);
                let (kind, buffer, byte_offset, element_size, is_bigint) =
                    match validate_integer_typed_array(interp, &ta_val, false) {
                        Ok(info) => info,
                        Err(e) => return Completion::Throw(e),
                    };
                let byte_index =
                    match validate_atomic_access(interp, &ta_val, &index_val, element_size) {
                        Ok(i) => i,
                        Err(e) => return Completion::Throw(e),
                    };
                let (expected, replacement) = if is_bigint {
                    let exp = match interp.to_bigint_value(&expected_val) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };
                    let rep = match interp.to_bigint_value(&replacement_val) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };
                    (exp, rep)
                } else {
                    let exp = match interp.to_number_value(&expected_val) {
                        Ok(n) => JsValue::Number(n),
                        Err(e) => return Completion::Throw(e),
                    };
                    let rep = match interp.to_number_value(&replacement_val) {
                        Ok(n) => JsValue::Number(n),
                        Err(e) => return Completion::Throw(e),
                    };
                    (exp, rep)
                };
                let offset = byte_offset + byte_index;
                let mut buf = buffer.borrow_mut();
                if is_bigint {
                    let old = read_bigint_raw_bytes(&buf, offset, kind);
                    let exp_bytes = bigint_to_raw_bytes(kind, &expected);
                    if old == exp_bytes {
                        write_bigint_to_buffer(&mut buf, offset, kind, &replacement);
                    }
                    let old_val = bigint_from_raw_bytes(kind, &old);
                    Completion::Normal(old_val)
                } else {
                    let old = read_number_raw_bytes(&buf, offset, kind);
                    let exp_bytes = number_to_raw_bytes(kind, &expected);
                    if old == exp_bytes {
                        write_number_to_buffer(&mut buf, offset, kind, &replacement);
                    }
                    let old_val = number_from_raw_bytes(kind, &old);
                    Completion::Normal(old_val)
                }
            },
        ));
        atomics_obj
            .borrow_mut()
            .insert_builtin("compareExchange".to_string(), cmpxchg_fn);

        // Atomics.isLockFree
        let is_lock_free_fn = self.create_function(JsFunction::native(
            "isLockFree".to_string(),
            1,
            |interp, _this, args| {
                let size_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let size = match interp.to_number_value(&size_val) {
                    Ok(n) => n,
                    Err(e) => return Completion::Throw(e),
                };
                let result = matches!(size as u64, 1 | 2 | 4 | 8) && size == (size as u64) as f64;
                Completion::Normal(JsValue::Boolean(result))
            },
        ));
        atomics_obj
            .borrow_mut()
            .insert_builtin("isLockFree".to_string(), is_lock_free_fn);

        // Atomics.wait — always throws TypeError since [[CanBlock]] = false
        let wait_fn = self.create_function(JsFunction::native(
            "wait".to_string(),
            4,
            |interp, _this, args| {
                let ta_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let index_val = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let value = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                // Validate to get correct error ordering
                let (kind, buffer, byte_offset, element_size, is_bigint) =
                    match validate_integer_typed_array(interp, &ta_val, true) {
                        Ok(info) => info,
                        Err(e) => return Completion::Throw(e),
                    };
                let _ = (kind, buffer, byte_offset, is_bigint);
                let _byte_index =
                    match validate_atomic_access(interp, &ta_val, &index_val, element_size) {
                        Ok(i) => i,
                        Err(e) => return Completion::Throw(e),
                    };
                // Convert value
                if is_bigint {
                    if let Err(e) = interp.to_bigint_value(&value) {
                        return Completion::Throw(e);
                    }
                } else if let Err(e) = interp.to_number_value(&value) {
                    return Completion::Throw(e);
                }
                // Convert timeout
                let timeout_val = args.get(3).cloned().unwrap_or(JsValue::Undefined);
                if !matches!(timeout_val, JsValue::Undefined)
                    && let Err(e) = interp.to_number_value(&timeout_val)
                {
                    return Completion::Throw(e);
                }
                // [[CanBlock]] is false for main thread
                Completion::Throw(
                    interp.create_type_error("Atomics.wait cannot block on the main thread"),
                )
            },
        ));
        atomics_obj
            .borrow_mut()
            .insert_builtin("wait".to_string(), wait_fn);

        // Atomics.notify
        let notify_fn = self.create_function(JsFunction::native(
            "notify".to_string(),
            3,
            |interp, _this, args| {
                let ta_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let index_val = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let (kind, _buffer, _byte_offset, element_size, _is_bigint) =
                    match validate_integer_typed_array(interp, &ta_val, true) {
                        Ok(info) => info,
                        Err(e) => return Completion::Throw(e),
                    };
                let _ = kind;
                let _byte_index =
                    match validate_atomic_access(interp, &ta_val, &index_val, element_size) {
                        Ok(i) => i,
                        Err(e) => return Completion::Throw(e),
                    };
                // count argument coercion (per spec)
                let count_val = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                if !matches!(count_val, JsValue::Undefined) {
                    let c = match interp.to_number_value(&count_val) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    if c.is_nan() {
                        // treat as 0
                    } else {
                        let ci = if c.is_infinite() && c > 0.0 {
                            f64::INFINITY
                        } else {
                            c.trunc().max(0.0)
                        };
                        let _ = ci;
                    }
                }
                // No waiting threads in single-threaded engine
                Completion::Normal(JsValue::Number(0.0))
            },
        ));
        atomics_obj
            .borrow_mut()
            .insert_builtin("notify".to_string(), notify_fn);

        // Atomics.waitAsync
        let wait_async_fn = self.create_function(JsFunction::native(
            "waitAsync".to_string(),
            4,
            |interp, _this, args| {
                let ta_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let index_val = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let value = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                let (kind, buffer, byte_offset, element_size, is_bigint) =
                    match validate_integer_typed_array(interp, &ta_val, true) {
                        Ok(info) => info,
                        Err(e) => return Completion::Throw(e),
                    };
                // Check buffer is shared
                let buffer_is_shared = if let JsValue::Object(o) = &ta_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if obj_ref.typed_array_info.is_some() {
                        if let Some(buf_id) = obj_ref.view_buffer_object_id
                            && let Some(buf_obj) = interp.get_object(buf_id)
                        {
                            buf_obj.borrow().arraybuffer_is_shared
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };
                let _ = kind;
                let byte_index =
                    match validate_atomic_access(interp, &ta_val, &index_val, element_size) {
                        Ok(i) => i,
                        Err(e) => return Completion::Throw(e),
                    };
                // Convert value
                let converted = if is_bigint {
                    match interp.to_bigint_value(&value) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    match interp.to_number_value(&value) {
                        Ok(n) => JsValue::Number(n),
                        Err(e) => return Completion::Throw(e),
                    }
                };
                // Convert timeout
                let timeout_val = args.get(3).cloned().unwrap_or(JsValue::Undefined);
                let timeout = if matches!(timeout_val, JsValue::Undefined) {
                    f64::INFINITY
                } else {
                    match interp.to_number_value(&timeout_val) {
                        Ok(n) => {
                            if n.is_nan() {
                                f64::INFINITY
                            } else {
                                n.max(0.0)
                            }
                        }
                        Err(e) => return Completion::Throw(e),
                    }
                };
                if !buffer_is_shared {
                    return Completion::Throw(
                        interp.create_type_error("Atomics.waitAsync requires a shared typed array"),
                    );
                }

                // Read current value and compare
                let offset = byte_offset + byte_index;
                let buf = buffer.borrow();
                let current_matches = if is_bigint {
                    let current = read_bigint_raw_bytes(&buf, offset, kind);
                    let expected = bigint_to_raw_bytes(kind, &converted);
                    current == expected
                } else {
                    let current = read_number_raw_bytes(&buf, offset, kind);
                    let expected = number_to_raw_bytes(kind, &converted);
                    current == expected
                };
                drop(buf);

                // Build result object { async, value }
                let result = interp.create_object();
                if !current_matches {
                    result.borrow_mut().insert_property(
                        "async".to_string(),
                        PropertyDescriptor::data(JsValue::Boolean(false), true, true, true),
                    );
                    result.borrow_mut().insert_property(
                        "value".to_string(),
                        PropertyDescriptor::data(
                            JsValue::String(JsString::from_str("not-equal")),
                            true,
                            true,
                            true,
                        ),
                    );
                } else if timeout <= 0.0 {
                    result.borrow_mut().insert_property(
                        "async".to_string(),
                        PropertyDescriptor::data(JsValue::Boolean(false), true, true, true),
                    );
                    result.borrow_mut().insert_property(
                        "value".to_string(),
                        PropertyDescriptor::data(
                            JsValue::String(JsString::from_str("timed-out")),
                            true,
                            true,
                            true,
                        ),
                    );
                } else {
                    // In single-threaded engine, no one will notify, so return timed-out
                    result.borrow_mut().insert_property(
                        "async".to_string(),
                        PropertyDescriptor::data(JsValue::Boolean(false), true, true, true),
                    );
                    result.borrow_mut().insert_property(
                        "value".to_string(),
                        PropertyDescriptor::data(
                            JsValue::String(JsString::from_str("timed-out")),
                            true,
                            true,
                            true,
                        ),
                    );
                }
                let id = result.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(JsObject { id }))
            },
        ));
        atomics_obj
            .borrow_mut()
            .insert_builtin("waitAsync".to_string(), wait_async_fn);

        // Atomics.pause
        let pause_fn =
            self.create_function(JsFunction::native(
                "pause".to_string(),
                0,
                |interp, _this, args| {
                    if let Some(arg) = args.first()
                        && !matches!(arg, JsValue::Undefined)
                    {
                        // Step 1a: Type(iterationNumber) must be Number
                        let n = if let JsValue::Number(n) = arg {
                            *n
                        } else {
                            return Completion::Throw(interp.create_type_error(
                                "Atomics.pause requires a non-negative integer",
                            ));
                        };
                        // Step 1b: IsIntegralNumber(n) must be true
                        // Step 1c: n must be >= 0
                        if n.is_nan() || n.is_infinite() || n != n.trunc() || n < 0.0 {
                            return Completion::Throw(interp.create_type_error(
                                "Atomics.pause requires a non-negative integer",
                            ));
                        }
                    }
                    Completion::Normal(JsValue::Undefined)
                },
            ));
        atomics_obj
            .borrow_mut()
            .insert_builtin("pause".to_string(), pause_fn);

        // @@toStringTag
        {
            let tag = JsValue::String(JsString::from_str("Atomics"));
            let sym_key = "Symbol(Symbol.toStringTag)".to_string();
            let desc = PropertyDescriptor::data(tag, false, false, true);
            atomics_obj
                .borrow_mut()
                .property_order
                .push(sym_key.clone());
            atomics_obj.borrow_mut().properties.insert(sym_key, desc);
        }

        let atomics_val = JsValue::Object(crate::types::JsObject { id: atomics_id });
        self.realm().global_env
            .borrow_mut()
            .declare("Atomics", BindingKind::Const);
        let _ = self.realm().global_env.borrow_mut().set("Atomics", atomics_val);
    }
}

// ValidateIntegerTypedArray: returns (kind, buffer, byte_offset, element_size, is_bigint)
#[allow(clippy::type_complexity)]
fn validate_integer_typed_array(
    interp: &mut Interpreter,
    ta_val: &JsValue,
    waitable: bool,
) -> Result<
    (
        TypedArrayKind,
        std::rc::Rc<std::cell::RefCell<Vec<u8>>>,
        usize,
        usize,
        bool,
    ),
    JsValue,
> {
    if let JsValue::Object(o) = ta_val
        && let Some(obj) = interp.get_object(o.id)
    {
        let obj_ref = obj.borrow();
        if let Some(ref info) = obj_ref.typed_array_info {
            let kind = info.kind;
            if waitable {
                if !matches!(kind, TypedArrayKind::Int32 | TypedArrayKind::BigInt64) {
                    return Err(interp.create_type_error(
                        "Atomics.wait/notify requires Int32Array or BigInt64Array",
                    ));
                }
            } else if matches!(
                kind,
                TypedArrayKind::Float32 | TypedArrayKind::Float64 | TypedArrayKind::Uint8Clamped
            ) {
                return Err(
                    interp.create_type_error("Atomics operations require integer typed arrays")
                );
            }
            if info.is_detached.get() {
                return Err(interp.create_type_error("typed array is detached"));
            }
            let element_size = kind.bytes_per_element();
            let is_bigint = kind.is_bigint();
            return Ok((
                kind,
                info.buffer.clone(),
                info.byte_offset,
                element_size,
                is_bigint,
            ));
        }
    }
    Err(interp.create_type_error("first argument must be a typed array"))
}

fn validate_atomic_access(
    interp: &mut Interpreter,
    ta_val: &JsValue,
    index_val: &JsValue,
    element_size: usize,
) -> Result<usize, JsValue> {
    let idx = match interp.to_index(index_val) {
        Completion::Normal(JsValue::Number(n)) => n as usize,
        Completion::Throw(e) => return Err(e),
        _ => 0,
    };
    // Get array length
    let array_length = if let JsValue::Object(o) = ta_val
        && let Some(obj) = interp.get_object(o.id)
    {
        let obj_ref = obj.borrow();
        if let Some(ref info) = obj_ref.typed_array_info {
            info.array_length
        } else {
            0
        }
    } else {
        0
    };
    if idx >= array_length {
        return Err(interp.create_error("RangeError", "index out of range"));
    }
    Ok(idx * element_size)
}

// Read-modify-write helper for Atomics.add/sub/and/or/xor/exchange
fn atomics_rmw(
    interp: &mut Interpreter,
    args: &[JsValue],
    num_op: fn(i64, i64) -> i64,
    bigint_op: fn(i64, i64) -> i64,
) -> Completion {
    let ta_val = args.first().cloned().unwrap_or(JsValue::Undefined);
    let index_val = args.get(1).cloned().unwrap_or(JsValue::Undefined);
    let value = args.get(2).cloned().unwrap_or(JsValue::Undefined);
    let (kind, buffer, byte_offset, element_size, is_bigint) =
        match validate_integer_typed_array(interp, &ta_val, false) {
            Ok(info) => info,
            Err(e) => return Completion::Throw(e),
        };
    // Convert value before access validation (per spec)
    let converted = if is_bigint {
        match interp.to_bigint_value(&value) {
            Ok(v) => v,
            Err(e) => return Completion::Throw(e),
        }
    } else {
        match interp.to_number_value(&value) {
            Ok(n) => JsValue::Number(n),
            Err(e) => return Completion::Throw(e),
        }
    };
    let byte_index = match validate_atomic_access(interp, &ta_val, &index_val, element_size) {
        Ok(i) => i,
        Err(e) => return Completion::Throw(e),
    };
    let offset = byte_offset + byte_index;
    let mut buf = buffer.borrow_mut();
    if is_bigint {
        let old_bytes = read_bigint_raw_bytes(&buf, offset, kind);
        let old_i64 = i64::from_le_bytes(old_bytes);
        let new_i64 = match &converted {
            JsValue::BigInt(b) => i64::try_from(&b.value).unwrap_or(0),
            _ => 0,
        };
        let result_i64 = bigint_op(old_i64, new_i64);
        let result_bytes = result_i64.to_le_bytes();
        buf[offset..offset + 8].copy_from_slice(&result_bytes);
        let old_val = bigint_from_raw_bytes(kind, &old_bytes);
        Completion::Normal(old_val)
    } else {
        let old_raw = read_number_raw_bytes(&buf, offset, kind);
        let old_i64 = number_raw_to_i64(kind, &old_raw);
        let new_i64 = converted_number_to_i64(kind, &converted);
        let result_i64 = num_op(old_i64, new_i64);
        write_i64_to_buffer(&mut buf, offset, kind, result_i64);
        let old_val = number_from_raw_bytes(kind, &old_raw);
        Completion::Normal(old_val)
    }
}

// Buffer read/write helpers

fn read_number_from_buffer(buf: &[u8], offset: usize, kind: TypedArrayKind) -> JsValue {
    let raw = read_number_raw_bytes(buf, offset, kind);
    number_from_raw_bytes(kind, &raw)
}

fn read_bigint_from_buffer(buf: &[u8], offset: usize, kind: TypedArrayKind) -> JsValue {
    let raw = read_bigint_raw_bytes(buf, offset, kind);
    bigint_from_raw_bytes(kind, &raw)
}

fn read_number_raw_bytes(buf: &[u8], offset: usize, kind: TypedArrayKind) -> [u8; 8] {
    let mut raw = [0u8; 8];
    let size = kind.bytes_per_element();
    if offset + size <= buf.len() {
        raw[..size].copy_from_slice(&buf[offset..offset + size]);
    }
    raw
}

fn read_bigint_raw_bytes(buf: &[u8], offset: usize, _kind: TypedArrayKind) -> [u8; 8] {
    let mut raw = [0u8; 8];
    if offset + 8 <= buf.len() {
        raw.copy_from_slice(&buf[offset..offset + 8]);
    }
    raw
}

fn number_from_raw_bytes(kind: TypedArrayKind, raw: &[u8; 8]) -> JsValue {
    match kind {
        TypedArrayKind::Int8 => JsValue::Number(raw[0] as i8 as f64),
        TypedArrayKind::Uint8 => JsValue::Number(raw[0] as f64),
        TypedArrayKind::Uint8Clamped => JsValue::Number(raw[0] as f64),
        TypedArrayKind::Int16 => {
            let v = i16::from_le_bytes([raw[0], raw[1]]);
            JsValue::Number(v as f64)
        }
        TypedArrayKind::Uint16 => {
            let v = u16::from_le_bytes([raw[0], raw[1]]);
            JsValue::Number(v as f64)
        }
        TypedArrayKind::Int32 => {
            let v = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
            JsValue::Number(v as f64)
        }
        TypedArrayKind::Uint32 => {
            let v = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
            JsValue::Number(v as f64)
        }
        _ => JsValue::Number(0.0),
    }
}

fn bigint_from_raw_bytes(kind: TypedArrayKind, raw: &[u8; 8]) -> JsValue {
    let i = i64::from_le_bytes(*raw);
    match kind {
        TypedArrayKind::BigInt64 => JsValue::BigInt(JsBigInt {
            value: num_bigint::BigInt::from(i),
        }),
        TypedArrayKind::BigUint64 => {
            let u = u64::from_le_bytes(*raw);
            JsValue::BigInt(JsBigInt {
                value: num_bigint::BigInt::from(u),
            })
        }
        _ => JsValue::Number(0.0),
    }
}

fn number_to_raw_bytes(kind: TypedArrayKind, val: &JsValue) -> [u8; 8] {
    let mut raw = [0u8; 8];
    let i = converted_number_to_i64(kind, val);
    let size = kind.bytes_per_element();
    let bytes = i.to_le_bytes();
    raw[..size].copy_from_slice(&bytes[..size]);
    raw
}

fn bigint_to_raw_bytes(kind: TypedArrayKind, val: &JsValue) -> [u8; 8] {
    match val {
        JsValue::BigInt(b) => {
            let i = i64::try_from(&b.value).unwrap_or(0);
            match kind {
                TypedArrayKind::BigInt64 => i.to_le_bytes(),
                TypedArrayKind::BigUint64 => (i as u64).to_le_bytes(),
                _ => [0u8; 8],
            }
        }
        _ => [0u8; 8],
    }
}

fn write_number_to_buffer(buf: &mut [u8], offset: usize, kind: TypedArrayKind, val: &JsValue) {
    let i = converted_number_to_i64(kind, val);
    let size = kind.bytes_per_element();
    let bytes = i.to_le_bytes();
    if offset + size <= buf.len() {
        buf[offset..offset + size].copy_from_slice(&bytes[..size]);
    }
}

fn write_bigint_to_buffer(buf: &mut [u8], offset: usize, kind: TypedArrayKind, val: &JsValue) {
    if let JsValue::BigInt(b) = val {
        let i = i64::try_from(&b.value).unwrap_or(0);
        match kind {
            TypedArrayKind::BigInt64 | TypedArrayKind::BigUint64 => {
                let bytes = i.to_le_bytes();
                if offset + 8 <= buf.len() {
                    buf[offset..offset + 8].copy_from_slice(&bytes);
                }
            }
            _ => {}
        }
    }
}

fn number_raw_to_i64(kind: TypedArrayKind, raw: &[u8; 8]) -> i64 {
    match kind {
        TypedArrayKind::Int8 => raw[0] as i8 as i64,
        TypedArrayKind::Uint8 => raw[0] as i64,
        TypedArrayKind::Uint8Clamped => raw[0] as i64,
        TypedArrayKind::Int16 => i16::from_le_bytes([raw[0], raw[1]]) as i64,
        TypedArrayKind::Uint16 => u16::from_le_bytes([raw[0], raw[1]]) as i64,
        TypedArrayKind::Int32 => i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as i64,
        TypedArrayKind::Uint32 => u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as i64,
        _ => 0,
    }
}

fn converted_number_to_i64(kind: TypedArrayKind, val: &JsValue) -> i64 {
    let n = match val {
        JsValue::Number(n) => *n,
        _ => 0.0,
    };
    if n.is_nan() || n.is_infinite() || n == 0.0 {
        return 0;
    }
    // Use modular integer conversion matching JS semantics (ToInt32/ToUint32 style)
    let int_val = n.trunc();
    match kind {
        TypedArrayKind::Int8 => {
            let v = (int_val as i64) & 0xFF;
            if v >= 128 { v - 256 } else { v }
        }
        TypedArrayKind::Uint8 | TypedArrayKind::Uint8Clamped => (int_val as i64) & 0xFF,
        TypedArrayKind::Int16 => {
            let v = (int_val as i64) & 0xFFFF;
            if v >= 32768 { v - 65536 } else { v }
        }
        TypedArrayKind::Uint16 => (int_val as i64) & 0xFFFF,
        TypedArrayKind::Int32 => (int_val as i64) & 0xFFFFFFFF_i64,
        TypedArrayKind::Uint32 => (int_val as i64) & 0xFFFFFFFF_i64,
        _ => 0,
    }
}

fn write_i64_to_buffer(buf: &mut [u8], offset: usize, kind: TypedArrayKind, val: i64) {
    match kind {
        TypedArrayKind::Int8 => {
            if offset < buf.len() {
                buf[offset] = val as i8 as u8;
            }
        }
        TypedArrayKind::Uint8 => {
            if offset < buf.len() {
                buf[offset] = val as u8;
            }
        }
        TypedArrayKind::Uint8Clamped => {
            if offset < buf.len() {
                buf[offset] = val as u8;
            }
        }
        TypedArrayKind::Int16 => {
            let bytes = (val as i16).to_le_bytes();
            if offset + 2 <= buf.len() {
                buf[offset..offset + 2].copy_from_slice(&bytes);
            }
        }
        TypedArrayKind::Uint16 => {
            let bytes = (val as u16).to_le_bytes();
            if offset + 2 <= buf.len() {
                buf[offset..offset + 2].copy_from_slice(&bytes);
            }
        }
        TypedArrayKind::Int32 => {
            let bytes = (val as i32).to_le_bytes();
            if offset + 4 <= buf.len() {
                buf[offset..offset + 4].copy_from_slice(&bytes);
            }
        }
        TypedArrayKind::Uint32 => {
            let bytes = (val as u32).to_le_bytes();
            if offset + 4 <= buf.len() {
                buf[offset..offset + 4].copy_from_slice(&bytes);
            }
        }
        _ => {}
    }
}
