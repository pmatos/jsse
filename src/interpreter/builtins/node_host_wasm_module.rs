//! Modules and instances for `__host_wasm` (issue #1378): compile and validate, import and export
//! descriptions, custom sections, and instantiation against already-resolved externs.

use super::super::*;
use super::node_host_crypto::window;
use super::node_host_wasm::*;
use super::node_host_wasm_run::{
    error_completion, make_host_func, mk_error, refresh_buffers, with_ctx,
};
use std::collections::VecDeque;
use wasmi::{AsContext, Extern, ExternType, Instance, Module};

fn read_bytes(interp: &Interpreter, v: &JsValue) -> Option<Vec<u8>> {
    let w = window(interp, v)?;
    w.buffer
        .borrow()
        .with_read(|b| b.get(w.offset..w.offset + w.len).map(<[u8]>::to_vec))
}

fn leb_u64(bytes: &[u8], pos: &mut usize) -> Option<u64> {
    let mut result = 0u64;
    for shift in (0..70).step_by(7) {
        let byte = *bytes.get(*pos)?;
        *pos += 1;
        result |= u64::from(byte & 0x7f).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some(result);
        }
    }
    None
}

fn leb(bytes: &[u8], pos: &mut usize) -> Option<u32> {
    u32::try_from(leb_u64(bytes, pos)?).ok()
}

/// `(id, payload)` of every section of a module the engine has already validated.
fn sections(bytes: &[u8]) -> Vec<(u8, &[u8])> {
    let mut out = Vec::new();
    let mut pos = 8;
    while let Some(&id) = bytes.get(pos) {
        pos += 1;
        let Some(size) = leb(bytes, &mut pos) else {
            break;
        };
        let Some(payload) = bytes.get(pos..pos + size as usize) else {
            break;
        };
        out.push((id, payload));
        pos += size as usize;
    }
    out
}

fn name_at<'a>(bytes: &'a [u8], pos: &mut usize) -> Option<&'a [u8]> {
    let len = leb(bytes, pos)? as usize;
    let name = bytes.get(*pos..*pos + len)?;
    *pos += len;
    Some(name)
}

/// `(name, kind, index)` from the export section, in module order.
fn export_list(bytes: &[u8]) -> Vec<(String, &'static str, u32)> {
    let mut out = Vec::new();
    for (id, payload) in sections(bytes) {
        if id != 7 {
            continue;
        }
        let mut pos = 0;
        let count = leb(payload, &mut pos).unwrap_or(0);
        for _ in 0..count {
            let Some(name) = name_at(payload, &mut pos) else {
                break;
            };
            let kind = payload.get(pos).copied().unwrap_or(0);
            pos += 1;
            let Some(index) = leb(payload, &mut pos) else {
                break;
            };
            let kind = match kind {
                0 => "function",
                1 => "table",
                2 => "memory",
                3 => "global",
                _ => "tag",
            };
            out.push((String::from_utf8_lossy(name).into_owned(), kind, index));
        }
    }
    out
}

fn skip_limits(payload: &[u8], pos: &mut usize) -> Option<()> {
    let flags = *payload.get(*pos)?;
    *pos += 1;
    leb_u64(payload, pos)?;
    if flags & 1 != 0 {
        leb_u64(payload, pos)?;
    }
    if flags & 8 != 0 {
        leb_u64(payload, pos)?;
    }
    Some(())
}

/// `(module, name, kind)` of every import in declaration order; kind 0 function, 1 table, 2 memory,
/// 3 global.
fn import_names(bytes: &[u8]) -> Vec<(String, String, u8)> {
    let mut out = Vec::new();
    for (id, payload) in sections(bytes) {
        if id != 2 {
            continue;
        }
        let mut pos = 0;
        for _ in 0..leb(payload, &mut pos).unwrap_or(0) {
            let (Some(module), Some(field)) =
                (name_at(payload, &mut pos), name_at(payload, &mut pos))
            else {
                return out;
            };
            let kind = payload.get(pos).copied().unwrap_or(9);
            pos += 1;
            let skipped = match kind {
                0 => leb(payload, &mut pos).map(drop),
                1 => {
                    pos += 1;
                    skip_limits(payload, &mut pos)
                }
                2 => skip_limits(payload, &mut pos),
                3 => {
                    pos += 2;
                    Some(())
                }
                _ => None,
            };
            if skipped.is_none() {
                return out;
            }
            out.push((
                String::from_utf8_lossy(module).into_owned(),
                String::from_utf8_lossy(field).into_owned(),
                kind,
            ));
        }
    }
    out
}

/// An import as the module declares it, with its position and type in wasmi's own import order
/// (functions, then tables, memories, globals), which is the order `Instance::new` takes externs in.
struct DeclImport {
    module: String,
    name: String,
    wasmi_pos: usize,
    ty: ExternType,
}

fn import_decls(bytes: &[u8], module: &Module) -> Vec<DeclImport> {
    let mut queues: [VecDeque<(usize, ExternType)>; 4] = Default::default();
    for (pos, import) in module.imports().enumerate() {
        let ty = import.ty().clone();
        let kind = match ty {
            ExternType::Func(_) => 0,
            ExternType::Table(_) => 1,
            ExternType::Memory(_) => 2,
            ExternType::Global(_) => 3,
        };
        queues[kind].push_back((pos, ty));
    }
    import_names(bytes)
        .into_iter()
        .filter_map(|(module, name, kind)| {
            let (wasmi_pos, ty) = queues.get_mut(usize::from(kind))?.pop_front()?;
            Some(DeclImport {
                module,
                name,
                wasmi_pos,
                ty,
            })
        })
        .collect()
}

fn module_entry(interp: &mut Interpreter, id: u32) -> Option<(Module, Vec<u8>)> {
    wasm(interp)
        .modules
        .get(id as usize)
        .map(|m| (m.module.clone(), m.bytes.clone()))
}

fn module_bytes(interp: &mut Interpreter, id: u32) -> Option<Vec<u8>> {
    wasm(interp)
        .modules
        .get(id as usize)
        .map(|m| m.bytes.clone())
}

fn type_error(interp: &mut Interpreter, message: &str) -> Completion {
    Completion::Throw(interp.create_type_error(message))
}

pub(super) fn validate(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let Some(bytes) = read_bytes(interp, arg(args, 0)) else {
        return type_error(
            interp,
            "WebAssembly.validate(): Argument 0 must be a buffer source",
        );
    };
    let ok = Module::validate(&wasm(interp).engine, &bytes).is_ok();
    Completion::Normal(JsValue::boolean(ok))
}

pub(super) fn compile(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let Some(bytes) = read_bytes(interp, arg(args, 0)) else {
        return type_error(interp, "Argument 0 must be a buffer source");
    };
    match Module::new(&wasm(interp).engine, &bytes[..]) {
        Ok(module) => {
            let w = wasm(interp);
            w.modules.push(ModuleEntry { module, bytes });
            Completion::Normal(JsValue::number((w.modules.len() - 1) as f64))
        }
        Err(e) => mk_error(interp, "compile", &e.to_string()),
    }
}

fn desc(strings: &[&str], numbers: [Option<u64>; 2]) -> Vec<JsValue> {
    let mut out: Vec<JsValue> = strings.iter().map(|s| js_str(s)).collect();
    out.extend(numbers.map(|n| n.map_or(JsValue::UNDEFINED, |n| JsValue::number(n as f64))));
    out
}

/// `[module, name, kind, typeText, min, max]` per import: `typeText` is the signature of a function,
/// the value type of a global or the element type of a table; `min` of a global is 1 when mutable.
pub(super) fn module_imports(
    interp: &mut Interpreter,
    _this: &JsValue,
    args: &[JsValue],
) -> Completion {
    let Some((module, bytes)) = module_entry(interp, id_arg(args, 0)) else {
        return type_error(interp, "WebAssembly: invalid module");
    };
    let mut rows = Vec::new();
    for import in import_decls(&bytes, &module) {
        let (kind, text, min, max) = match &import.ty {
            ExternType::Func(f) => ("function", signature(f), None, None),
            ExternType::Global(g) => (
                "global",
                valtype_char(g.content()).to_string(),
                Some(u64::from(g.mutability().is_mut())),
                None,
            ),
            ExternType::Memory(m) => ("memory", String::new(), Some(m.minimum()), m.maximum()),
            ExternType::Table(t) => (
                "table",
                valtype_char(t.element().into()).to_string(),
                Some(t.minimum()),
                t.maximum(),
            ),
        };
        let mut row = vec![js_str(&import.module), js_str(&import.name), js_str(kind)];
        row.extend(desc(&[&text], [min, max]));
        rows.push(interp.create_array(row));
    }
    Completion::Normal(interp.create_array(rows))
}

/// `[name, kind, index]` per export, in module order.
pub(super) fn module_exports(
    interp: &mut Interpreter,
    _this: &JsValue,
    args: &[JsValue],
) -> Completion {
    let Some(bytes) = module_bytes(interp, id_arg(args, 0)) else {
        return type_error(interp, "WebAssembly: invalid module");
    };
    let mut rows = Vec::new();
    for (name, kind, index) in export_list(&bytes) {
        let row = vec![
            js_str(&name),
            js_str(kind),
            JsValue::number(f64::from(index)),
        ];
        rows.push(interp.create_array(row));
    }
    Completion::Normal(interp.create_array(rows))
}

pub(super) fn custom_sections(
    interp: &mut Interpreter,
    _this: &JsValue,
    args: &[JsValue],
) -> Completion {
    let Some(bytes) = module_bytes(interp, id_arg(args, 0)) else {
        return type_error(interp, "WebAssembly: invalid module");
    };
    let wanted = match interp.to_string_lossy(arg(args, 1)) {
        Ok(s) => s,
        Err(e) => return Completion::Throw(e),
    };
    let mut buffers = Vec::new();
    for (id, payload) in sections(&bytes) {
        if id != 0 {
            continue;
        }
        let mut pos = 0;
        let Some(name) = name_at(payload, &mut pos) else {
            continue;
        };
        if String::from_utf8_lossy(name) == wanted {
            let id = interp.create_arraybuffer_resizable(payload[pos..].to_vec(), None);
            buffers.push(JsValue::object(id));
        }
    }
    Completion::Normal(interp.create_array(buffers))
}

enum Import {
    Js(usize),
    Extern(Extern),
}

/// `instantiate(moduleId, externs)`: `externs[i]` is `["js", callable]` or `[kind, handle]` for the
/// i-th import in declaration order. Returns the instance id.
pub(super) fn instantiate(
    interp: &mut Interpreter,
    _this: &JsValue,
    args: &[JsValue],
) -> Completion {
    let Some((module, bytes)) = module_entry(interp, id_arg(args, 0)) else {
        return type_error(interp, "WebAssembly: invalid module");
    };
    let externs = array_items(interp, arg(args, 1));
    let decls = import_decls(&bytes, &module);
    let mut plan = Vec::with_capacity(decls.len());
    for index in 0..decls.len() {
        let entry = array_items(
            interp,
            externs.get(index).unwrap_or(JsValue::undefined_ref()),
        );
        let kind = match interp.to_string_lossy(arg(&entry, 0)) {
            Ok(k) => k,
            Err(e) => return Completion::Throw(e),
        };
        let handle = arg(&entry, 1);
        let id = handle.as_number().unwrap_or(0.0) as u32;
        let w = wasm(interp);
        let resolved = match kind.as_str() {
            "function" => w.funcs.get(id).map(Extern::Func),
            "memory" => w.mems.get(id).map(Extern::Memory),
            "table" => w.tables.get(id).map(Extern::Table),
            "global" => w.globals.get(id).map(Extern::Global),
            _ => None,
        };
        plan.push(match (kind.as_str(), resolved) {
            ("js", _) => Import::Js(slot_push(interp, handle.clone())),
            (_, Some(e)) => Import::Extern(e),
            _ => return mk_error(interp, "link", "invalid import"),
        });
    }
    let outcome = with_ctx(interp, |ctx| {
        let mut resolved: Vec<Option<Extern>> = vec![None; decls.len()];
        for (item, decl) in plan.iter().zip(&decls) {
            resolved[decl.wasmi_pos] = Some(match (item, &decl.ty) {
                (Import::Js(slot), ExternType::Func(f)) => {
                    Extern::Func(make_host_func(ctx, f.clone(), *slot))
                }
                (Import::Extern(e), _) => *e,
                _ => return Err(wasmi::Error::new("import is not a function")),
            });
        }
        let ordered: Vec<Extern> = resolved.into_iter().flatten().collect();
        Instance::new(&mut *ctx, &module, &ordered)
    });
    refresh_buffers(interp);
    match outcome {
        Ok(instance) => {
            let w = wasm(interp);
            w.instances.push(instance);
            Completion::Normal(JsValue::number((w.instances.len() - 1) as f64))
        }
        Err(e) => error_completion(interp, e),
    }
}

/// `instanceExports(instanceId, moduleId)`: `[kind, handle]` per export, in module order.
pub(super) fn instance_exports(
    interp: &mut Interpreter,
    _this: &JsValue,
    args: &[JsValue],
) -> Completion {
    let (Some(instance), Some(bytes)) = (
        wasm(interp)
            .instances
            .get(id_arg(args, 0) as usize)
            .copied(),
        module_bytes(interp, id_arg(args, 1)),
    ) else {
        return type_error(interp, "WebAssembly: invalid instance");
    };
    let mut rows = Vec::new();
    for (name, _, _) in export_list(&bytes) {
        let found = with_ctx(interp, |c| instance.get_export(c.as_context(), &name));
        let w = wasm(interp);
        let (kind, id) = match found {
            Some(Extern::Func(f)) => ("function", w.funcs.intern(f)),
            Some(Extern::Memory(m)) => ("memory", w.mems.intern(m)),
            Some(Extern::Table(t)) => ("table", w.tables.intern(t)),
            Some(Extern::Global(g)) => ("global", w.globals.intern(g)),
            None => ("tag", 0),
        };
        let row = vec![js_str(kind), JsValue::number(f64::from(id))];
        rows.push(interp.create_array(row));
    }
    Completion::Normal(interp.create_array(rows))
}
