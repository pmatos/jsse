//! V8's wording for proxy trap errors, under `--node` only (#1532): they name
//! the property and print the values the invariant compared. The engine's
//! invariant checks run right after the trap returned, in code that no longer
//! has the key, so `invoke_proxy_trap` leaves a note of the last trap call
//! (kept only with the stack hooks installed) for `proxy_wording` to read.

use super::super::*;

pub(crate) struct ProxyNote {
    trap: String,
    target: JsValue,
    key: Option<JsValue>,
    result: JsValue,
}

impl Interpreter {
    pub(crate) fn note_proxy_trap(&mut self, trap: &str, args: &[JsValue], result: &JsValue) {
        if !self.node_host.stack.installed {
            return;
        }
        self.node_host.proxy_note = Some(ProxyNote {
            trap: trap.to_string(),
            target: args.first().cloned().unwrap_or(JsValue::UNDEFINED),
            key: args.get(1).cloned(),
            result: result.clone(),
        });
    }

    /// Calls a proxy trap that does not go through `invoke_proxy_trap`.
    pub(crate) fn call_noted_trap(
        &mut self,
        trap: &str,
        function: &JsValue,
        handler: &JsValue,
        args: &[JsValue],
    ) -> Result<JsValue, JsValue> {
        match self.call_function(function, handler, args) {
            Completion::Normal(v) => {
                self.note_proxy_trap(trap, args, &v);
                Ok(v)
            }
            Completion::Throw(e) => Err(e),
            _ => Ok(JsValue::UNDEFINED),
        }
    }

    fn proxy_key_text(&self, key: &JsValue) -> String {
        match key.as_string() {
            Some(s) => format!("'{s}'"),
            None => self.v8_plain_text(key),
        }
    }

    /// `'1' returned for property 'get' of object '#<Object>' is not a function`.
    pub(crate) fn proxy_trap_error(
        &mut self,
        trap: &str,
        value: &JsValue,
        handler: &JsValue,
    ) -> JsValue {
        let engine = format!("proxy handler's {trap} trap is not a function");
        let text = self.v8_message(&engine, |i| {
            format!(
                "'{}' returned for property '{trap}' of object '{}' is not a function",
                i.v8_plain_text(value),
                i.v8_plain_text(handler)
            )
        });
        self.create_type_error(&text)
    }

    /// Notes the target and key of the define that is about to run, for the
    /// engine's key-less "Cannot define property" message.
    pub(crate) fn note_define_key(&mut self, target: u64, key: &JsPropertyKey) {
        if self.node_host.stack.installed {
            self.node_host.define_note = Some((target, key.clone()));
        }
    }

    /// V8's text for a refused `[[DefineOwnProperty]]` of `key` on `target`: a
    /// redefinition of an existing property differs from a new property on a
    /// non-extensible object, and a proxy names its trap.
    fn define_rejected_text(&self, target: u64, key: &JsPropertyKey) -> String {
        let object = self.get_object(target);
        if object.as_ref().is_some_and(|obj| obj.borrow().is_proxy()) {
            return format!(
                "'defineProperty' on proxy: trap returned falsish for property '{key}'"
            );
        }
        if object.is_some_and(|obj| obj.borrow().has_own_property(key)) {
            format!("Cannot redefine property: {key}")
        } else {
            format!("Cannot define property {key}, object is not extensible")
        }
    }

    pub(crate) fn define_rejected_error(&mut self, target: u64, key: &JsPropertyKey) -> JsValue {
        let engine =
            "Cannot define property, object is not extensible or property is non-configurable";
        let text = self.v8_message(engine, |i| i.define_rejected_text(target, key));
        self.create_type_error(&text)
    }

    /// The same message when the throw site has no key (`Object.defineProperty`
    /// on a proxy, `Object.defineProperties`).
    pub(crate) fn define_wording(&self, msg: &str) -> Option<String> {
        const ENGINE: &str =
            "Cannot define property, object is not extensible or property is non-configurable";
        let (target, key) = self
            .node_host
            .define_note
            .as_ref()
            .filter(|_| msg == ENGINE)?;
        Some(self.define_rejected_text(*target, key))
    }

    /// `'ownKeys' on proxy: trap result did not include 'x'`.
    pub(crate) fn proxy_missing_key_error(&mut self, key: &JsPropertyKey) -> JsValue {
        let engine = "'ownKeys' on proxy: trap result did not include all non-configurable own keys of the proxy target";
        let text = self.v8_message(engine, |_| {
            format!("'ownKeys' on proxy: trap result did not include '{key}'")
        });
        self.create_type_error(&text)
    }

    /// V8's text for an invariant message `'trap' on proxy: ...`.
    pub(crate) fn proxy_wording(&self, msg: &str) -> Option<String> {
        let rest = msg.strip_prefix('\'')?;
        let (trap, tail) = rest.split_once("' on proxy: ")?;
        let note = self
            .node_host
            .proxy_note
            .as_ref()
            .filter(|n| n.trap == trap)?;
        let key = self.proxy_key_text(note.key.as_ref()?);
        let target_value = || {
            let id = note.target.as_object_id()?;
            let name = note.key.as_ref()?.as_string()?.to_string();
            self.get_object(id)?.borrow().get_own_property(&name)?.value
        };
        let tail = if tail.starts_with("property is a read-only and non-configurable data") {
            let expected = self.v8_plain_text(&target_value()?);
            let got = self.v8_plain_text(&note.result);
            format!(
                "property {key} is a read-only and non-configurable data property on the proxy target but the proxy did not return its actual value (expected '{expected}' but got '{got}')"
            )
        } else if let Some(more) = tail.strip_prefix("property is a non-configurable accessor") {
            let got = self.v8_plain_text(&note.result);
            format!("property {key} is a non-configurable accessor{more} (got '{got}')")
        } else if tail.contains("for property ") && !tail.contains("for property '") {
            tail.replacen("for property ", &format!("for property {key} "), 1)
        } else {
            return None;
        };
        Some(format!("'{trap}' on proxy: {tail}"))
    }
}
