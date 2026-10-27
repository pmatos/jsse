//! Where a `throw` statement ran (issue #1229), for the `--node` fatal-error
//! report. The parser keeps the offset of the `throw` keyword; executing the
//! statement notes it with the thrown value, and `__host_stack_throw_site`
//! answers only for that same value, so an error built elsewhere or thrown by
//! the engine falls back to its own stack. Throws in embedded Node modules do
//! not count, so a rethrow by the loader keeps the user's site. Inert until
//! the stack hooks are installed.

use super::super::*;

const THROWN: PrivateId = PrivateId(u64::MAX - 6);

impl Interpreter {
    pub(crate) fn node_note_throw(&mut self, value: &JsValue, at: u32) {
        if !self.node_host.stack.installed || at == 0 {
            return;
        }
        let Some(func) = self
            .call_stack_frames
            .iter()
            .rev()
            .map(|f| f.func_obj_id)
            .find(|&id| id != 0)
        else {
            return;
        };
        let source = self
            .get_object(func)
            .and_then(|obj| match obj.borrow().callable.as_deref() {
                Some(JsFunction::User {
                    source_text: Some(text),
                    ..
                }) => Some(text.origin().0.clone()),
                _ => None,
            });
        let Some(source) = source else {
            return;
        };
        let Some(global) = self.realm().global_object else {
            return;
        };
        let Some(handle) = self.get_object_cell(global).cloned() else {
            return;
        };
        let id = self.node_host.stack.source_id(&source);
        if self.node_host.stack.sources[id]
            .name
            .as_ref()
            .is_some_and(|name| name.starts_with("node:"))
        {
            return;
        }
        self.node_host.stack.throw_site = (id, at);
        handle.borrow_mut().set_hidden_slot(THROWN, value.clone());
        self.gc_write_barrier_value(&handle, value);
    }

    pub(crate) fn node_note_vm_throw(&mut self, value: &JsValue) {
        let at = self.take_call_pos();
        self.node_note_throw(value, at);
    }

    /// The property of `receiver` (or its prototypes) that holds `func`, for a
    /// frame whose function has no name: V8 prints it as `Type.method`.
    pub(super) fn method_name_of(&self, receiver: &JsValue, func: u64) -> Option<String> {
        let mut id = receiver.as_object_id()?;
        for _ in 0..16 {
            let obj = self.get_object(id)?;
            let object = obj.borrow();
            for key in object.ordinary_own_property_keys(OwnKeyFilter::AllStrings)? {
                let holds = object
                    .get_own_property(&key)
                    .and_then(|d| d.value?.as_object_id())
                    == Some(func);
                if let (true, Some(name)) = (holds, key.as_str()) {
                    return Some(name.to_string());
                }
            }
            id = object.prototype_id?;
        }
        None
    }

    pub(super) fn setup_node_host_throw(&mut self) {
        let site = self.create_function(JsFunction::native(
            "__host_stack_throw_site".to_string(),
            1,
            |interp, _this, args| {
                let undefined = Completion::Normal(JsValue::UNDEFINED);
                let (id, at) = interp.node_host.stack.throw_site;
                let value = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let stored = interp
                    .realm()
                    .global_object
                    .and_then(|g| interp.get_object(g))
                    .and_then(|g| g.borrow().hidden_slot(THROWN));
                if at == 0 || !stored.is_some_and(|s| same_value(&s, &value)) {
                    return undefined;
                }
                let Some((file, line, column)) = interp.node_host.stack.locate(id, at) else {
                    return undefined;
                };
                let file = file.map_or(JsValue::UNDEFINED, |f| {
                    JsValue::string(JsString::from_str(&f))
                });
                Completion::Normal(interp.create_array(vec![
                    file,
                    JsValue::number(line as f64),
                    JsValue::number(column as f64),
                ]))
            },
        ));
        self.install_host_global("__host_stack_throw_site", site);
    }
}
