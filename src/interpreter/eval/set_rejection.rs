//! The TypeError for a strict-mode [[Set]] the object rejected. Under `--node`
//! the text is V8's (#1510): `Cannot assign to read only property 'a' of object
//! '#<Object>'`, `Cannot add property b, object is not extensible`, `Cannot set
//! property x of #<G> which has only a getter`.

use super::*;

/// The state a rejected-[[Set]] diagnostic is chosen from, gathered once by
/// `Interpreter::set_rejection_facts`. `[[Set]]` has already decided and
/// discarded its reason by the time a message is needed, so the assignment
/// paths re-derive it here rather than each walking the prototype chain again.
pub(super) struct SetRejection {
    is_module_namespace: bool,
    has_own: bool,
    is_proxy: bool,
    /// `None` when the lookup was skipped because a proxy is involved.
    desc: Option<PropertyDescriptor>,
}

impl Interpreter {
    /// The strict-mode TypeError for a [[Set]] rejected at OrdinarySet's
    /// "Receiver is not an Object" step (§10.1.9.2 step 3): no own property can
    /// be created on a primitive, so the write simply cannot land.
    pub(super) fn non_object_receiver_error(
        &mut self,
        receiver: &JsValue,
        key: &JsPropertyKey,
    ) -> JsValue {
        let engine = format!("Cannot create property '{key}' on {receiver}");
        let text = self.v8_message(&engine, |i| {
            let kind = i.typeof_value(receiver);
            let kind = kind.as_string().map(|s| s.to_string()).unwrap_or_default();
            let own = receiver.as_string().is_some_and(|s| {
                key.eq_str("length")
                    || crate::interpreter::types::string_exotic_index(key, s.len()).is_some()
            });
            let (what, on) = if own {
                ("assign to read only", "of")
            } else {
                ("create", "on")
            };
            format!(
                "Cannot {what} property '{key}' {on} {kind} '{}'",
                i.v8_plain_text(receiver)
            )
        });
        self.create_type_error(&text)
    }

    /// Builds the strict-mode TypeError for a rejected [[Set]] on `obj_id`,
    /// preserving the diagnostic distinctions this assignment path historically
    /// produced: a module-namespace target, a getter-only accessor (own or
    /// inherited), or a plain read-only data property. Descriptor re-inspection
    /// is skipped when a proxy sits on the object or its prototype chain, so no
    /// trap is re-invoked on the error path.
    pub(super) fn read_only_assignment_error(
        &mut self,
        obj_id: u64,
        key: &JsPropertyKey,
    ) -> JsValue {
        let facts = self.set_rejection_facts(obj_id, key);
        self.read_only_assignment_error_from(obj_id, facts.as_ref(), key)
    }

    /// Reads, exactly once, every fact the two rejection formatters below
    /// select a message from. Returns `None` when `obj_id` names no live
    /// object, which both formatters report as the undecorated message.
    ///
    /// `desc` is deliberately left `None` when a proxy sits on the object or
    /// its prototype chain, so no trap is re-invoked on the error path.
    fn set_rejection_facts(&mut self, obj_id: u64, key: &JsPropertyKey) -> Option<SetRejection> {
        let cell = self.get_object_cell(obj_id)?;
        let (is_module_namespace, is_proxy, has_own) = {
            let b = cell.borrow();
            (
                b.module_namespace().is_some(),
                b.is_proxy() || b.is_proxy_revoked(),
                b.has_own_property(key),
            )
        };
        let desc = if is_proxy || self.has_proxy_in_prototype_chain(obj_id) {
            None
        } else {
            self.get_property_descriptor_on_id(obj_id, key)
        };
        Some(SetRejection {
            is_module_namespace,
            has_own,
            is_proxy,
            desc,
        })
    }

    /// V8's text for a rejected [[Set]] of `key` on `obj_id`, if the facts
    /// select one.
    fn v8_rejection_text(
        &self,
        obj_id: u64,
        facts: &SetRejection,
        key: &JsPropertyKey,
    ) -> Option<String> {
        let target = JsValue::object(obj_id);
        let shown = self.v8_plain_text(&target);
        if facts.is_proxy {
            return Some(format!(
                "'set' on proxy: trap returned falsish for property '{key}'"
            ));
        }
        let desc = facts.desc.as_ref()?;
        if desc.is_accessor_descriptor() {
            return Some(format!(
                "Cannot set property {key} of {shown} which has only a getter"
            ));
        }
        if desc.writable == Some(false) {
            let kind = self.typeof_value(&target);
            let kind = kind.as_string().map(|s| s.to_string()).unwrap_or_default();
            return Some(format!(
                "Cannot assign to read only property '{key}' of {kind} '{shown}'"
            ));
        }
        None
    }

    fn read_only_assignment_error_from(
        &mut self,
        obj_id: u64,
        facts: Option<&SetRejection>,
        key: &JsPropertyKey,
    ) -> JsValue {
        if self.node_host.stack.installed
            && let Some(f) = facts
            && !f.is_module_namespace
        {
            let text = self
                .v8_rejection_text(obj_id, f, key)
                .unwrap_or_else(|| format!("Cannot add property {key}, object is not extensible"));
            return self.create_type_error(&text);
        }
        match facts {
            Some(f) if f.is_module_namespace => self.create_type_error(&format!(
                "Cannot assign to read only property '{key}' of module namespace"
            )),
            Some(f) if f.desc.as_ref().is_some_and(|d| d.is_accessor_descriptor()) => self
                .create_type_error(&format!(
                    "Cannot set property '{key}' which has only a getter"
                )),
            _ => self.create_type_error(&format!("Cannot assign to read only property '{key}'")),
        }
    }

    /// Preserve the host-compatible diagnostics historically produced by the
    /// plain member-assignment dispatcher after canonical [[Set]] rejects.
    /// The rejection itself and all observable descriptor/proxy work have
    /// already happened inside `property.rs`; this only formats the TypeError.
    pub(super) fn member_assignment_error(
        &mut self,
        base: &JsValue,
        key: &JsPropertyKey,
    ) -> JsValue {
        let Some(obj_id) = base.as_object_id() else {
            return self.non_object_receiver_error(base, key);
        };
        let facts = self.set_rejection_facts(obj_id, key);
        if let Some(f) = &facts {
            if f.is_module_namespace {
                return self.create_type_error(&format!(
                    "Cannot assign to read only property '{key}' of object '[object Module]'"
                ));
            }
            if !f.has_own
                && f.desc
                    .as_ref()
                    .is_some_and(|d| d.is_data_descriptor() && d.writable == Some(false))
            {
                if self.node_host.stack.installed {
                    return self.read_only_assignment_error_from(obj_id, facts.as_ref(), key);
                }
                return self.create_type_error(&format!(
                    "Cannot assign to read only property '{key}' of object '#<Object>'"
                ));
            }
        }
        self.read_only_assignment_error_from(obj_id, facts.as_ref(), key)
    }
}
