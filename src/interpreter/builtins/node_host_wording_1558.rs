//! More V8 wording for what builtins throw, under `--node` only (#1558).
//! Like `node_host_receiver_wording.rs`, the rules read the builtin that is
//! running (`top_native`) and rewrite the engine's message, so the throw
//! sites stay as they are.

use super::super::*;

const SHARED_BUFFER_MEMBER: [&str; 5] = [
    "grow",
    "slice",
    "get byteLength",
    "get growable",
    "get maxByteLength",
];

const INTL_CONSTRUCTOR: [&str; 10] = [
    "NumberFormat",
    "DateTimeFormat",
    "Collator",
    "PluralRules",
    "ListFormat",
    "RelativeTimeFormat",
    "DisplayNames",
    "Segmenter",
    "DurationFormat",
    "Locale",
];

const IMMUTABLE_PROTOTYPE: &str =
    "Immutable prototype object 'Object.prototype' cannot have their prototype set";

impl Interpreter {
    /// V8's text for an engine message, from the builtin that is running.
    pub(crate) fn wording_1558(&self, msg: &str) -> Option<String> {
        if msg == "Immutable prototype object's [[Prototype]] may not be set" {
            return Some(IMMUTABLE_PROTOTYPE.to_string());
        }
        let (func, receiver) = self.top_native()?;
        let name = self.own_data_string(func, "name")?;
        let name = name.as_str();
        let intrinsics = &self.realm().iterator_next_intrinsics;
        let is_next = |intrinsic: Option<u64>| intrinsic == Some(func);
        if let Some((value, option)) = msg
            .strip_prefix("Value ")
            .and_then(|rest| rest.rsplit_once(" is not allowed for option "))
            && INTL_CONSTRUCTOR.contains(&name)
        {
            return Some(format!(
                "Value {value} out of range for Intl.{name} options property {option}"
            ));
        }
        if INTL_CONSTRUCTOR.contains(&name) {
            if let Some(code) = msg.strip_prefix("Invalid currency code: ") {
                return Some(format!("Invalid currency code : {code}"));
            }
            if let Some(option) = msg.strip_suffix(" is out of range")
                && option.ends_with("Digits")
            {
                return Some(format!("{option} value is out of range."));
            }
        }
        match msg {
            "Cyclic __proto__ value"
                if name == "set __proto__"
                    && receiver.as_object_id() == self.realm().object_prototype =>
            {
                Some(IMMUTABLE_PROTOTYPE.to_string())
            }
            "not a SharedArrayBuffer" | "SharedArrayBuffer is not growable"
                if SHARED_BUFFER_MEMBER.contains(&name) =>
            {
                let label = match name.strip_prefix("get ") {
                    Some(getter) => format!("get SharedArrayBuffer.prototype.{getter}"),
                    None => format!("SharedArrayBuffer.prototype.{name}"),
                };
                Some(self.incompatible(&label, &receiver))
            }
            "new byte length exceeds maxByteLength" if name == "grow" => {
                Some("SharedArrayBuffer.prototype.grow: Invalid length parameter".to_string())
            }
            "next called on non-object" | "next called on non-array iterator"
                if is_next(intrinsics.array) =>
            {
                Some(self.incompatible("Array Iterator.prototype.next", &receiver))
            }
            "next called on non-object" | "next called on non-string iterator"
                if is_next(intrinsics.string) =>
            {
                Some(self.incompatible("String Iterator.prototype.next", &receiver))
            }
            "%RegExpStringIteratorPrototype%.next requires that 'this' be a RegExp String Iterator" => {
                Some(self.incompatible("%RegExpStringIterator%.prototype.next", &receiver))
            }
            "value is not an object" if name == "from" => {
                Some("Iterator.from called on non-object".to_string())
            }
            _ => self.collator_wording(msg, name, &receiver),
        }
    }

    fn collator_wording(&self, msg: &str, name: &str, receiver: &JsValue) -> Option<String> {
        let member = msg
            .strip_prefix("Intl.Collator.prototype.")?
            .strip_suffix(" called on non-Collator object")?;
        let label = match name.strip_prefix("get ") {
            Some(getter) => format!("get Intl.Collator.prototype.{getter}"),
            None => format!("Intl.Collator.prototype.{member}"),
        };
        Some(self.incompatible(&label, receiver))
    }

    /// The SyntaxError for a pattern that does not compile: V8 prints the
    /// flags after the pattern.
    pub(crate) fn regexp_error(&mut self, msg: &str, flags: &str) -> JsValue {
        let text = self.v8_message(msg, |_| match msg.rsplit_once("/: ") {
            Some((head, reason)) if head.starts_with("Invalid regular expression: /") => {
                format!("{head}/{flags}: {reason}")
            }
            _ => crate::parser::v8_messages::regexp_literal_message(msg, flags),
        });
        self.create_error("SyntaxError", &text)
    }

    /// `FinalizationRegistry.prototype.register: invalid target` and the like.
    pub(crate) fn registry_error(&mut self, engine: &str, v8: &str) -> JsValue {
        let text = self.v8_message(engine, |_| v8.to_string());
        self.create_type_error(&text)
    }

    /// `Invalid unregisterToken ('1')`.
    pub(crate) fn unregister_token_error(&mut self, token: &JsValue, engine: &str) -> JsValue {
        let text = self.v8_message(engine, |i| {
            format!("Invalid unregisterToken ('{}')", i.v8_plain_text(token))
        });
        self.create_type_error(&text)
    }

    /// `structuredClone()` called without its argument: Node's
    /// `ERR_MISSING_ARGS`.
    pub(crate) fn structured_clone_missing_error(&mut self, engine: &str) -> JsValue {
        if !self.node_host.stack.installed {
            return self.create_type_error(engine);
        }
        let text = "The \"The value argument must be specified\" argument must be specified";
        let error = self.create_type_error(text);
        if let Some(id) = error.as_object_id() {
            let code = JsValue::string(JsString::from_str("ERR_MISSING_ARGS"));
            self.get_object_cell_expect(id)
                .borrow_mut()
                .insert_property(
                    "code".to_string(),
                    PropertyDescriptor::data(code, true, true, true),
                );
        }
        error
    }

    /// `Cannot initialize #y twice on the same object`: a private name of a
    /// class initialized on an object that has it already. `method` is for a
    /// private method or accessor.
    pub(crate) fn private_twice_error(&mut self, name: PrivateId, method: bool) -> JsValue {
        let engine = if method {
            "Cannot add private method to object twice"
        } else {
            "Cannot initialize private field twice on the same object"
        };
        let text = self.v8_message(engine, |i| i.private_twice_text(name, method, engine));
        self.create_type_error(&text)
    }
}
