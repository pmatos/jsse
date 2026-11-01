//! What `--node` stack frames know about their receiver (issue #1229): the
//! type name V8 prints before a method (`Timeout._onTimeout`) comes from the
//! `this` a frame was called with, and for an arrow function that is not its
//! lexical `this`, so the call keeps it while the hooks are installed.

use super::super::*;

impl Interpreter {
    pub(crate) fn node_note_arrow_receiver(&mut self, receiver: &JsValue) {
        let depth = self.call_stack_frames.len().saturating_sub(1);
        self.node_host
            .stack
            .arrow_receivers
            .push((depth, receiver.clone()));
    }

    pub(super) fn node_arrow_receiver(&self, frame: usize) -> Option<JsValue> {
        let notes = &self.node_host.stack.arrow_receivers;
        notes
            .iter()
            .rev()
            .find(|(at, _)| *at == frame)
            .map(|(_, r)| r.clone())
    }

    pub(crate) fn collect_node_receiver_roots(&self, roots: &mut Vec<u64>) {
        for (_, receiver) in &self.node_host.stack.arrow_receivers {
            roots.extend(receiver.as_object_id());
        }
        roots.extend(
            self.node_host
                .stack
                .import_site
                .iter()
                .filter_map(JsValue::as_object_id),
        );
    }

    /// V8 prints a private (`#m`) or symbol-keyed (`[sym]`) method without
    /// its receiver's type.
    pub(super) fn receiver_type_name(
        &self,
        receiver: &JsValue,
        method: &Option<String>,
    ) -> Option<String> {
        if method.as_deref().is_some_and(|n| n.starts_with(['#', '['])) {
            return None;
        }
        let Some(mut id) = receiver.as_object_id() else {
            return [
                (receiver.is_string(), "String"),
                (receiver.is_number(), "Number"),
                (receiver.is_boolean(), "Boolean"),
                (receiver.is_symbol(), "Symbol"),
                (receiver.is_bigint(), "BigInt"),
            ]
            .into_iter()
            .find_map(|(is, name)| is.then(|| name.to_string()));
        };
        if self.node_host.vm.is_context_global(id)
            || self.realm().global_object == Some(id)
            || self.realms[0].global_object == Some(id)
        {
            return None;
        }
        if self.is_callable(receiver) {
            return Some(
                self.own_data_string(id, "name")
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "Function".to_string()),
            );
        }
        let start = id;
        for _ in 0..64 {
            let obj = self.get_object(id)?;
            let (ctor, proto) = {
                let b = obj.borrow();
                if b.is_proxy() {
                    break;
                }
                let ctor = b
                    .get_own_property("constructor")
                    .and_then(|d| d.value)
                    .and_then(|v| v.as_object_id());
                (ctor, b.proto_id())
            };
            if let Some(ctor) = ctor {
                if let Some(name) = self.own_data_string(ctor, "name").filter(|n| !n.is_empty())
                    && name != "Object"
                {
                    return Some(name);
                }
                break;
            }
            let Some(next) = proto else { break };
            id = next;
        }
        Some(
            self.string_tag(start)
                .unwrap_or_else(|| "Object".to_string()),
        )
    }

    /// The string a plain object's `Symbol.toStringTag` data property holds,
    /// found without running script (`JSON`, `Math`, `Reflect`): V8 names the
    /// receiver after it when the constructor is just `Object`.
    fn string_tag(&self, mut id: u64) -> Option<String> {
        let key = JsPropertyKey::well_known_symbol("toStringTag");
        for _ in 0..64 {
            let obj = self.get_object(id)?;
            let b = obj.borrow();
            if b.is_proxy() {
                return None;
            }
            if let Some(descriptor) = b.get_own_property(&key) {
                return descriptor
                    .value?
                    .as_string()
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty());
            }
            id = b.proto_id()?;
        }
        None
    }
}
