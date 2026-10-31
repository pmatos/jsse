//! More V8 wording for errors builtins throw under `--node` (#1510), apart
//! from `node_host_builtin_errors.rs`.

use super::super::*;

impl Interpreter {
    /// `number 1 is not iterable (cannot read property Symbol(Symbol.iterator))`.
    pub(crate) fn not_iterable_error(&mut self, value: &JsValue) -> JsValue {
        let text = self.v8_message("is not iterable", |i| {
            let shown = i.v8_call_site_text(value);
            format!("{shown} is not iterable (cannot read property Symbol(Symbol.iterator))")
        });
        let error = self.create_type_error(&text);
        if self.node_host.stack.installed {
            self.node_host.iterable_value = Some(value.clone());
        }
        let frame = self.call_depth;
        self.node_host.iterable_error = error
            .as_object_id()
            .filter(|_| self.node_host.stack.installed)
            .map(|id| (id, frame));
        error
    }

    /// `GetIterator(undefined)`.
    pub(crate) fn nullish_iterable_error(&mut self, value: &JsValue) -> JsValue {
        if self.node_host.stack.installed {
            return self.not_iterable_error(value);
        }
        self.create_type_error("Cannot convert undefined or null to object")
    }

    /// `ToIndex` whose RangeError reads as V8's under `--node`: `what` 1 is a
    /// typed array length, 2 an array buffer length.
    pub(crate) fn index_arg(&mut self, value: &JsValue, what: u8) -> Completion {
        if !self.node_host.stack.installed {
            return self.to_index(value);
        }
        let number = match self.to_number_value(value) {
            Ok(n) if n.is_nan() => 0.0,
            Ok(n) => n.trunc(),
            Err(e) => return Completion::Throw(e),
        };
        if !(0.0..=9007199254740991.0).contains(&number) {
            let text = if what == 1 {
                format!("Invalid typed array length: {}", self.v8_plain_text(value))
            } else {
                "Invalid array buffer length".to_string()
            };
            return Completion::Throw(self.create_range_error(&text));
        }
        Completion::Normal(JsValue::number(number))
    }

    /// `Atomics` on something that is not an integer (or waitable) typed array.
    pub(crate) fn atomics_array_error(&mut self, value: &JsValue, waitable: bool) -> JsValue {
        self.atomics_error(
            value,
            waitable,
            "Atomics operations require integer typed arrays",
        )
    }

    /// The same for an argument that is no typed array at all.
    pub(crate) fn atomics_arg_error(&mut self, value: &JsValue, waitable: bool) -> JsValue {
        self.atomics_error(value, waitable, "first argument must be a typed array")
    }

    fn atomics_error(&mut self, value: &JsValue, waitable: bool, engine: &str) -> JsValue {
        let text = self.v8_message(engine, |i| {
            let kind = if waitable {
                "an int32 or BigInt64"
            } else {
                "an integer"
            };
            format!("{} is not {kind} typed array.", i.v8_plain_text(value))
        });
        self.create_type_error(&text)
    }

    pub(crate) fn atomics_shared_error(&mut self, value: &JsValue, method: &str) -> JsValue {
        let engine = format!("Atomics.{method} requires a shared typed array");
        let text = self.v8_message(&engine, |i| {
            format!("{} is not a shared typed array.", i.v8_plain_text(value))
        });
        self.create_type_error(&text)
    }

    pub(crate) fn uncloneable_object(&mut self, class: &str, id: u64) -> JsValue {
        let text = self.v8_message(&format!("#<{class}> could not be cloned."), |i| {
            format!(
                "{} could not be cloned.",
                i.v8_plain_text(&JsValue::object(id))
            )
        });
        self.data_clone_error(&text)
    }

    /// `Getter must be a function: 1`.
    pub(crate) fn accessor_not_function_error(&mut self, which: &str, value: &JsValue) -> JsValue {
        let engine = format!("{which} must be a function");
        let text = self.v8_message(&engine, |i| {
            format!("{which} must be a function: {}", i.v8_plain_text(value))
        });
        self.create_type_error(&text)
    }

    pub(crate) fn mixed_descriptor_error(&mut self, value: &JsValue) -> JsValue {
        let engine = "Invalid property descriptor. Cannot both specify accessors and a value or writable attribute";
        let text = self.v8_message(engine, |i| format!("{engine}, {}", i.v8_plain_text(value)));
        self.create_type_error(&text)
    }

    pub(crate) fn proto_not_extensible_error(&mut self, target: &JsValue) -> JsValue {
        let engine = "Object.setPrototypeOf called on non-extensible object";
        let text = self.v8_message(engine, |i| {
            format!("{} is not extensible", i.v8_plain_text(target))
        });
        self.create_type_error(&text)
    }

    pub(crate) fn bigint_urshift_error(&mut self) -> JsValue {
        let engine = "Cannot mix BigInt and other types, use explicit conversions";
        let text = self.v8_message(engine, |_| {
            "BigInts have no unsigned right shift, use >> instead".to_string()
        });
        self.create_type_error(&text)
    }

    pub(crate) fn not_symbol_error(&mut self, value: &JsValue) -> JsValue {
        let engine = "Symbol.keyFor requires a symbol argument";
        let text = self.v8_message(engine, |i| {
            format!("{} is not a symbol", i.v8_plain_text(value))
        });
        self.create_type_error(&text)
    }

    pub(crate) fn reflect_apply_error(&mut self, target: &JsValue) -> JsValue {
        let engine = "Reflect.apply requires a function target";
        let text = self.not_function_target_message("Function.prototype.apply", target, engine);
        self.create_type_error(&text)
    }

    pub(crate) fn reflect_construct_error(&mut self, target: &JsValue) -> JsValue {
        let engine = "Reflect.construct requires a constructor";
        let text = self.v8_message(engine, |i| {
            format!("{} is not a constructor", i.v8_plain_text(target))
        });
        self.create_type_error(&text)
    }

    /// `start offset of Uint32Array should be a multiple of 4`.
    pub(crate) fn multiple_error(&mut self, what: &str, kind: &str, bpe: usize) -> JsValue {
        let engine = format!("{what} of typed array should be a multiple of BYTES_PER_ELEMENT");
        let text = self.v8_message(&engine, |_| {
            format!("{what} of {kind} should be a multiple of {bpe}")
        });
        self.create_range_error(&text)
    }

    /// A typed array constructor called without `new`.
    pub(crate) fn requires_new_error(&mut self, name: &str) -> JsValue {
        let text = self.v8_message(&format!("{name} is not a constructor"), |_| {
            format!("Constructor {name} requires 'new'")
        });
        self.create_type_error(&text)
    }
}
