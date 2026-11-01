//! Running wasm for `__host_wasm` (issue #1378): reaching the store, JS-to-wasm calls, wasm-to-JS import
//! trampolines, value conversion, error mapping, and the buffers that alias linear memories.
//!
//! The store lives in `WasmState` between calls. While a call is on the native stack the store is
//! lent to wasmi, so anything re-entering (an import that calls an export, a buffer refresh) goes
//! through the `Caller` the innermost trampoline recorded in `WasmState::caller`.

use super::super::*;
use super::node_host_wasm::*;
use wasmi::{
    AsContext, AsContextMut, Caller, ExternRef, Func, FuncType, Nullable, Ref, StoreContextMut,
    TrapCode, Val,
};

const MAX_DEPTH: u32 = 64;

pub(super) type Ctx<'a> = StoreContextMut<'a, Host>;

/// Run `f` against the store: the one lent to the innermost running import if there is one, else the
/// resting store.
pub(super) fn with_ctx<R>(interp: &mut Interpreter, f: impl FnOnce(&mut Ctx<'_>) -> R) -> R {
    let interp_ptr: *mut Interpreter = interp;
    if let Some(caller) = wasm(interp).caller {
        // SAFETY: `caller` points at the `Caller` local of an import trampoline that is still on the
        // native stack (it is cleared before the trampoline returns), and nothing else uses that
        // `Caller` while the trampoline's JS callback runs.
        let mut ctx = unsafe { &mut *caller }.as_context_mut();
        return f(&mut ctx);
    }
    let mut store = wasm(interp)
        .store
        .take()
        .expect("wasm store is in use without a caller");
    store.data_mut().interp = interp_ptr;
    let result = {
        let mut ctx = store.as_context_mut();
        f(&mut ctx)
    };
    wasm(interp).store = Some(store);
    result
}

pub(super) fn make_host_func(ctx: &mut Ctx<'_>, ty: FuncType, slot: usize) -> Func {
    Func::new(
        &mut *ctx,
        ty,
        move |mut caller: Caller<'_, Host>, params: &[Val], results: &mut [Val]| {
            run_import(&mut caller, slot, params, results)
        },
    )
}

fn run_import(
    caller: &mut Caller<'_, Host>,
    slot: usize,
    params: &[Val],
    results: &mut [Val],
) -> Result<(), wasmi::Error> {
    // SAFETY: the pointer was stored by `with_ctx` from the `&mut Interpreter` whose call is
    // running this trampoline; that reference is not used again until the call returns.
    let interp = unsafe { &mut *caller.data().interp };
    if wasm(interp).depth >= MAX_DEPTH {
        return Err(wasmi::Error::from(TrapCode::StackOverflow));
    }
    let pointer = std::ptr::from_mut(caller).cast::<Caller<'static, Host>>();
    let previous = wasm(interp).caller.replace(pointer);
    wasm(interp).depth += 1;
    let out = import_body(interp, slot, params, results);
    wasm(interp).depth -= 1;
    wasm(interp).caller = previous;
    out
}

fn import_body(
    interp: &mut Interpreter,
    slot: usize,
    params: &[Val],
    results: &mut [Val],
) -> Result<(), wasmi::Error> {
    refresh_buffers(interp);
    let function = slot_get(interp, slot);
    let args: Vec<JsValue> = params.iter().map(|v| from_val(interp, v)).collect();
    let ret = interp.call_function(&function, &JsValue::UNDEFINED, &args);
    match ret {
        Completion::Normal(v) | Completion::Return(v) => {
            let items = array_items(interp, &v);
            for (i, result) in results.iter_mut().enumerate() {
                let item = items.get(i).unwrap_or(JsValue::undefined_ref());
                match to_val(interp, result.ty(), item) {
                    Ok(val) => *result = val,
                    Err(e) => return Err(park(interp, Completion::Throw(e))),
                }
            }
            Ok(())
        }
        other => Err(park(interp, other)),
    }
}

/// Remember an abrupt completion of a JS import; `error_completion` rethrows it after wasm unwinds.
fn park(interp: &mut Interpreter, completion: Completion) -> wasmi::Error {
    let pending = match completion {
        Completion::Throw(v) => Pending::Throw(slot_push(interp, v)),
        Completion::Exit(code) => Pending::Exit(code),
        _ => Pending::Exit(1),
    };
    wasm(interp).pending = Some(pending);
    wasmi::Error::new("exception in a JavaScript import")
}

pub(super) fn mk_error(interp: &mut Interpreter, kind: &str, message: &str) -> Completion {
    let factory = slot_get(interp, 0);
    match interp.call_function(
        &factory,
        &JsValue::UNDEFINED,
        &[js_str(kind), js_str(message)],
    ) {
        Completion::Normal(error) => Completion::Throw(error),
        other => other,
    }
}

fn trap_message(code: TrapCode) -> &'static str {
    match code {
        TrapCode::UnreachableCodeReached => "unreachable",
        TrapCode::MemoryOutOfBounds => "memory access out of bounds",
        TrapCode::TableOutOfBounds => "table index is out of bounds",
        TrapCode::IndirectCallToNull | TrapCode::BadSignature => "function signature mismatch",
        TrapCode::IntegerDivisionByZero => "divide by zero",
        TrapCode::IntegerOverflow => "divide result unrepresentable",
        TrapCode::BadConversionToInteger => "float unrepresentable in integer range",
        _ => "wasm trap",
    }
}

/// The completion for a failed wasmi call or instantiation: what a JS import left behind, else a
/// `RangeError` for stack exhaustion, a `RuntimeError` for a trap and a `LinkError` for the rest.
pub(super) fn error_completion(interp: &mut Interpreter, error: wasmi::Error) -> Completion {
    match wasm(interp).pending.take() {
        Some(Pending::Throw(slot)) => {
            let thrown = slot_get(interp, slot);
            slot_set(interp, slot, JsValue::UNDEFINED);
            return Completion::Throw(thrown);
        }
        Some(Pending::Exit(code)) => return Completion::Exit(code),
        None => {}
    }
    match error.as_trap_code() {
        Some(TrapCode::StackOverflow) => {
            mk_error(interp, "range", "Maximum call stack size exceeded")
        }
        Some(code) => mk_error(interp, "runtime", trap_message(code)),
        None => mk_error(interp, "link", &error.to_string()),
    }
}

fn big_i64(v: &JsValue) -> Option<i64> {
    v.with_bigint(|b| i64::try_from(b).ok()).flatten()
}

/// JS to wasm value of type `ty`. The shim has already coerced numbers and BigInts, so a mismatch is
/// a TypeError. References are `null`/function id for funcref, `null`/anything for externref.
pub(super) fn to_val(
    interp: &mut Interpreter,
    ty: wasmi::ValType,
    v: &JsValue,
) -> Result<Val, JsValue> {
    use wasmi::ValType as T;
    let bad = |interp: &mut Interpreter| interp.create_type_error("WebAssembly: invalid value");
    Ok(match ty {
        T::I32 => Val::I32(v.as_number().ok_or_else(|| bad(interp))? as i32),
        T::I64 => Val::I64(big_i64(v).ok_or_else(|| bad(interp))?),
        T::F32 => Val::F32((v.as_number().ok_or_else(|| bad(interp))? as f32).into()),
        T::F64 => Val::F64(v.as_number().ok_or_else(|| bad(interp))?.into()),
        T::FuncRef => {
            if v.is_null() {
                Val::FuncRef(Nullable::Null)
            } else {
                let id = v.as_number().ok_or_else(|| bad(interp))? as u32;
                let func = wasm(interp).funcs.get(id).ok_or_else(|| bad(interp))?;
                Val::FuncRef(Nullable::Val(func))
            }
        }
        T::ExternRef => {
            if v.is_null() {
                Val::ExternRef(Nullable::Null)
            } else {
                let slot = slot_push(interp, v.clone());
                let reference = with_ctx(interp, |c| ExternRef::new(&mut *c, slot));
                Val::ExternRef(Nullable::Val(reference))
            }
        }
        T::V128 => return Err(interp.create_type_error("WebAssembly: v128 is not supported")),
    })
}

pub(super) fn from_val(interp: &mut Interpreter, val: &Val) -> JsValue {
    match val {
        Val::I32(n) => JsValue::number(f64::from(*n)),
        Val::I64(n) => JsValue::bigint(JsBigInt::new(num_bigint::BigInt::from(*n))),
        Val::F32(n) => JsValue::number(f64::from(n.to_float())),
        Val::F64(n) => JsValue::number(n.to_float()),
        Val::FuncRef(Nullable::Null) | Val::ExternRef(Nullable::Null) => JsValue::NULL,
        Val::FuncRef(Nullable::Val(f)) => JsValue::number(f64::from(wasm(interp).funcs.intern(*f))),
        Val::ExternRef(Nullable::Val(e)) => {
            let slot = with_ctx(interp, |c| {
                e.data(c.as_context()).downcast_ref::<usize>().copied()
            });
            slot.map_or(JsValue::UNDEFINED, |s| slot_get(interp, s))
        }
        _ => JsValue::UNDEFINED,
    }
}

pub(super) fn ref_to_js(interp: &mut Interpreter, r: Ref) -> JsValue {
    match r {
        Ref::Func(f) => from_val(interp, &Val::FuncRef(f)),
        Ref::Extern(e) => from_val(interp, &Val::ExternRef(e)),
    }
}

pub(super) fn val_to_ref(val: Val) -> Option<Ref> {
    match val {
        Val::FuncRef(f) => Some(Ref::Func(f)),
        Val::ExternRef(e) => Some(Ref::Extern(e)),
        _ => None,
    }
}

pub(super) fn func_sig(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let Some(func) = wasm(interp).funcs.get(id_arg(args, 0)) else {
        return Completion::Throw(interp.create_type_error("WebAssembly: invalid function"));
    };
    let ty = with_ctx(interp, |c| func.ty(c.as_context()));
    Completion::Normal(js_str(&signature(&ty)))
}

/// `call(funcId, [args])`: the results as an array.
pub(super) fn call(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let Some(func) = wasm(interp).funcs.get(id_arg(args, 0)) else {
        return Completion::Throw(interp.create_type_error("WebAssembly: invalid function"));
    };
    let ty = with_ctx(interp, |c| func.ty(c.as_context()));
    let given = array_items(interp, arg(args, 1));
    let mut params = Vec::with_capacity(ty.params().len());
    for (i, t) in ty.params().iter().enumerate() {
        match to_val(interp, *t, given.get(i).unwrap_or(JsValue::undefined_ref())) {
            Ok(v) => params.push(v),
            Err(e) => return Completion::Throw(e),
        }
    }
    let mut results: Vec<Val> = ty
        .results()
        .iter()
        .map(|t| Val::default_for_ty(*t))
        .collect();
    if wasm(interp).depth >= MAX_DEPTH {
        return mk_error(interp, "range", "Maximum call stack size exceeded");
    }
    wasm(interp).depth += 1;
    let outcome = with_ctx(interp, |c| func.call(&mut *c, &params, &mut results));
    wasm(interp).depth -= 1;
    refresh_buffers(interp);
    match outcome {
        Ok(()) => {
            let items = results.iter().map(|v| from_val(interp, v)).collect();
            Completion::Normal(interp.create_array(items))
        }
        Err(e) => error_completion(interp, e),
    }
}

/// An `ArrayBuffer` whose bytes are the `len` bytes at `ptr`; see `ForeignBytes`.
fn foreign_buffer(interp: &mut Interpreter, ptr: *mut u8, len: usize) -> JsValue {
    // SAFETY: `refresh_buffers` detaches this buffer before the memory it covers can move or die.
    let bytes = unsafe { ForeignBytes::new(ptr, len) };
    let id = interp.create_object_id();
    let proto = interp.realm().arraybuffer_prototype;
    let mut ab = interp.get_object_cell_expect(id).borrow_mut();
    ab.class_name = "ArrayBuffer";
    ab.set_proto(proto);
    ab.kind = ObjectKind::ArrayBuffer(Box::new(ArrayBufferData {
        data: Rc::new(RefCell::new(BufferData::Foreign(bytes))),
        detached: Some(Rc::new(Cell::new(false))),
        max_byte_length: None,
        is_shared: false,
        is_immutable: false,
        sab_shared: None,
    }));
    drop(ab);
    JsValue::object(id)
}

fn detach_value(interp: &mut Interpreter, buffer: &JsValue) {
    let Some(cell) = buffer
        .as_object_id()
        .and_then(|id| interp.get_object_cell(id))
    else {
        return;
    };
    let mut obj = cell.borrow_mut();
    if let Some(ab) = obj.arraybuffer_mut() {
        if let Some(detached) = &ab.detached {
            detached.set(true);
        }
        let old = std::mem::replace(
            &mut ab.data,
            Rc::new(RefCell::new(BufferData::Owned(Vec::new()))),
        );
        *old.borrow_mut() = BufferData::Owned(Vec::new());
    }
}

/// Detach the buffer handed out for memory `mem`, if any (a grow, or `Memory.prototype.grow`).
pub(super) fn detach_buffer(interp: &mut Interpreter, mem: u32) {
    let Some(entry) = wasm(interp).mem_bufs.remove(&mem) else {
        return;
    };
    let buffer = slot_get(interp, entry.slot);
    slot_set(interp, entry.slot, JsValue::UNDEFINED);
    detach_value(interp, &buffer);
}

fn memory_window(interp: &mut Interpreter, mem: u32) -> Option<(*mut u8, usize)> {
    let memory = wasm(interp).mems.get(mem)?;
    Some(with_ctx(interp, |c| {
        let ctx = c.as_context();
        (memory.data_ptr(ctx), memory.data_size(ctx))
    }))
}

/// Detach every handed-out buffer whose memory moved or changed size. Called whenever control goes
/// from wasm back to JS, so no script observes a buffer over memory that has been reallocated.
pub(super) fn refresh_buffers(interp: &mut Interpreter) {
    if wasm(interp).mem_bufs.is_empty() {
        return;
    }
    let ids: Vec<u32> = wasm(interp).mem_bufs.keys().copied().collect();
    for id in ids {
        let Some((ptr, len)) = memory_window(interp, id) else {
            continue;
        };
        let stale = wasm(interp)
            .mem_bufs
            .get(&id)
            .is_some_and(|b| b.ptr != ptr || b.len != len);
        if stale {
            detach_buffer(interp, id);
        }
    }
}

/// The current `ArrayBuffer` of memory `mem`: the same object until the memory grows.
pub(super) fn memory_buffer(interp: &mut Interpreter, mem: u32) -> Option<JsValue> {
    refresh_buffers(interp);
    if let Some(entry) = wasm(interp).mem_bufs.get(&mem) {
        let slot = entry.slot;
        return Some(slot_get(interp, slot));
    }
    let (ptr, len) = memory_window(interp, mem)?;
    let buffer = foreign_buffer(interp, ptr, len);
    let slot = slot_push(interp, buffer.clone());
    wasm(interp).mem_bufs.insert(mem, MemBuf { slot, ptr, len });
    Some(buffer)
}
