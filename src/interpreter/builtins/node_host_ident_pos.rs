//! V8 reports the identifier for a reference error of a load that needs a hole
//! check or finds no binding, unless it is the first thing its statement
//! evaluates (then the statement's start stands) (#1511). The parser records
//! the identifier's offset under `--node` only; the error is made first and
//! moved to that position by whoever evaluated the identifier.

use super::super::Interpreter;
use crate::types::JsValue;

impl Interpreter {
    /// Remembers `id`, just made by `create_error`, when it reports a missing
    /// or uninitialised binding.
    pub(crate) fn node_note_ident_error(&mut self, id: u64, msg: &str) {
        if !self.node_host.stack.installed {
            return;
        }
        let name = msg.strip_suffix(" is not defined").or_else(|| {
            msg.strip_prefix("Cannot access '")?
                .strip_suffix("' before initialization")
        });
        if let Some(name) = name {
            self.node_host.ident_error = Some((id, name.into()));
            let store = self.node_host.assign_pos;
            if store != 0 && msg.ends_with(" is not defined") {
                self.node_host.call_pos = store;
                self.node_capture_stack(id, 0, None, true);
            }
        }
    }

    /// `thrown` came out of the evaluation of the identifier at `at` (offset
    /// plus one, 0: unknown) named `name`: when it is the error noted above,
    /// its stack moves to that position.
    pub(crate) fn node_place_ident_error(&mut self, thrown: &JsValue, name: &str, at: u32) {
        if self
            .node_host
            .ident_error
            .as_ref()
            .is_some_and(|e| *e.1 == *name)
        {
            self.node_move_ident_error(thrown, at);
        }
    }

    /// Moves the error noted above, if `thrown` is it, to `at`.
    pub(crate) fn node_move_ident_error(&mut self, thrown: &JsValue, at: u32) {
        let Some((id, _)) = &self.node_host.ident_error else {
            return;
        };
        if at == 0 || thrown.as_object_id() != Some(*id) {
            return;
        }
        let id = *id;
        self.node_host.ident_error = None;
        self.node_host.call_pos = at;
        self.node_capture_stack(id, 0, None, true);
    }
}
