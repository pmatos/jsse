//! The `WebAssembly` binding for Node host mode (issue #1378): a hidden `__host_wasm` global that
//! `scripts/node-wasm-shim.js` captures, deleting the global, and wraps in the JS API (`Module`,
//! `Instance`, `Memory`, `Table`, `Global`, `instantiate`, ...). Execution is the pure-Rust `wasmi`
//! interpreter; everything here is integer handles into registries on `WasmState`. See
//! `docs/adr/2026-10-28-1200-node-host-webassembly.md`.
//!
//! This file holds the state and registries; the natives are in `node_host_wasm_module` (modules and
//! instances), `node_host_wasm_objects` (memories, tables, globals) and `node_host_wasm_run` (calls,
//! imports, buffer aliasing, errors).

use super::super::gc::RootedSlots;
use super::super::*;
use super::node_host_wasm_module as module_api;
use super::node_host_wasm_objects as objects_api;
use super::node_host_wasm_run as run_api;
use std::collections::HashMap;
use wasmi::{Caller, Engine, Func, Global, Instance, Memory, Module, Store, Table, ValType};

pub(super) type Native = fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion;

/// Host data of the wasmi store: the interpreter that wasm-to-JS imports call back into. Set
/// whenever the store is taken out of `WasmState` for a call and only dereferenced while that call is
/// on the native stack (see `node_host_wasm_run::with_ctx`).
pub(super) struct Host {
    pub(super) interp: *mut Interpreter,
}

/// Handles of one kind. wasmi's handles have no equality, so identity is their `Debug` text (store id
/// plus arena index); a function exported twice, or taken from a table, keeps one id and one JS wrapper.
pub(super) struct Registry<T> {
    items: Vec<T>,
    index: HashMap<String, u32>,
}

impl<T> Default for Registry<T> {
    fn default() -> Self {
        Registry {
            items: Vec::new(),
            index: HashMap::new(),
        }
    }
}

impl<T: Copy + std::fmt::Debug> Registry<T> {
    pub(super) fn intern(&mut self, item: T) -> u32 {
        let key = format!("{item:?}");
        if let Some(&id) = self.index.get(&key) {
            return id;
        }
        let id = self.items.len() as u32;
        self.items.push(item);
        self.index.insert(key, id);
        id
    }

    pub(super) fn get(&self, id: u32) -> Option<T> {
        self.items.get(id as usize).copied()
    }
}

pub(super) struct ModuleEntry {
    pub(super) module: Module,
    pub(super) bytes: Vec<u8>,
}

/// The `ArrayBuffer` currently handed out as a memory's `buffer`, the window it covers and the slot
/// that keeps it alive.
pub(super) struct MemBuf {
    pub(super) slot: usize,
    pub(super) ptr: *mut u8,
    pub(super) len: usize,
}

/// What a JS import left behind when it did not return normally.
pub(super) enum Pending {
    Throw(usize),
    Exit(i32),
}

pub(crate) struct WasmState {
    pub(super) engine: Engine,
    /// `None` while a call is on the native stack: the import trampolines then reach the store
    /// through `caller`.
    pub(super) store: Option<Store<Host>>,
    pub(super) caller: Option<*mut Caller<'static, Host>>,
    pub(super) depth: u32,
    pub(super) modules: Vec<ModuleEntry>,
    pub(super) instances: Vec<Instance>,
    pub(super) funcs: Registry<Func>,
    pub(super) mems: Registry<Memory>,
    pub(super) tables: Registry<Table>,
    pub(super) globals: Registry<Global>,
    /// GC-visible values: slot 0 is the JS error factory, then import callables, externref payloads,
    /// memory buffers and a pending throw. Pinned on the binding object, never shrunk.
    pub(super) slots: RootedSlots,
    pub(super) mem_bufs: HashMap<u32, MemBuf>,
    pub(super) pending: Option<Pending>,
}

#[derive(Default)]
pub(crate) struct WasmSlot(Option<Box<WasmState>>);

pub(super) fn wasm(interp: &mut Interpreter) -> &mut WasmState {
    interp
        .node_host
        .wasm
        .0
        .as_deref_mut()
        .expect("__host_wasm.init has not run")
}

pub(super) fn arg(args: &[JsValue], i: usize) -> &JsValue {
    args.get(i).unwrap_or(JsValue::undefined_ref())
}

pub(super) fn js_str(s: &str) -> JsValue {
    JsValue::string(JsString::from_str(s))
}

pub(super) fn num_arg(args: &[JsValue], i: usize) -> f64 {
    arg(args, i).as_number().unwrap_or(0.0)
}

pub(super) fn id_arg(args: &[JsValue], i: usize) -> u32 {
    num_arg(args, i) as u32
}

pub(super) fn slot_push(interp: &mut Interpreter, value: JsValue) -> usize {
    let slots = wasm(interp).slots.clone();
    let index = slots.len(interp);
    slots.push(interp, value);
    index
}

pub(super) fn slot_get(interp: &mut Interpreter, index: usize) -> JsValue {
    let slots = wasm(interp).slots.clone();
    slots.get(interp, index)
}

pub(super) fn slot_set(interp: &mut Interpreter, index: usize, value: JsValue) {
    let slots = wasm(interp).slots.clone();
    slots.set(interp, index, value);
}

pub(super) fn array_items(interp: &Interpreter, v: &JsValue) -> Vec<JsValue> {
    if v.as_object_id()
        .and_then(|id| interp.get_object_cell(id))
        .is_some_and(|c| c.borrow().array_elements().is_some())
    {
        interp.with_array_elements(v, |e| e.to_vec())
    } else {
        Vec::new()
    }
}

pub(super) fn valtype_char(t: ValType) -> char {
    match t {
        ValType::I32 => 'i',
        ValType::I64 => 'l',
        ValType::F32 => 'f',
        ValType::F64 => 'd',
        ValType::V128 => 'v',
        ValType::FuncRef => 'r',
        ValType::ExternRef => 'e',
    }
}

pub(super) fn valtype_of(c: char) -> Option<ValType> {
    Some(match c {
        'i' => ValType::I32,
        'l' => ValType::I64,
        'f' => ValType::F32,
        'd' => ValType::F64,
        'r' => ValType::FuncRef,
        'e' => ValType::ExternRef,
        _ => return None,
    })
}

pub(super) fn signature(ty: &wasmi::FuncType) -> String {
    let mut s: String = ty.params().iter().map(|t| valtype_char(*t)).collect();
    s.push('>');
    s.extend(ty.results().iter().map(|t| valtype_char(*t)));
    s
}

/// `init(mkError)`: create the engine and store, pinning the slots on the binding (`this`).
fn init(interp: &mut Interpreter, this: &JsValue, args: &[JsValue]) -> Completion {
    if interp.node_host.wasm.0.is_some() {
        return Completion::Normal(JsValue::UNDEFINED);
    }
    let engine = Engine::default();
    let store = Store::new(
        &engine,
        Host {
            interp: std::ptr::null_mut(),
        },
    );
    let slots = RootedSlots::new(interp, 0);
    slots.pin_on(interp, this);
    slots.push(interp, arg(args, 0).clone());
    interp.node_host.wasm.0 = Some(Box::new(WasmState {
        engine,
        store: Some(store),
        caller: None,
        depth: 0,
        modules: Vec::new(),
        instances: Vec::new(),
        funcs: Registry::default(),
        mems: Registry::default(),
        tables: Registry::default(),
        globals: Registry::default(),
        slots,
        mem_bufs: HashMap::new(),
        pending: None,
    }));
    Completion::Normal(JsValue::UNDEFINED)
}

impl Interpreter {
    pub(super) fn setup_node_host_wasm(&mut self) {
        let binding = self.create_object_id();
        let functions: [(&str, usize, Native); 23] = [
            ("init", 1, init),
            ("validate", 1, module_api::validate),
            ("compile", 1, module_api::compile),
            ("moduleImports", 1, module_api::module_imports),
            ("moduleExports", 1, module_api::module_exports),
            ("customSections", 2, module_api::custom_sections),
            ("instantiate", 2, module_api::instantiate),
            ("instanceExports", 2, module_api::instance_exports),
            ("funcSig", 1, run_api::func_sig),
            ("call", 2, run_api::call),
            ("memNew", 2, objects_api::mem_new),
            ("memBuffer", 1, objects_api::mem_buffer),
            ("memGrow", 2, objects_api::mem_grow),
            ("tableNew", 4, objects_api::table_new),
            ("tableGet", 2, objects_api::table_get),
            ("tableSet", 3, objects_api::table_set),
            ("tableGrow", 3, objects_api::table_grow),
            ("tableLen", 1, objects_api::table_len),
            ("tableType", 1, objects_api::table_type),
            ("globalNew", 3, objects_api::global_new),
            ("globalGet", 1, objects_api::global_get),
            ("globalSet", 2, objects_api::global_set),
            ("globalType", 1, objects_api::global_type),
        ];
        for (name, arity, native) in functions {
            let f = self.create_function(JsFunction::native(String::new(), arity, native));
            self.get_object_cell_expect(binding)
                .borrow_mut()
                .insert_value(name, f);
        }
        self.install_host_global("__host_wasm", JsValue::object(binding));
    }
}
