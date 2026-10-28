//! Memories, tables and globals for `__host_wasm` (issue #1378). Values cross as the shim coerced
//! them (see `node_host_wasm_run::to_val`); references are a function id or `null` for funcref and any
//! JS value for externref.

use super::super::*;
use super::node_host_wasm::*;
use super::node_host_wasm_run::{
    detach_buffer, memory_buffer, mk_error, ref_to_js, to_val, val_to_ref, with_ctx,
};
use wasmi::{
    AsContext, Global, Memory, MemoryType, Mutability, RefType, Table, TableType, Val, ValType,
};

fn invalid(interp: &mut Interpreter) -> Completion {
    Completion::Throw(interp.create_type_error("WebAssembly: invalid object"))
}

fn optional_u32(args: &[JsValue], i: usize) -> Option<u32> {
    arg(args, i).as_number().map(|n| n as u32)
}

pub(super) fn mem_new(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let ty = MemoryType::new(id_arg(args, 0), optional_u32(args, 1));
    match with_ctx(interp, |c| Memory::new(&mut *c, ty)) {
        Ok(memory) => {
            let id = wasm(interp).mems.intern(memory);
            Completion::Normal(JsValue::number(f64::from(id)))
        }
        Err(e) => mk_error(interp, "range", &e.to_string()),
    }
}

pub(super) fn mem_buffer(
    interp: &mut Interpreter,
    _this: &JsValue,
    args: &[JsValue],
) -> Completion {
    match memory_buffer(interp, id_arg(args, 0)) {
        Some(buffer) => Completion::Normal(buffer),
        None => invalid(interp),
    }
}

/// `memGrow(id, delta)`: the previous size in pages. A successful grow always replaces the buffer, as
/// the JS API does; a failed one leaves it alone.
pub(super) fn mem_grow(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let id = id_arg(args, 0);
    let Some(memory) = wasm(interp).mems.get(id) else {
        return invalid(interp);
    };
    let outcome = with_ctx(interp, |c| memory.grow(&mut *c, u64::from(id_arg(args, 1))));
    if outcome.is_ok() {
        detach_buffer(interp, id);
    }
    match outcome {
        Ok(previous) => Completion::Normal(JsValue::number(previous as f64)),
        Err(_) => mk_error(
            interp,
            "range",
            "WebAssembly.Memory.grow(): Maximum memory size exceeded",
        ),
    }
}

fn ref_type(c: &JsValue) -> Option<(RefType, ValType)> {
    match c.with_string(|s| s.first().copied()).flatten() {
        Some(114) => Some((RefType::Func, ValType::FuncRef)),
        Some(101) => Some((RefType::Extern, ValType::ExternRef)),
        _ => None,
    }
}

fn element_val(interp: &mut Interpreter, table: Table, v: &JsValue) -> Result<wasmi::Ref, JsValue> {
    let element = with_ctx(interp, |c| table.ty(c.as_context()).element());
    let ty = if element == RefType::Func {
        ValType::FuncRef
    } else {
        ValType::ExternRef
    };
    let val = to_val(interp, ty, v)?;
    val_to_ref(val).ok_or_else(|| interp.create_type_error("WebAssembly: invalid reference"))
}

/// `tableNew("r" | "e", initial, maximum, init)`.
pub(super) fn table_new(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let Some((element, ty)) = ref_type(arg(args, 0)) else {
        return invalid(interp);
    };
    let init = match to_val(interp, ty, arg(args, 3)).map(val_to_ref) {
        Ok(Some(r)) => r,
        Ok(None) => return invalid(interp),
        Err(e) => return Completion::Throw(e),
    };
    let table_ty = TableType::new(element, id_arg(args, 1), optional_u32(args, 2));
    match with_ctx(interp, |c| Table::new(&mut *c, table_ty, init)) {
        Ok(table) => {
            let id = wasm(interp).tables.intern(table);
            Completion::Normal(JsValue::number(f64::from(id)))
        }
        Err(e) => mk_error(interp, "range", &e.to_string()),
    }
}

pub(super) fn table_get(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let Some(table) = wasm(interp).tables.get(id_arg(args, 0)) else {
        return invalid(interp);
    };
    let index = num_arg(args, 1) as u64;
    match with_ctx(interp, |c| table.get(c.as_context(), index)) {
        Some(r) => Completion::Normal(ref_to_js(interp, r)),
        None => mk_error(interp, "range", "WebAssembly.Table.get(): invalid address"),
    }
}

pub(super) fn table_set(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let Some(table) = wasm(interp).tables.get(id_arg(args, 0)) else {
        return invalid(interp);
    };
    let value = match element_val(interp, table, arg(args, 2)) {
        Ok(v) => v,
        Err(e) => return Completion::Throw(e),
    };
    let index = num_arg(args, 1) as u64;
    match with_ctx(interp, |c| table.set(&mut *c, index, value)) {
        Ok(()) => Completion::Normal(JsValue::UNDEFINED),
        Err(_) => mk_error(interp, "range", "WebAssembly.Table.set(): invalid address"),
    }
}

pub(super) fn table_grow(
    interp: &mut Interpreter,
    _this: &JsValue,
    args: &[JsValue],
) -> Completion {
    let Some(table) = wasm(interp).tables.get(id_arg(args, 0)) else {
        return invalid(interp);
    };
    let init = match element_val(interp, table, arg(args, 2)) {
        Ok(v) => v,
        Err(e) => return Completion::Throw(e),
    };
    let delta = u64::from(id_arg(args, 1));
    match with_ctx(interp, |c| table.grow(&mut *c, delta, init)) {
        Ok(previous) => Completion::Normal(JsValue::number(previous as f64)),
        Err(_) => mk_error(
            interp,
            "range",
            &format!(
                "WebAssembly.Table.grow(): failed to grow table by {}",
                id_arg(args, 1)
            ),
        ),
    }
}

pub(super) fn table_len(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let Some(table) = wasm(interp).tables.get(id_arg(args, 0)) else {
        return invalid(interp);
    };
    let size = with_ctx(interp, |c| table.size(c.as_context()));
    Completion::Normal(JsValue::number(size as f64))
}

pub(super) fn table_type(
    interp: &mut Interpreter,
    _this: &JsValue,
    args: &[JsValue],
) -> Completion {
    let Some(table) = wasm(interp).tables.get(id_arg(args, 0)) else {
        return invalid(interp);
    };
    let element = with_ctx(interp, |c| table.ty(c.as_context()).element());
    Completion::Normal(js_str(if element == RefType::Func { "r" } else { "e" }))
}

/// `globalNew(typeChar, mutable, value)`.
pub(super) fn global_new(
    interp: &mut Interpreter,
    _this: &JsValue,
    args: &[JsValue],
) -> Completion {
    let ty = arg(args, 0)
        .with_string(|s| s.first().copied())
        .flatten()
        .and_then(|c| valtype_of(char::from(c as u8)));
    let Some(ty) = ty else { return invalid(interp) };
    let value = match to_val(interp, ty, arg(args, 2)) {
        Ok(v) => v,
        Err(e) => return Completion::Throw(e),
    };
    let mutability = if arg(args, 1).as_boolean() == Some(true) {
        Mutability::Var
    } else {
        Mutability::Const
    };
    let global = with_ctx(interp, |c| Global::new(&mut *c, value, mutability));
    let id = wasm(interp).globals.intern(global);
    Completion::Normal(JsValue::number(f64::from(id)))
}

pub(super) fn global_get(
    interp: &mut Interpreter,
    _this: &JsValue,
    args: &[JsValue],
) -> Completion {
    let Some(global) = wasm(interp).globals.get(id_arg(args, 0)) else {
        return invalid(interp);
    };
    let value: Val = with_ctx(interp, |c| global.get(c.as_context()));
    Completion::Normal(super::node_host_wasm_run::from_val(interp, &value))
}

pub(super) fn global_set(
    interp: &mut Interpreter,
    _this: &JsValue,
    args: &[JsValue],
) -> Completion {
    let Some(global) = wasm(interp).globals.get(id_arg(args, 0)) else {
        return invalid(interp);
    };
    let ty = with_ctx(interp, |c| global.ty(c.as_context()).content());
    let value = match to_val(interp, ty, arg(args, 1)) {
        Ok(v) => v,
        Err(e) => return Completion::Throw(e),
    };
    match with_ctx(interp, |c| global.set(&mut *c, value)) {
        Ok(()) => Completion::Normal(JsValue::UNDEFINED),
        Err(_) => Completion::Throw(
            interp.create_type_error("Can't set the value of an immutable global."),
        ),
    }
}

/// `[typeChar, mutable]`.
pub(super) fn global_type(
    interp: &mut Interpreter,
    _this: &JsValue,
    args: &[JsValue],
) -> Completion {
    let Some(global) = wasm(interp).globals.get(id_arg(args, 0)) else {
        return invalid(interp);
    };
    let ty = with_ctx(interp, |c| global.ty(c.as_context()));
    let row = vec![
        js_str(&valtype_char(ty.content()).to_string()),
        JsValue::boolean(ty.mutability().is_mut()),
    ];
    Completion::Normal(interp.create_array(row))
}
