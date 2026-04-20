use super::super::*;
use crate::interpreter::types::{BufferData, SharedBufferInner};
use crate::types::{JsBigInt, JsObject, JsString, JsValue};
use rustc_hash::FxHashMap as HashMap;
use std::sync::atomic::{
    AtomicI8, AtomicI16, AtomicI32, AtomicI64, AtomicU8, AtomicU16, AtomicU32, Ordering,
};
use std::sync::{Arc, Condvar, LazyLock, Mutex};

struct WaiterEntry {
    notified: Arc<(Mutex<bool>, Condvar)>,
}

static WAITER_MAP: LazyLock<Mutex<HashMap<(u64, usize), Vec<WaiterEntry>>>> =
    LazyLock::new(|| Mutex::new(HashMap::default()));

fn check_ta_detached(interp: &mut Interpreter, ta_val: &JsValue) -> Result<(), JsValue> {
    if let JsValue::Object(o) = ta_val
        && let Some(obj) = interp.get_object(o.id)
    {
        let obj_ref = obj.borrow();
        if let Some(ref info) = obj_ref.typed_array_info
            && info.is_detached.get()
        {
            return Err(interp.create_type_error("typed array is detached"));
        }
    }
    Ok(())
}

fn get_sab_info(interp: &Interpreter, ta_val: &JsValue) -> Option<(Arc<SharedBufferInner>, usize)> {
    if let JsValue::Object(o) = ta_val
        && let Some(obj) = interp.get_object(o.id)
    {
        let obj_ref = obj.borrow();
        let byte_offset = obj_ref
            .typed_array_info
            .as_ref()
            .map(|i| i.byte_offset)
            .unwrap_or(0);
        if let Some(buf_id) = obj_ref.view_buffer_object_id
            && let Some(buf_obj) = interp.get_object(buf_id)
        {
            let buf_ref = buf_obj.borrow();
            if buf_ref.arraybuffer_is_shared
                && let Some(ref inner) = buf_ref.sab_shared
            {
                return Some((inner.clone(), byte_offset));
            }
        }
    }
    None
}

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
                if let Err(e) = check_ta_detached(interp, &ta_val) {
                    return Completion::Throw(e);
                }
                let offset = byte_offset + byte_index;
                if let Some((sab, _ta_byte_offset)) = get_sab_info(interp, &ta_val) {
                    return atomic_load_shared(&sab, offset, kind, is_bigint);
                }
                (*buffer.borrow()).with_read(|buf| {
                    if is_bigint {
                        Completion::Normal(read_bigint_from_buffer(buf, offset, kind))
                    } else {
                        Completion::Normal(read_number_from_buffer(buf, offset, kind))
                    }
                })
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
                let byte_index =
                    match validate_atomic_access(interp, &ta_val, &index_val, element_size) {
                        Ok(i) => i,
                        Err(e) => return Completion::Throw(e),
                    };
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
                    let int_val = if n.is_nan() || n == 0.0 {
                        0.0
                    } else if n.is_infinite() {
                        n
                    } else {
                        n.trunc()
                    };
                    (JsValue::Number(n), JsValue::Number(int_val))
                };
                if let Err(e) = check_ta_detached(interp, &ta_val) {
                    return Completion::Throw(e);
                }
                let offset = byte_offset + byte_index;
                if let Some((sab, _ta_byte_offset)) = get_sab_info(interp, &ta_val) {
                    atomic_store_shared(&sab, offset, kind, is_bigint, &converted);
                    return Completion::Normal(return_val);
                }
                (*buffer.borrow_mut()).with_write(|buf| {
                    if is_bigint {
                        write_bigint_to_buffer(buf, offset, kind, &converted);
                    } else {
                        write_number_to_buffer(buf, offset, kind, &converted);
                    }
                });
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
                if let Err(e) = check_ta_detached(interp, &ta_val) {
                    return Completion::Throw(e);
                }
                let offset = byte_offset + byte_index;
                if let Some((sab, _ta_byte_offset)) = get_sab_info(interp, &ta_val) {
                    return atomic_compare_exchange_shared(
                        &sab,
                        offset,
                        kind,
                        is_bigint,
                        &expected,
                        &replacement,
                    );
                }
                (*buffer.borrow_mut()).with_write(|buf| {
                    if is_bigint {
                        let old = read_bigint_raw_bytes(buf, offset, kind);
                        let exp_bytes = bigint_to_raw_bytes(kind, &expected);
                        if old == exp_bytes {
                            write_bigint_to_buffer(buf, offset, kind, &replacement);
                        }
                        Completion::Normal(bigint_from_raw_bytes(kind, &old))
                    } else {
                        let old = read_number_raw_bytes(buf, offset, kind);
                        let exp_bytes = number_to_raw_bytes(kind, &expected);
                        if old == exp_bytes {
                            write_number_to_buffer(buf, offset, kind, &replacement);
                        }
                        Completion::Normal(number_from_raw_bytes(kind, &old))
                    }
                })
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

        // Atomics.wait
        let wait_fn = self.create_function(JsFunction::native(
            "wait".to_string(),
            4,
            |interp, _this, args| {
                let ta_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let index_val = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let value = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                let (_kind, _buffer, byte_offset, element_size, is_bigint) =
                    match validate_integer_typed_array(interp, &ta_val, true) {
                        Ok(info) => info,
                        Err(e) => return Completion::Throw(e),
                    };
                // Step 3: IsSharedArrayBuffer check — before index/value/timeout
                let sab_info = get_sab_info(interp, &ta_val);
                if sab_info.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Atomics.wait requires a shared typed array"),
                    );
                }
                let (sab, _ta_byte_offset) = sab_info.unwrap();
                let byte_index =
                    match validate_atomic_access(interp, &ta_val, &index_val, element_size) {
                        Ok(i) => i,
                        Err(e) => return Completion::Throw(e),
                    };
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

                let offset = byte_offset + byte_index;

                let current_matches = if is_bigint {
                    let current = sab
                        .with_atomic_ptr::<i64, _>(offset, 8, |ptr| unsafe {
                            AtomicI64::from_ptr(ptr).load(Ordering::SeqCst)
                        })
                        .unwrap_or(0);
                    let expected = match &converted {
                        JsValue::BigInt(b) => i64::try_from(&b.value).unwrap_or(0),
                        _ => 0,
                    };
                    current == expected
                } else {
                    let current = sab
                        .with_atomic_ptr::<i32, _>(offset, 4, |ptr| unsafe {
                            AtomicI32::from_ptr(ptr).load(Ordering::SeqCst)
                        })
                        .unwrap_or(0);
                    let expected = match &converted {
                        JsValue::Number(n) => *n as i32,
                        _ => 0,
                    };
                    current == expected
                };

                if !current_matches {
                    return Completion::Normal(JsValue::String(JsString::from_str("not-equal")));
                }

                // §25.4.12 step 14-15: AgentCanSuspend() check
                if !interp.can_block {
                    return Completion::Throw(
                        interp.create_type_error("Atomics.wait cannot block on the main thread"),
                    );
                }

                if timeout == 0.0 {
                    return Completion::Normal(JsValue::String(JsString::from_str("timed-out")));
                }

                let pair = Arc::new((Mutex::new(false), Condvar::new()));
                let key = (sab.id, offset);
                {
                    let mut map = WAITER_MAP.lock().unwrap();
                    map.entry(key).or_default().push(WaiterEntry {
                        notified: pair.clone(),
                    });
                }

                let (lock, cvar) = &*pair;
                let mut notified = lock.lock().unwrap();
                let result = if timeout == f64::INFINITY {
                    while !*notified {
                        notified = cvar.wait(notified).unwrap();
                    }
                    "ok"
                } else {
                    let duration = std::time::Duration::from_millis(timeout as u64);
                    let deadline = std::time::Instant::now() + duration;
                    while !*notified {
                        let remaining =
                            deadline.saturating_duration_since(std::time::Instant::now());
                        if remaining.is_zero() {
                            break;
                        }
                        let (guard, timeout_result) =
                            cvar.wait_timeout(notified, remaining).unwrap();
                        notified = guard;
                        if timeout_result.timed_out() && !*notified {
                            break;
                        }
                    }
                    if *notified { "ok" } else { "timed-out" }
                };

                if !*notified {
                    let mut map = WAITER_MAP.lock().unwrap();
                    if let Some(waiters) = map.get_mut(&key) {
                        waiters.retain(|w| !Arc::ptr_eq(&w.notified, &pair));
                        if waiters.is_empty() {
                            map.remove(&key);
                        }
                    }
                }

                Completion::Normal(JsValue::String(JsString::from_str(result)))
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
                let (kind, _buffer, byte_offset, element_size, _is_bigint) =
                    match validate_integer_typed_array(interp, &ta_val, true) {
                        Ok(info) => info,
                        Err(e) => return Completion::Throw(e),
                    };
                let _ = kind;
                let byte_index =
                    match validate_atomic_access(interp, &ta_val, &index_val, element_size) {
                        Ok(i) => i,
                        Err(e) => return Completion::Throw(e),
                    };
                let count_val = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                let count: usize = if matches!(count_val, JsValue::Undefined) {
                    usize::MAX
                } else {
                    let c = match interp.to_number_value(&count_val) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    if c.is_nan() || c <= 0.0 {
                        0
                    } else if c.is_infinite() {
                        usize::MAX
                    } else {
                        c.trunc().max(0.0) as usize
                    }
                };

                let sab_info = get_sab_info(interp, &ta_val);
                if sab_info.is_none() {
                    return Completion::Normal(JsValue::Number(0.0));
                }
                let (sab, _ta_byte_offset) = sab_info.unwrap();
                let offset = byte_offset + byte_index;
                let key = (sab.id, offset);

                let mut woken = 0usize;
                let mut map = WAITER_MAP.lock().unwrap();
                if let Some(waiters) = map.get_mut(&key) {
                    let to_wake = count.min(waiters.len());
                    for entry in waiters.drain(..to_wake) {
                        let (lock, cvar) = &*entry.notified;
                        let mut notified = lock.lock().unwrap();
                        *notified = true;
                        cvar.notify_one();
                        woken += 1;
                    }
                    if waiters.is_empty() {
                        map.remove(&key);
                    }
                }

                Completion::Normal(JsValue::Number(woken as f64))
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
                // Step 3: IsSharedArrayBuffer check — before index/value/timeout
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
                if !buffer_is_shared {
                    return Completion::Throw(
                        interp.create_type_error("Atomics.waitAsync requires a shared typed array"),
                    );
                }
                let _ = kind;
                let byte_index =
                    match validate_atomic_access(interp, &ta_val, &index_val, element_size) {
                        Ok(i) => i,
                        Err(e) => return Completion::Throw(e),
                    };
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

                let offset = byte_offset + byte_index;
                let current_matches = (*buffer.borrow()).with_read(|buf| {
                    if is_bigint {
                        let current = read_bigint_raw_bytes(buf, offset, kind);
                        let expected = bigint_to_raw_bytes(kind, &converted);
                        current == expected
                    } else {
                        let current = read_number_raw_bytes(buf, offset, kind);
                        let expected = number_to_raw_bytes(kind, &converted);
                        current == expected
                    }
                });

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
                    let sab_info = get_sab_info(interp, &ta_val);
                    let (resolve_fn, _reject_fn, promise_val) = interp.create_promise_parts();
                    interp.gc_root_value(&resolve_fn);
                    interp.gc_root_value(&promise_val);

                    if let Some((sab, _)) = sab_info {
                        let key = (sab.id, offset);
                        let pair = Arc::new((Mutex::new(false), Condvar::new()));
                        {
                            let mut map = WAITER_MAP.lock().unwrap();
                            map.entry(key).or_default().push(WaiterEntry {
                                notified: pair.clone(),
                            });
                        }
                        let pair_clone = pair.clone();
                        let resolve_clone = resolve_fn.clone();
                        let timeout_ms = timeout;
                        let pending = interp.agent_async_completions.clone();
                        let pending_jobs = interp.pending_async_jobs.clone();
                        let pending_promise_ids = interp.pending_async_promise_ids.clone();
                        let promise_id = if let JsValue::Object(ref o) = promise_val {
                            o.id
                        } else {
                            0
                        };
                        if promise_id != 0 {
                            pending_promise_ids.lock().unwrap().insert(promise_id);
                        }
                        pending_jobs.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        std::thread::spawn(move || {
                            let (lock, cvar) = &*pair_clone;
                            let mut notified = lock.lock().unwrap();
                            let result_str = if timeout_ms == f64::INFINITY {
                                while !*notified {
                                    notified = cvar.wait(notified).unwrap();
                                }
                                "ok"
                            } else {
                                let duration = std::time::Duration::from_millis(timeout_ms as u64);
                                let deadline = std::time::Instant::now() + duration;
                                while !*notified {
                                    let remaining = deadline
                                        .saturating_duration_since(std::time::Instant::now());
                                    if remaining.is_zero() {
                                        break;
                                    }
                                    let (guard, _) =
                                        cvar.wait_timeout(notified, remaining).unwrap();
                                    notified = guard;
                                }
                                if *notified { "ok" } else { "timed-out" }
                            };
                            if !*notified {
                                let mut map = WAITER_MAP.lock().unwrap();
                                if let Some(waiters) = map.get_mut(&key) {
                                    waiters.retain(|w| !Arc::ptr_eq(&w.notified, &pair_clone));
                                    if waiters.is_empty() {
                                        map.remove(&key);
                                    }
                                }
                            }
                            let result_val = JsValue::String(JsString::from_str(result_str));
                            let resolve = resolve_clone;
                            let (ref mtx, ref completion_cvar) = *pending;
                            mtx.lock()
                                .unwrap()
                                .push(Box::new(move |interp: &mut Interpreter| {
                                    let _ = interp.call_function(
                                        &resolve,
                                        &JsValue::Undefined,
                                        &[result_val],
                                    );
                                    interp.gc_unroot_value(&resolve);
                                }));
                            if promise_id != 0 {
                                pending_promise_ids.lock().unwrap().remove(&promise_id);
                            }
                            pending_jobs.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                            completion_cvar.notify_one();
                        });
                    }

                    result.borrow_mut().insert_property(
                        "async".to_string(),
                        PropertyDescriptor::data(JsValue::Boolean(true), true, true, true),
                    );
                    result.borrow_mut().insert_property(
                        "value".to_string(),
                        PropertyDescriptor::data(promise_val.clone(), true, true, true),
                    );
                    // resolve_fn stays rooted until completion callback runs
                    interp.gc_unroot_value(&promise_val);
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
                        let n = if let JsValue::Number(n) = arg {
                            *n
                        } else {
                            return Completion::Throw(interp.create_type_error(
                                "Atomics.pause requires a non-negative integer",
                            ));
                        };
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
        self.realm()
            .global_env
            .borrow_mut()
            .declare("Atomics", BindingKind::Const);
        self.realm()
            .global_env
            .borrow_mut()
            .initialize_binding("Atomics", atomics_val);
    }
}

// Atomic operations on shared buffers

fn atomic_load_shared(
    sab: &SharedBufferInner,
    offset: usize,
    kind: TypedArrayKind,
    is_bigint: bool,
) -> Completion {
    if is_bigint {
        let val = sab
            .with_atomic_ptr::<i64, _>(offset, 8, |ptr| unsafe {
                AtomicI64::from_ptr(ptr).load(Ordering::SeqCst)
            })
            .unwrap_or(0);
        let bv = match kind {
            TypedArrayKind::BigInt64 => JsValue::BigInt(JsBigInt {
                value: num_bigint::BigInt::from(val),
            }),
            TypedArrayKind::BigUint64 => JsValue::BigInt(JsBigInt {
                value: num_bigint::BigInt::from(val as u64),
            }),
            _ => JsValue::Number(0.0),
        };
        Completion::Normal(bv)
    } else {
        let val: i64 = match kind {
            TypedArrayKind::Int8 => sab
                .with_atomic_ptr::<i8, _>(offset, 1, |ptr| unsafe {
                    AtomicI8::from_ptr(ptr).load(Ordering::SeqCst)
                })
                .map(|v| v as i64)
                .unwrap_or(0),
            TypedArrayKind::Uint8 => sab
                .with_atomic_ptr::<u8, _>(offset, 1, |ptr| unsafe {
                    AtomicU8::from_ptr(ptr).load(Ordering::SeqCst)
                })
                .map(|v| v as i64)
                .unwrap_or(0),
            TypedArrayKind::Int16 => sab
                .with_atomic_ptr::<i16, _>(offset, 2, |ptr| unsafe {
                    AtomicI16::from_ptr(ptr).load(Ordering::SeqCst)
                })
                .map(|v| v as i64)
                .unwrap_or(0),
            TypedArrayKind::Uint16 => sab
                .with_atomic_ptr::<u16, _>(offset, 2, |ptr| unsafe {
                    AtomicU16::from_ptr(ptr).load(Ordering::SeqCst)
                })
                .map(|v| v as i64)
                .unwrap_or(0),
            TypedArrayKind::Int32 => sab
                .with_atomic_ptr::<i32, _>(offset, 4, |ptr| unsafe {
                    AtomicI32::from_ptr(ptr).load(Ordering::SeqCst)
                })
                .map(|v| v as i64)
                .unwrap_or(0),
            TypedArrayKind::Uint32 => sab
                .with_atomic_ptr::<u32, _>(offset, 4, |ptr| unsafe {
                    AtomicU32::from_ptr(ptr).load(Ordering::SeqCst)
                })
                .map(|v| v as i64)
                .unwrap_or(0),
            _ => 0,
        };
        Completion::Normal(JsValue::Number(val as f64))
    }
}

fn atomic_store_shared(
    sab: &SharedBufferInner,
    offset: usize,
    kind: TypedArrayKind,
    is_bigint: bool,
    val: &JsValue,
) {
    if is_bigint {
        let i = match val {
            JsValue::BigInt(b) => i64::try_from(&b.value).unwrap_or(0),
            _ => 0,
        };
        let _ = sab.with_atomic_ptr::<i64, _>(offset, 8, |ptr| unsafe {
            AtomicI64::from_ptr(ptr).store(i, Ordering::SeqCst)
        });
    } else {
        let n = match val {
            JsValue::Number(n) => *n,
            _ => 0.0,
        };
        let i = converted_number_to_i64_val(kind, n);
        match kind {
            TypedArrayKind::Int8 => {
                let _ = sab.with_atomic_ptr::<i8, _>(offset, 1, |ptr| unsafe {
                    AtomicI8::from_ptr(ptr).store(i as i8, Ordering::SeqCst)
                });
            }
            TypedArrayKind::Uint8 => {
                let _ = sab.with_atomic_ptr::<u8, _>(offset, 1, |ptr| unsafe {
                    AtomicU8::from_ptr(ptr).store(i as u8, Ordering::SeqCst)
                });
            }
            TypedArrayKind::Int16 => {
                let _ = sab.with_atomic_ptr::<i16, _>(offset, 2, |ptr| unsafe {
                    AtomicI16::from_ptr(ptr).store(i as i16, Ordering::SeqCst)
                });
            }
            TypedArrayKind::Uint16 => {
                let _ = sab.with_atomic_ptr::<u16, _>(offset, 2, |ptr| unsafe {
                    AtomicU16::from_ptr(ptr).store(i as u16, Ordering::SeqCst)
                });
            }
            TypedArrayKind::Int32 => {
                let _ = sab.with_atomic_ptr::<i32, _>(offset, 4, |ptr| unsafe {
                    AtomicI32::from_ptr(ptr).store(i as i32, Ordering::SeqCst)
                });
            }
            TypedArrayKind::Uint32 => {
                let _ = sab.with_atomic_ptr::<u32, _>(offset, 4, |ptr| unsafe {
                    AtomicU32::from_ptr(ptr).store(i as u32, Ordering::SeqCst)
                });
            }
            _ => {}
        }
    }
}

fn atomic_compare_exchange_shared(
    sab: &SharedBufferInner,
    offset: usize,
    kind: TypedArrayKind,
    is_bigint: bool,
    expected: &JsValue,
    replacement: &JsValue,
) -> Completion {
    if is_bigint {
        let exp = match expected {
            JsValue::BigInt(b) => i64::try_from(&b.value).unwrap_or(0),
            _ => 0,
        };
        let rep = match replacement {
            JsValue::BigInt(b) => i64::try_from(&b.value).unwrap_or(0),
            _ => 0,
        };
        let old = sab
            .with_atomic_ptr::<i64, _>(offset, 8, |ptr| unsafe {
                AtomicI64::from_ptr(ptr)
                    .compare_exchange(exp, rep, Ordering::SeqCst, Ordering::SeqCst)
                    .unwrap_or_else(|v| v)
            })
            .unwrap_or(0);
        let bv = match kind {
            TypedArrayKind::BigInt64 => JsValue::BigInt(JsBigInt {
                value: num_bigint::BigInt::from(old),
            }),
            TypedArrayKind::BigUint64 => JsValue::BigInt(JsBigInt {
                value: num_bigint::BigInt::from(old as u64),
            }),
            _ => JsValue::Number(0.0),
        };
        Completion::Normal(bv)
    } else {
        match kind {
            TypedArrayKind::Int8 => {
                let exp = converted_number_to_i64_val(
                    kind,
                    match expected {
                        JsValue::Number(n) => *n,
                        _ => 0.0,
                    },
                ) as i8;
                let rep = converted_number_to_i64_val(
                    kind,
                    match replacement {
                        JsValue::Number(n) => *n,
                        _ => 0.0,
                    },
                ) as i8;
                let old = sab
                    .with_atomic_ptr::<i8, _>(offset, 1, |ptr| unsafe {
                        AtomicI8::from_ptr(ptr)
                            .compare_exchange(exp, rep, Ordering::SeqCst, Ordering::SeqCst)
                            .unwrap_or_else(|v| v)
                    })
                    .unwrap_or(0);
                Completion::Normal(JsValue::Number(old as f64))
            }
            TypedArrayKind::Uint8 => {
                let exp = converted_number_to_i64_val(
                    kind,
                    match expected {
                        JsValue::Number(n) => *n,
                        _ => 0.0,
                    },
                ) as u8;
                let rep = converted_number_to_i64_val(
                    kind,
                    match replacement {
                        JsValue::Number(n) => *n,
                        _ => 0.0,
                    },
                ) as u8;
                let old = sab
                    .with_atomic_ptr::<u8, _>(offset, 1, |ptr| unsafe {
                        AtomicU8::from_ptr(ptr)
                            .compare_exchange(exp, rep, Ordering::SeqCst, Ordering::SeqCst)
                            .unwrap_or_else(|v| v)
                    })
                    .unwrap_or(0);
                Completion::Normal(JsValue::Number(old as f64))
            }
            TypedArrayKind::Int16 => {
                let exp = converted_number_to_i64_val(
                    kind,
                    match expected {
                        JsValue::Number(n) => *n,
                        _ => 0.0,
                    },
                ) as i16;
                let rep = converted_number_to_i64_val(
                    kind,
                    match replacement {
                        JsValue::Number(n) => *n,
                        _ => 0.0,
                    },
                ) as i16;
                let old = sab
                    .with_atomic_ptr::<i16, _>(offset, 2, |ptr| unsafe {
                        AtomicI16::from_ptr(ptr)
                            .compare_exchange(exp, rep, Ordering::SeqCst, Ordering::SeqCst)
                            .unwrap_or_else(|v| v)
                    })
                    .unwrap_or(0);
                Completion::Normal(JsValue::Number(old as f64))
            }
            TypedArrayKind::Uint16 => {
                let exp = converted_number_to_i64_val(
                    kind,
                    match expected {
                        JsValue::Number(n) => *n,
                        _ => 0.0,
                    },
                ) as u16;
                let rep = converted_number_to_i64_val(
                    kind,
                    match replacement {
                        JsValue::Number(n) => *n,
                        _ => 0.0,
                    },
                ) as u16;
                let old = sab
                    .with_atomic_ptr::<u16, _>(offset, 2, |ptr| unsafe {
                        AtomicU16::from_ptr(ptr)
                            .compare_exchange(exp, rep, Ordering::SeqCst, Ordering::SeqCst)
                            .unwrap_or_else(|v| v)
                    })
                    .unwrap_or(0);
                Completion::Normal(JsValue::Number(old as f64))
            }
            TypedArrayKind::Int32 => {
                let exp = converted_number_to_i64_val(
                    kind,
                    match expected {
                        JsValue::Number(n) => *n,
                        _ => 0.0,
                    },
                ) as i32;
                let rep = converted_number_to_i64_val(
                    kind,
                    match replacement {
                        JsValue::Number(n) => *n,
                        _ => 0.0,
                    },
                ) as i32;
                let old = sab
                    .with_atomic_ptr::<i32, _>(offset, 4, |ptr| unsafe {
                        AtomicI32::from_ptr(ptr)
                            .compare_exchange(exp, rep, Ordering::SeqCst, Ordering::SeqCst)
                            .unwrap_or_else(|v| v)
                    })
                    .unwrap_or(0);
                Completion::Normal(JsValue::Number(old as f64))
            }
            TypedArrayKind::Uint32 => {
                let exp = converted_number_to_i64_val(
                    kind,
                    match expected {
                        JsValue::Number(n) => *n,
                        _ => 0.0,
                    },
                ) as u32;
                let rep = converted_number_to_i64_val(
                    kind,
                    match replacement {
                        JsValue::Number(n) => *n,
                        _ => 0.0,
                    },
                ) as u32;
                let old = sab
                    .with_atomic_ptr::<u32, _>(offset, 4, |ptr| unsafe {
                        AtomicU32::from_ptr(ptr)
                            .compare_exchange(exp, rep, Ordering::SeqCst, Ordering::SeqCst)
                            .unwrap_or_else(|v| v)
                    })
                    .unwrap_or(0);
                Completion::Normal(JsValue::Number(old as f64))
            }
            _ => Completion::Normal(JsValue::Number(0.0)),
        }
    }
}

fn atomic_rmw_shared(
    sab: &SharedBufferInner,
    offset: usize,
    kind: TypedArrayKind,
    is_bigint: bool,
    converted: &JsValue,
    num_op: fn(i64, i64) -> i64,
    bigint_op: fn(i64, i64) -> i64,
) -> Completion {
    if is_bigint {
        let new_i64 = match converted {
            JsValue::BigInt(b) => i64::try_from(&b.value).unwrap_or(0),
            _ => 0,
        };
        let old = sab
            .with_atomic_ptr::<i64, _>(offset, 8, |ptr| unsafe {
                let atomic = AtomicI64::from_ptr(ptr);
                loop {
                    let current = atomic.load(Ordering::SeqCst);
                    let result = bigint_op(current, new_i64);
                    match atomic.compare_exchange(
                        current,
                        result,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    ) {
                        Ok(v) => break v,
                        Err(_) => continue,
                    }
                }
            })
            .unwrap_or(0);
        let bv = match kind {
            TypedArrayKind::BigInt64 => JsValue::BigInt(JsBigInt {
                value: num_bigint::BigInt::from(old),
            }),
            TypedArrayKind::BigUint64 => JsValue::BigInt(JsBigInt {
                value: num_bigint::BigInt::from(old as u64),
            }),
            _ => JsValue::Number(0.0),
        };
        Completion::Normal(bv)
    } else {
        let n = match converted {
            JsValue::Number(n) => *n,
            _ => 0.0,
        };
        let new_i64 = converted_number_to_i64_val(kind, n);
        macro_rules! do_rmw {
            ($ty:ty, $size:expr, $atomic_ty:ty) => {{
                let old = sab
                    .with_atomic_ptr::<$ty, _>(offset, $size, |ptr| unsafe {
                        let atomic = <$atomic_ty>::from_ptr(ptr);
                        loop {
                            let current = atomic.load(Ordering::SeqCst);
                            let result = num_op(current as i64, new_i64) as $ty;
                            match atomic.compare_exchange(
                                current,
                                result,
                                Ordering::SeqCst,
                                Ordering::SeqCst,
                            ) {
                                Ok(v) => break v,
                                Err(_) => continue,
                            }
                        }
                    })
                    .unwrap_or(0 as $ty);
                Completion::Normal(JsValue::Number(old as f64))
            }};
        }
        match kind {
            TypedArrayKind::Int8 => do_rmw!(i8, 1, AtomicI8),
            TypedArrayKind::Uint8 => do_rmw!(u8, 1, AtomicU8),
            TypedArrayKind::Int16 => do_rmw!(i16, 2, AtomicI16),
            TypedArrayKind::Uint16 => do_rmw!(u16, 2, AtomicU16),
            TypedArrayKind::Int32 => do_rmw!(i32, 4, AtomicI32),
            TypedArrayKind::Uint32 => do_rmw!(u32, 4, AtomicU32),
            _ => Completion::Normal(JsValue::Number(0.0)),
        }
    }
}

fn converted_number_to_i64_val(kind: TypedArrayKind, n: f64) -> i64 {
    if n.is_nan() || n.is_infinite() || n == 0.0 {
        return 0;
    }
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

// ValidateIntegerTypedArray: returns (kind, buffer, byte_offset, element_size, is_bigint)
#[allow(clippy::type_complexity)]
fn validate_integer_typed_array(
    interp: &mut Interpreter,
    ta_val: &JsValue,
    waitable: bool,
) -> Result<
    (
        TypedArrayKind,
        std::rc::Rc<std::cell::RefCell<BufferData>>,
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
                TypedArrayKind::Float16
                    | TypedArrayKind::Float32
                    | TypedArrayKind::Float64
                    | TypedArrayKind::Uint8Clamped
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
    let byte_index = match validate_atomic_access(interp, &ta_val, &index_val, element_size) {
        Ok(i) => i,
        Err(e) => return Completion::Throw(e),
    };
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
    if let Err(e) = check_ta_detached(interp, &ta_val) {
        return Completion::Throw(e);
    }
    let offset = byte_offset + byte_index;

    if let Some((sab, _)) = get_sab_info(interp, &ta_val) {
        return atomic_rmw_shared(&sab, offset, kind, is_bigint, &converted, num_op, bigint_op);
    }

    (*buffer.borrow_mut()).with_write(|buf| {
        if is_bigint {
            let old_bytes = read_bigint_raw_bytes(buf, offset, kind);
            let old_i64 = i64::from_le_bytes(old_bytes);
            let new_i64 = match &converted {
                JsValue::BigInt(b) => i64::try_from(&b.value).unwrap_or(0),
                _ => 0,
            };
            let result_i64 = bigint_op(old_i64, new_i64);
            let result_bytes = result_i64.to_le_bytes();
            buf[offset..offset + 8].copy_from_slice(&result_bytes);
            Completion::Normal(bigint_from_raw_bytes(kind, &old_bytes))
        } else {
            let old_raw = read_number_raw_bytes(buf, offset, kind);
            let old_i64 = number_raw_to_i64(kind, &old_raw);
            let new_i64 = converted_number_to_i64(kind, &converted);
            let result_i64 = num_op(old_i64, new_i64);
            write_i64_to_buffer(buf, offset, kind, result_i64);
            Completion::Normal(number_from_raw_bytes(kind, &old_raw))
        }
    })
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
    converted_number_to_i64_val(kind, n)
}

fn write_i64_to_buffer(buf: &mut [u8], offset: usize, kind: TypedArrayKind, val: i64) {
    match kind {
        TypedArrayKind::Int8 if offset < buf.len() => {
            buf[offset] = val as i8 as u8;
        }
        TypedArrayKind::Uint8 if offset < buf.len() => {
            buf[offset] = val as u8;
        }
        TypedArrayKind::Uint8Clamped if offset < buf.len() => {
            buf[offset] = val as u8;
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
