//! V8's wording for the errors builtins throw, under `--node` only (#1510).
//!
//! Without the stack hooks every message keeps the engine's own text. A message
//! that needs no operand is rewritten in `create_error` (`static_text`); one
//! that prints a value or a name takes it from the throw site through the
//! helpers below.

use super::super::*;
use super::errors::error_class_name;
use crate::types::ValueKind;
use std::borrow::Cow;

const EXACT: &[(&str, &str)] = &[
    (
        "radix must be between 2 and 36",
        "toString() radix argument must be between 2 and 36",
    ),
    (
        "Cannot create proxy with a non-object as target",
        "Cannot create proxy with a non-object as target or handler",
    ),
    (
        "Cannot create proxy with a non-object as handler",
        "Cannot create proxy with a non-object as target or handler",
    ),
    (
        "WeakRef: target must be an object or non-registered symbol",
        "WeakRef: invalid target",
    ),
    (
        "FinalizationRegistry requires a callable cleanup callback",
        "FinalizationRegistry: cleanup must be callable",
    ),
    ("this is not a Date object", "this is not a Date object."),
    (
        "offset is outside the bounds of the DataView",
        "Offset is outside the bounds of the DataView",
    ),
    (
        "Promise constructor requires 'new'",
        "Promise constructor cannot be invoked without 'new'",
    ),
    (
        "Promise.resolve requires an object",
        "PromiseResolve called on non-object",
    ),
    (
        "Setter must be a function",
        "Object.prototype.__defineSetter__: Expecting function",
    ),
    (
        "Generator is already executing",
        "Generator is already running",
    ),
    (
        "AsyncGenerator is already executing",
        "Generator is already running",
    ),
    (
        "'super()' has already been called in this derived constructor",
        "Super constructor may only be called once",
    ),
    (
        "Getter must be a function",
        "Object.prototype.__defineGetter__: Expecting function",
    ),
];

const REGEXP_GETTERS: [&str; 9] = [
    "global",
    "ignoreCase",
    "multiline",
    "dotAll",
    "unicode",
    "unicodeSets",
    "sticky",
    "hasIndices",
    "source",
];

/// The V8 text for an engine message that needs no operand.
pub(crate) fn static_text(msg: &str) -> Option<Cow<'static, str>> {
    if let Some((_, v8)) = EXACT.iter().find(|(engine, _)| *engine == msg) {
        return Some(Cow::Borrowed(v8));
    }
    if let Some(method) = msg
        .strip_prefix("Reflect.")
        .and_then(|rest| rest.strip_suffix(" requires an object"))
    {
        return Some(Cow::Owned(format!("Reflect.{method} called on non-object")));
    }
    if let Some((head, class)) = msg.rsplit_once(" requires a ")
        && ["Number", "Boolean", "Symbol", "BigInt"].contains(&class)
        && head.starts_with(class)
        && head[class.len()..].starts_with(".prototype")
    {
        return Some(Cow::Owned(format!(
            "{head} requires that 'this' be a {class}"
        )));
    }
    if let Some(getter) = msg
        .strip_prefix("RegExp.prototype.")
        .and_then(|rest| rest.split_once(" requires that 'this' be a"))
        .map(|(getter, _)| getter)
        .filter(|getter| REGEXP_GETTERS.contains(getter))
    {
        return Some(Cow::Owned(format!(
            "RegExp.prototype.{getter} getter called on non-RegExp object"
        )));
    }
    if msg.starts_with("Invalid regular expression: /")
        && let Some(at) = msg.rfind("/ : ")
    {
        return Some(Cow::Owned(format!("{}/: {}", &msg[..at], &msg[at + 4..])));
    }
    if let Some(flags) = msg.strip_prefix("Invalid regular expression flags '") {
        return Some(Cow::Owned(format!(
            "Invalid flags supplied to RegExp constructor '{flags}"
        )));
    }
    msg.strip_prefix("The normalization form should be one of NFC, NFD, NFKC, NFKD.")
        .map(|_| Cow::Borrowed("The normalization form should be one of NFC, NFD, NFKC, NFKD."))
}

impl Interpreter {
    pub(crate) fn create_error(&mut self, name: &str, msg: &str) -> JsValue {
        let msg = self.node_error_message(msg);
        let ctor = self.get_global_var(name);
        let error_proto_id: Option<u64> = ctor.and_then(|v| {
            let constructor_id = v.as_object_id()?;
            self.get_property_on_id(constructor_id, "prototype")
                .as_object_id()
        });
        let obj_id = self.create_object_id();
        {
            let mut o = self.get_object_cell_expect(obj_id).borrow_mut();
            o.class_name = error_class_name(name);
            if let Some(proto_id) = error_proto_id {
                o.prototype_id = Some(proto_id);
            }
            o.insert_builtin(
                "message".to_string(),
                JsValue::string(JsString::from_str(&msg)),
            );
            o.insert_builtin(
                "name".to_string(),
                JsValue::string(JsString::from_str(name)),
            );
        }
        self.node_capture_error(obj_id)
    }
}

impl Interpreter {
    /// `create_error`'s message under `--node`.
    pub(crate) fn node_error_message<'a>(&self, msg: &'a str) -> Cow<'a, str> {
        if !self.node_host.stack.installed {
            return Cow::Borrowed(msg);
        }
        static_text(msg).unwrap_or(Cow::Borrowed(msg))
    }

    /// V8's `NoSideEffectsToString`: a string bare, a function by its source,
    /// an error as `Name: message`, an object as `#<Constructor>` or
    /// `[object Tag]`. Never runs user code.
    pub(crate) fn v8_plain_text(&self, value: &JsValue) -> String {
        if let Some(s) = value.as_string() {
            return s.to_string();
        }
        let Some(id) = value.as_object_id() else {
            return value.to_string();
        };
        let Some(obj) = self.get_object(id) else {
            return "#<Object>".to_string();
        };
        if let Some(callable) = obj.borrow().callable.as_deref() {
            return match callable {
                crate::interpreter::types::JsFunction::User {
                    source_text: Some(text),
                    ..
                } => text.as_str().to_string(),
                crate::interpreter::types::JsFunction::Native(name, ..) => {
                    format!("function {name}() {{ [native code] }}")
                }
                _ => "function () { [native code] }".to_string(),
            };
        }
        let class = obj.borrow().class_name.to_string();
        if class == "Error" {
            let name = self.data_property_string(id, "name").unwrap_or_default();
            let message = self.data_property_string(id, "message").unwrap_or_default();
            return match (name.is_empty(), message.is_empty()) {
                (_, true) => name,
                (true, false) => message,
                (false, false) => format!("{name}: {message}"),
            };
        }
        if let Some(name) = self.default_to_string_constructor(id) {
            return format!("#<{name}>");
        }
        format!("[object {class}]")
    }

    /// The first data property `key` along `id`'s prototype chain.
    fn data_property_value(&self, mut id: u64, key: &str) -> Option<JsValue> {
        for _ in 0..64 {
            let obj = self.get_object(id)?;
            let b = obj.borrow();
            if b.is_proxy() {
                return None;
            }
            if let Some(desc) = b.get_own_property(key) {
                return desc.value;
            }
            id = b.prototype_id?;
        }
        None
    }

    pub(super) fn data_property_string(&self, id: u64, key: &str) -> Option<String> {
        let value = self.data_property_value(id, key)?;
        value.as_string().map(|s| s.to_string())
    }

    /// `Constructor` when `id` still inherits `Object.prototype.toString` and
    /// has a named constructor, as V8 prints `#<Constructor>`.
    fn default_to_string_constructor(&self, id: u64) -> Option<String> {
        let to_string = self.data_property_value(id, "toString")?.as_object_id()?;
        let object_proto = self.realm().object_prototype?;
        let original = self
            .get_object(object_proto)?
            .borrow()
            .get_own_property("toString")?
            .value?
            .as_object_id()?;
        if to_string != original {
            return None;
        }
        let ctor = self
            .data_property_value(id, "constructor")?
            .as_object_id()?;
        self.own_data_string(ctor, "name").filter(|n| !n.is_empty())
    }

    /// V8's default call site text for a value that is not a function:
    /// `number 5`, `string "x"`, `object null`, `undefined`, `object`.
    pub(crate) fn v8_call_site_text(&self, value: &JsValue) -> String {
        let kind = self.typeof_value(value);
        let typeof_text = kind.as_string().map(|s| s.to_string()).unwrap_or_default();
        match value.kind() {
            ValueKind::String => format!("{typeof_text} \"{}\"", self.v8_plain_text(value)),
            ValueKind::Null | ValueKind::Boolean | ValueKind::Number => {
                format!("{typeof_text} {}", self.v8_plain_text(value))
            }
            _ => typeof_text,
        }
    }

    /// `callback` is not callable: V8 prints the value, not the builtin's
    /// parameter name.
    pub(crate) fn callback_error(&mut self, value: &JsValue, engine: &str) -> JsValue {
        let text = if self.node_host.stack.installed {
            format!("{} is not a function", self.v8_call_site_text(value))
        } else {
            engine.to_string()
        };
        self.create_type_error(&text)
    }

    /// `Method Foo.prototype.bar called on incompatible receiver 1`.
    pub(crate) fn incompatible_receiver_error(
        &mut self,
        method: &str,
        receiver: &JsValue,
        engine: &str,
    ) -> JsValue {
        let text = if self.node_host.stack.installed {
            format!(
                "Method {method} called on incompatible receiver {}",
                self.v8_plain_text(receiver)
            )
        } else {
            engine.to_string()
        };
        self.create_type_error(&text)
    }

    /// An error whose V8 text is `{prefix}: {value}` and whose engine text is
    /// `prefix`.
    pub(crate) fn error_with_value(
        &mut self,
        name: &str,
        engine: &str,
        v8_prefix: &str,
        value: &JsValue,
    ) -> JsValue {
        if self.node_host.stack.installed {
            let text = format!("{v8_prefix}{}", self.v8_plain_text(value));
            self.create_error(name, &text)
        } else {
            self.create_error(name, engine)
        }
    }

    pub(crate) fn proto_value_error(&mut self, value: &JsValue) -> JsValue {
        let engine = "Object prototype may only be an Object or null";
        self.error_with_value("TypeError", engine, &format!("{engine}: "), value)
    }

    pub(crate) fn descriptor_error(&mut self, value: &JsValue) -> JsValue {
        let engine = "Property description must be an object";
        self.error_with_value("TypeError", engine, &format!("{engine}: "), value)
    }

    pub(crate) fn compare_fn_error(&mut self, value: &JsValue) -> JsValue {
        let v8 = "The comparison function must be either a function or undefined: ";
        self.error_with_value("TypeError", "compareFn is not a function", v8, value)
    }

    pub(crate) fn promise_resolver_error(&mut self, value: &JsValue) -> JsValue {
        let engine = "Promise resolver is not a function";
        let text = if self.node_host.stack.installed {
            format!(
                "Promise resolver {} is not a function",
                self.v8_plain_text(value)
            )
        } else {
            engine.to_string()
        };
        self.create_type_error(&text)
    }

    /// `delete null.x`: V8 converts the base first.
    pub(crate) fn delete_nullish_error(&mut self, key: &str, base: &JsValue) -> JsValue {
        let text = if self.node_host.stack.installed {
            "Cannot convert undefined or null to object".to_string()
        } else {
            let base = if base.is_null() { "null" } else { "undefined" };
            format!("Cannot delete property '{key}' of {base}")
        };
        self.create_type_error(&text)
    }

    /// `v8` under `--node`, the engine's own `engine` text otherwise.
    pub(crate) fn v8_message(&self, engine: &str, v8: impl FnOnce(&Self) -> String) -> String {
        if self.node_host.stack.installed {
            v8(self)
        } else {
            engine.to_string()
        }
    }

    /// `Function.prototype.apply was called on 1, which is a number and not a
    /// function`.
    pub(crate) fn not_function_target_message(
        &self,
        method: &str,
        value: &JsValue,
        engine: &str,
    ) -> String {
        self.v8_message(engine, |i| {
            let kind = match value.kind() {
                ValueKind::Undefined => "undefined".to_string(),
                ValueKind::Null => "null".to_string(),
                _ => {
                    let word = i.typeof_value(value).as_string().map(|s| s.to_string());
                    let word = word.unwrap_or_default();
                    let article = if word == "object" { "an" } else { "a" };
                    format!("{article} {word}")
                }
            };
            format!(
                "{method} was called on {}, which is {kind} and not a function",
                i.v8_plain_text(value)
            )
        })
    }

    /// `Iterator value 1 is not an entry object`.
    pub(crate) fn entry_object_error(&mut self, value: &JsValue) -> JsValue {
        let text = self.v8_message("Iterator value is not an object", |i| {
            format!(
                "Iterator value {} is not an entry object",
                i.v8_plain_text(value)
            )
        });
        self.create_type_error(&text)
    }

    /// `Method Foo.prototype.bar called on incompatible receiver 1`; the
    /// engine text names the method before `requires` or `called on`.
    pub(crate) fn bad_this(&mut self, this: &JsValue, engine: &str) -> JsValue {
        let method = engine
            .split_once(" requires ")
            .or_else(|| engine.split_once(" called on "))
            .map(|(method, _)| method);
        match method {
            Some(method) if self.node_host.stack.installed => {
                let getter = method.ends_with(".prototype.size");
                let method = if getter {
                    format!("get {method}")
                } else {
                    method.to_string()
                };
                self.incompatible_receiver_error(&method, this, engine)
            }
            _ => self.create_type_error(engine),
        }
    }

    pub(crate) fn promise_then_receiver_error(&mut self, this: &JsValue) -> JsValue {
        self.bad_this(this, "Promise.prototype.then called on non-promise")
    }

    pub(crate) fn repeat_count_error(&mut self, count: &JsValue) -> JsValue {
        let text = self.v8_message("Invalid count value", |i| {
            format!("Invalid count value: {}", i.v8_plain_text(count))
        });
        self.create_range_error(&text)
    }

    pub(crate) fn delete_rejected_error(
        &mut self,
        target: &JsValue,
        key: &(impl std::fmt::Display + ?Sized),
    ) -> JsValue {
        let text = self.v8_message(&format!("Cannot delete property '{key}' of object"), |i| {
            format!(
                "Cannot delete property '{key}' of {}",
                i.v8_plain_text(target)
            )
        });
        self.create_type_error(&text)
    }

    /// `Object.defineProperty` refused: V8 tells a redefinition of an existing
    /// property from a new property on a non-extensible object.
    pub(crate) fn define_rejected_error(&mut self, target: u64, key: &JsPropertyKey) -> JsValue {
        let engine =
            "Cannot define property, object is not extensible or property is non-configurable";
        let exists = self
            .get_object(target)
            .is_some_and(|obj| obj.borrow().has_own_property(key));
        let text = self.v8_message(engine, |_| {
            if exists {
                format!("Cannot redefine property: {key}")
            } else {
                format!("Cannot define property {key}, object is not extensible")
            }
        });
        self.create_type_error(&text)
    }
}
