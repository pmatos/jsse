//! Platform objects with a transfer protocol in `structuredClone` (#1528).
//!
//! HTML gives platform objects `[[Clone]]`/`[[Transfer]]` steps; Node
//! spells them `markTransferMode(obj, cloneable, transferable)` plus the
//! `kClone`/`kTransfer`/`kTransferList`/`kDeserialize` methods
//! (`lib/internal/worker/js_transferable.js`). The engine owns only the
//! mark and the walk; a JS hook installed by `node-lib/internal/worker/
//! js_transferable.js` calls the methods. The mark is a private element,
//! invisible to scripts, and exists only under `--node`: without it
//! `mode_of` is 0 for every object and nothing here runs.

use super::*;
use crate::interpreter::types::{PrivateElement, PrivateId};

pub(super) const TRANSFERABLE: u8 = 1;
pub(super) const CLONEABLE: u8 = 2;
/// Stored with every mode, so a mark of 0 (not cloneable, not transferable) is
/// not the same as no mark.
const MARKED: u8 = 4;

const OP_TRANSFER_LIST: f64 = 0.0;
const OP_TRANSFER: f64 = 1.0;
const OP_CLONE: f64 = 2.0;
const OP_DESERIALIZE: f64 = 3.0;
const OP_NAME: f64 = 4.0;

#[derive(Default)]
pub(crate) struct PlatformState {
    mark: Option<PrivateId>,
    hook: Option<JsValue>,
}

/// The stored mode of `id`: 0 for an object that was never marked, else the
/// mode with `MARKED` set.
pub(super) fn mode_of(interp: &Interpreter, id: u64) -> u8 {
    let Some(mark) = interp.node_host.platform.mark else {
        return 0;
    };
    interp
        .get_object_cell(id)
        .and_then(|cell| match cell.borrow().private_field(mark) {
            Some(PrivateElement::Field(v)) => v.as_number(),
            _ => None,
        })
        .map_or(0, |n| n as u8)
}

fn call_hook(interp: &mut Interpreter, op: f64, a: &JsValue, b: &JsValue) -> CloneResult {
    let Some(hook) = interp.node_host.platform.hook.clone() else {
        return Err(interp.uncloneable("Object"));
    };
    let args = [JsValue::number(op), a.clone(), b.clone()];
    match interp.call_function(&hook, &JsValue::UNDEFINED, &args) {
        Completion::Normal(v) => {
            interp.gc_root_value(&v);
            Ok(v)
        }
        Completion::Throw(e) => Err(e),
        _ => Ok(JsValue::UNDEFINED),
    }
}

/// The platform objects of a transfer list, in the order they transfer.
#[derive(Default)]
pub(super) struct PlatformTransfer {
    pub(super) order: Vec<u64>,
}

impl PlatformTransfer {
    /// Add `id`; false when it is already listed.
    pub(super) fn admit(&mut self, seen: &mut FxHashSet<u64>, id: u64) -> bool {
        let fresh = seen.insert(id);
        if fresh {
            self.order.push(id);
        }
        fresh
    }

    /// Append what each listed object's `[kTransferList]()` names (a
    /// transformed stream's two sides, their message ports), transitively.
    pub(super) fn expand(
        &mut self,
        interp: &mut Interpreter,
        seen: &mut FxHashSet<u64>,
    ) -> Result<(), JsValue> {
        let mut next = 0;
        while let Some(&id) = self.order.get(next) {
            next += 1;
            let nested = call_hook(
                interp,
                OP_TRANSFER_LIST,
                &JsValue::object(id),
                JsValue::undefined_ref(),
            )?;
            if nested.is_undefined() {
                continue;
            }
            for item in interp.iterate_to_vec(&nested)? {
                interp.gc_root_value(&item);
                let marked = item
                    .as_object_id()
                    .is_some_and(|n| mode_of(interp, n) & TRANSFERABLE != 0);
                let Some(nid) = item.as_object_id().filter(|_| marked) else {
                    return Err(interp.data_clone_error("Found invalid value in transferList."));
                };
                self.admit(seen, nid);
            }
        }
        Ok(())
    }
}

/// The constructor name Node prints in "Transfer list contains duplicate X".
pub(super) fn label_of(interp: &mut Interpreter, id: u64) -> String {
    match call_hook(
        interp,
        OP_NAME,
        &JsValue::object(id),
        JsValue::undefined_ref(),
    ) {
        Ok(v) => v
            .as_string()
            .map_or_else(|| "object".to_string(), |s| s.to_rust_string()),
        Err(_) => "object".to_string(),
    }
}

impl Cloner {
    /// Clone a marked object met in the value: transfer it when it is in the
    /// transfer list, else clone it when it allows that, else refuse.
    pub(super) fn clone_platform(
        &mut self,
        interp: &mut Interpreter,
        id: u64,
        mode: u8,
    ) -> CloneResult {
        let op = if self.transfer.contains(&id) {
            OP_TRANSFER
        } else if mode & CLONEABLE != 0 {
            OP_CLONE
        } else if mode & TRANSFERABLE != 0 {
            return Err(interp.data_clone_error(
                "Object that needs transfer was found in message but not listed in transferList",
            ));
        } else {
            return Err(interp.data_clone_error("Cannot clone object of unsupported type."));
        };
        let source = JsValue::object(id);
        let record = call_hook(interp, op, &source, JsValue::undefined_ref())?;
        let Some(rid) = record.as_object_id() else {
            return Err(interp.uncloneable("Object"));
        };
        let data = get_or_throw(interp, rid, "data", &record)?;
        let info = get_or_throw(interp, rid, "deserializeInfo", &record)?;
        interp.gc_root_value(&data);
        interp.gc_root_value(&info);
        let cloned = self.clone_value(interp, &data)?;
        interp.gc_root_value(&cloned);
        let clone = call_hook(interp, OP_DESERIALIZE, &info, &cloned)?;
        let Some(cid) = clone.as_object_id() else {
            return Err(interp.uncloneable("Object"));
        };
        self.register(interp, id, cid);
        Ok(clone)
    }

    /// Transfer the listed platform objects the value never reached: their
    /// side effects (a stream locks, a port detaches) still happen.
    pub(super) fn transfer_unreferenced(
        &mut self,
        interp: &mut Interpreter,
        list: &PlatformTransfer,
    ) -> Result<(), JsValue> {
        for &id in &list.order {
            if !self.memory.contains_key(&id) {
                let source = JsValue::object(id);
                call_hook(interp, OP_TRANSFER, &source, JsValue::undefined_ref())?;
            }
        }
        Ok(())
    }
}

impl Interpreter {
    /// `__host_transfer_mark(obj [, mode])` reads or sets an object's mode;
    /// `__host_transfer_hook(fn)` installs the JS side, once.
    pub(in crate::interpreter::builtins) fn setup_node_host_transfer(&mut self) {
        let mark = self.create_function(JsFunction::native(
            "__host_transfer_mark".to_string(),
            2,
            |interp, _this, args| {
                let Some(id) = args.first().and_then(JsValue::as_object_id) else {
                    return Completion::Normal(JsValue::number(0.0));
                };
                let Some(mode) = args.get(1).and_then(JsValue::as_number) else {
                    let mode = mode_of(interp, id) & !MARKED;
                    return Completion::Normal(JsValue::number(f64::from(mode)));
                };
                let mark = match interp.node_host.platform.mark {
                    Some(mark) => mark,
                    None => {
                        let mark = PrivateId(interp.next_private_id);
                        interp.next_private_id += 1;
                        interp.node_host.platform.mark = Some(mark);
                        mark
                    }
                };
                let stored = f64::from((mode as u8 & (TRANSFERABLE | CLONEABLE)) | MARKED);
                if let Some(cell) = interp.get_object_cell(id) {
                    cell.borrow_mut()
                        .private_fields_mut()
                        .insert(mark, PrivateElement::Field(JsValue::number(stored)));
                }
                Completion::Normal(JsValue::UNDEFINED)
            },
        ));
        self.install_host_global("__host_transfer_mark", mark);

        let hook = self.create_function(JsFunction::native(
            "__host_transfer_hook".to_string(),
            1,
            |interp, _this, args| {
                let hook = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if interp.node_host.platform.hook.is_none() && hook.is_object() {
                    if let Some(global) = interp.realm().global_object {
                        interp.pin_native_root(&JsValue::object(global), &hook);
                    }
                    interp.node_host.platform.hook = Some(hook);
                }
                Completion::Normal(JsValue::UNDEFINED)
            },
        ));
        self.install_host_global("__host_transfer_hook", hook);
    }
}
