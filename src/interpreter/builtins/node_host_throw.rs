//! Where a `throw` statement ran (issue #1229), for the `--node` fatal-error
//! report. The parser keeps the offset of the `throw` keyword; executing the
//! statement notes it with the thrown value, and `__host_stack_throw_site`
//! answers only for that same value, so an error built elsewhere or thrown by
//! the engine falls back to its own stack. The last few throws are kept, so a
//! throw caught inside a `finally` does not hide the site of the outer one.
//! Throws in embedded Node modules do not count, so a rethrow by the loader
//! keeps the user's site. Inert until the stack hooks are installed.

use super::super::*;
use std::rc::Rc;

const THROW_SITES: usize = 4;

impl Interpreter {
    pub(crate) fn node_note_throw(&mut self, value: &JsValue, at: u32) {
        if !self.node_host.stack.installed || at == 0 {
            return;
        }
        let Some(func) = self
            .call_stack_frames
            .last()
            .filter(|f| f.func_obj_id != 0 && !f.is_eval)
            .map(|f| f.func_obj_id)
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
        let (id, internal) = match &self.node_host.stack.throw_source {
            Some((cached, id, internal)) if Rc::ptr_eq(cached, &source) => (*id, *internal),
            _ => {
                let id = self.node_host.stack.source_id(&source);
                let internal = self.node_host.stack.sources[id]
                    .name
                    .as_ref()
                    .is_some_and(|name| name.starts_with("node:"));
                self.node_host.stack.throw_source = Some((source, id, internal));
                (id, internal)
            }
        };
        if internal {
            return;
        }
        let throws = &mut self.node_host.stack.throws;
        throws.retain(|(thrown, _, _)| !same_value(thrown, value));
        if throws.len() == THROW_SITES {
            throws.remove(0);
        }
        throws.push((value.clone(), id, at));
    }

    pub(crate) fn collect_node_throw_roots(&self, roots: &mut Vec<u64>) {
        for (value, _, _) in &self.node_host.stack.throws {
            roots.extend(value.as_object_id());
        }
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
            if object.array_elements().is_some() || object.proxy().is_some() {
                return None;
            }
            for (key, desc) in object.properties.iter().take(256) {
                if desc.value().and_then(JsValue::as_object_id) == Some(func)
                    && let Some(name) = key.as_str()
                {
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
                let value = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let Some(&(_, id, at)) = interp
                    .node_host
                    .stack
                    .throws
                    .iter()
                    .rev()
                    .find(|(thrown, _, _)| same_value(thrown, &value))
                else {
                    return undefined;
                };
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
