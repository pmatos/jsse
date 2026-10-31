//! V8's receiver wording for builtins called on the wrong receiver, under
//! `--node` only (#1532). The engine's text names no method and prints no
//! receiver; V8's does (`String.prototype.trim called on null or undefined`,
//! `Method Date.prototype.toString called on incompatible receiver #<Object>`).
//! The builtin that is running and its receiver are the top of the native
//! frame list (`node_host_stack`), so the throw sites stay as they are:
//! `node_error_message` asks `frame_wording` first.

use super::super::*;

const ARRAY_NAMED: [&str; 13] = [
    "concat",
    "every",
    "filter",
    "find",
    "findIndex",
    "findLast",
    "findLastIndex",
    "forEach",
    "indexOf",
    "map",
    "reduce",
    "reduceRight",
    "some",
];

const DATE_METHOD: [&str; 27] = [
    "getYear",
    "setDate",
    "setFullYear",
    "setHours",
    "setMilliseconds",
    "setMinutes",
    "setMonth",
    "setSeconds",
    "setTime",
    "setUTCDate",
    "setUTCFullYear",
    "setUTCHours",
    "setUTCMilliseconds",
    "setUTCMinutes",
    "setUTCMonth",
    "setUTCSeconds",
    "setYear",
    "toDateString",
    "toISOString",
    "toLocaleDateString",
    "toLocaleString",
    "toLocaleTimeString",
    "toString",
    "toTemporalInstant",
    "toTimeString",
    "toUTCString",
    "toGMTString",
];

const ARRAY_BUFFER_METHOD: [&str; 4] = ["resize", "slice", "transfer", "transferToFixedLength"];

const TYPED_ARRAY_METHOD: [&str; 8] = [
    "at",
    "join",
    "slice",
    "sort",
    "subarray",
    "toLocaleString",
    "toReversed",
    "toSorted",
];

const EXACT_1532: [(&str, &str); 5] = [
    (
        "'ownKeys' on proxy: trap returned extra keys for non-extensible proxy target",
        "'ownKeys' on proxy: trap returned extra keys but proxy target is non-extensible",
    ),
    (
        "Cannot construct TypedArray from detached ArrayBuffer",
        "Cannot perform Construct on a detached ArrayBuffer",
    ),
    (
        "Cannot construct DataView from detached ArrayBuffer",
        "Cannot perform DataView constructor on a detached ArrayBuffer",
    ),
    (
        "maxByteLength must be at least as large as byteLength",
        "Invalid array buffer max length",
    ),
    (
        "Invalid index for TypedArray.prototype.with",
        "Invalid typed array index",
    ),
];

const PROMISE_STATIC: [&str; 5] = ["all", "allSettled", "any", "race", "reject"];

impl Interpreter {
    fn wording_own_function(&self, object: u64, key: &str) -> Option<u64> {
        self.get_object(object)?
            .borrow()
            .get_own_property(key)?
            .value?
            .as_object_id()
    }

    /// Whether `func` is `class.prototype[name]` (or `class[name]` when
    /// `statics`), by the realm's intrinsics.
    fn is_builtin_member(&self, func: u64, class: &str, name: &str, statics: bool) -> bool {
        let realm = self.realm();
        let proto = match class {
            "Array" => realm.array_prototype,
            "String" => realm.string_prototype,
            "Object" => realm.object_prototype,
            "Date" => realm.date_prototype,
            "RegExp" => realm.regexp_prototype,
            "ArrayBuffer" => realm.arraybuffer_prototype,
            "DataView" => realm.dataview_prototype,
            "Promise" => realm.promise_prototype,
            "BigInt" => realm.bigint_prototype,
            "%TypedArray%" => realm.typed_array_prototype,
            _ => None,
        };
        let holder = if statics {
            proto.and_then(|p| self.wording_own_function(p, "constructor"))
        } else {
            proto
        };
        holder.and_then(|h| self.wording_own_function(h, name)) == Some(func)
    }

    pub(super) fn incompatible(&self, label: &str, receiver: &JsValue) -> String {
        format!(
            "Method {label} called on incompatible receiver {}",
            self.v8_plain_text(receiver)
        )
    }

    /// V8's text for an engine message thrown by the builtin that is running,
    /// from its name and receiver.
    pub(crate) fn frame_wording(&self, msg: &str) -> Option<String> {
        if let Some((_, v8)) = EXACT_1532.iter().find(|(engine, _)| *engine == msg) {
            return Some((*v8).to_string());
        }
        let (func, receiver) = self.top_native()?;
        let name = self.own_data_string(func, "name")?;
        let name = name.as_str();
        if let Some(head) = msg
            .strip_prefix("RegExp.prototype")
            .and_then(|rest| rest.split_once(" requires that 'this' be a"))
            .map(|(head, _)| head)
        {
            let (shown, frame) = match head.strip_prefix("[@@").and_then(|h| h.strip_suffix(']')) {
                Some(sym) => (format!(".@@{sym}"), format!("[Symbol.{sym}]")),
                None => (head.to_string(), head.trim_start_matches('.').to_string()),
            };
            let reached_through_exec = head == ".exec";
            return ((reached_through_exec || frame == name) && self.is_regexp_member(func, name))
                .then(|| self.incompatible(&format!("RegExp.prototype{shown}"), &receiver));
        }
        match msg {
            "String.prototype method called on null or undefined"
                if self.is_builtin_member(func, "String", name, false) =>
            {
                let shown = match name {
                    "trimStart" => "trimLeft",
                    "trimEnd" => "trimRight",
                    other => other,
                };
                Some(format!(
                    "String.prototype.{shown} called on null or undefined"
                ))
            }
            "Cannot convert undefined or null to object" if receiver.is_nullish() => {
                if ARRAY_NAMED.contains(&name) && self.is_builtin_member(func, "Array", name, false)
                {
                    return Some(format!(
                        "Array.prototype.{name} called on null or undefined"
                    ));
                }
                if name == "toLocaleString" && self.is_builtin_member(func, "Object", name, false) {
                    return Some(
                        "Object.prototype.toLocaleString called on null or undefined".to_string(),
                    );
                }
                (name == "catch" && self.is_builtin_member(func, "Promise", name, false))
                    .then(|| format!("Cannot read properties of {} (reading 'then')", receiver))
            }
            "not an ArrayBuffer"
                if ARRAY_BUFFER_METHOD.contains(&name)
                    && self.is_builtin_member(func, "ArrayBuffer", name, false) =>
            {
                Some(self.incompatible(&format!("ArrayBuffer.prototype.{name}"), &receiver))
            }
            "not a DataView" if self.is_builtin_member(func, "DataView", name, false) => {
                Some(self.incompatible(&format!("DataView.prototype.{name}"), &receiver))
            }
            "this is not a Date object"
                if DATE_METHOD.contains(&name)
                    && self.is_builtin_member(func, "Date", name, false) =>
            {
                Some(self.incompatible(&format!("Date.prototype.{name}"), &receiver))
            }
            "not a TypedArray" if self.is_builtin_member(func, "%TypedArray%", name, false) => {
                if TYPED_ARRAY_METHOD.contains(&name) {
                    Some(self.incompatible(&format!("%TypedArray%.prototype.{name}"), &receiver))
                } else {
                    Some("this is not a typed array.".to_string())
                }
            }
            "Promise resolve or reject function is not callable"
                if PROMISE_STATIC.contains(&name)
                    && receiver.as_object_id().is_none()
                    && self.is_builtin_member(func, "Promise", name, true) =>
            {
                Some(format!("Promise.{name} called on non-object"))
            }
            "Invalid index"
                if ["asIntN", "asUintN"].contains(&name)
                    && self.is_builtin_member(func, "BigInt", name, true) =>
            {
                Some("Invalid value: not (convertible to) a safe integer".to_string())
            }
            "Invalid index" if self.is_builtin_member(func, "DataView", name, false) => {
                Some("Offset is outside the bounds of the DataView".to_string())
            }
            "typed array is detached" | "source typed array is detached"
                if self.is_builtin_member(func, "%TypedArray%", name, false) =>
            {
                Some(format!(
                    "Cannot perform %TypedArray%.prototype.{name} on a detached or out-of-bounds ArrayBuffer"
                ))
            }
            "DataView buffer is detached" => {
                let getter = name.strip_prefix("get ");
                let method = getter.unwrap_or(name);
                let held =
                    getter.is_some() || self.is_builtin_member(func, "DataView", method, false);
                held.then(|| {
                    let prefix = if getter.is_some() { "get " } else { "" };
                    format!(
                        "Cannot perform {prefix}DataView.prototype.{method} on a detached or out-of-bounds ArrayBuffer"
                    )
                })
            }
            "ArrayBuffer is detached"
                if self.is_builtin_member(func, "ArrayBuffer", name, false) =>
            {
                Some(format!(
                    "Cannot perform ArrayBuffer.prototype.{name} on a detached ArrayBuffer"
                ))
            }
            "ArrayBuffer is not resizable"
                if self.is_builtin_member(func, "ArrayBuffer", name, false) =>
            {
                Some(self.incompatible("ArrayBuffer.prototype.resize", &receiver))
            }
            "new byte length exceeds maxByteLength"
                if name == "resize" && self.is_builtin_member(func, "ArrayBuffer", name, false) =>
            {
                Some("ArrayBuffer.prototype.resize: Invalid length parameter".to_string())
            }
            _ => self.symbol_frame_wording(msg, name, &receiver),
        }
    }

    fn is_regexp_member(&self, func: u64, name: &str) -> bool {
        name.starts_with("[Symbol.") || self.is_builtin_member(func, "RegExp", name, false)
    }

    /// `Function.prototype.call` on something that is not a function.
    pub(crate) fn call_target_error(&mut self, target: &JsValue) -> JsValue {
        let text = self.not_function_target_message("Function.prototype.call", target, "");
        self.create_type_error(&text)
    }
}
