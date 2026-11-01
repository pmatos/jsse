//! WrappedFunctionCreate (§3.1.2 of the ShadowRealm proposal).

use super::super::*;

impl Interpreter {
    pub(crate) fn wrapped_function_create(
        &mut self,
        caller_realm_id: usize,
        target_func: &JsValue,
    ) -> Result<JsValue, JsValue> {
        self.with_gc_root_scope(|interp| {
            interp.wrapped_function_create_rooted(caller_realm_id, target_func)
        })
    }

    fn wrapped_function_create_rooted(
        &mut self,
        caller_realm_id: usize,
        target_func: &JsValue,
    ) -> Result<JsValue, JsValue> {
        let func_id = self.with_realm(Some(caller_realm_id), |interp| {
            let func_id = interp.create_object_id();
            let fp_id = interp.realms[caller_realm_id].function_prototype;
            let mut o = interp.get_object_cell_expect(func_id).borrow_mut();
            o.set_proto(fp_id);
            o.set_class("Function");
            o.set_function_realm(caller_realm_id);
            o.callable = Some(Box::new(JsFunction::native(
                "".to_string(),
                0,
                |_, _, _| Completion::Normal(JsValue::UNDEFINED),
            )));
            if let Some(tf) = (target_func)
                .as_object_id()
                .map(|id| crate::types::JsObject { id })
            {
                o.kind = crate::interpreter::types::ObjectKind::WrappedFunction(Box::new(
                    crate::interpreter::types::WrappedFunctionData {
                        target_id: tf.id,
                        caller_realm_id,
                    },
                ));
            }
            func_id
        });
        // CopyNameAndLength reads `length` and `name` through proxy traps and
        // getters, which can collect; the wrapper has no reference yet.
        self.gc_root_id(func_id);
        self.gc_root_value(target_func);

        // CopyNameAndLength (spec §10.4.2.4) — any error becomes TypeError from callerRealm
        let length_val = if let Some(tf) = (target_func)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            // HasOwnProperty via [[GetOwnProperty]] (invokes proxy trap if proxy)
            let has_own_length = match self.proxy_get_own_property_descriptor(tf.id, "length") {
                Ok(v) if v.is_undefined() => false,
                Ok(_) => true,
                Err(_) => {
                    return Err(self.create_error_in_realm(
                        caller_realm_id,
                        "TypeError",
                        "WrappedFunctionCreate: error getting length descriptor",
                    ));
                }
            };
            if has_own_length {
                match self.get_object_property(tf.id, "length", target_func) {
                    Completion::Normal(v) => v,
                    Completion::Throw(_) => {
                        return Err(self.create_error_in_realm(
                            caller_realm_id,
                            "TypeError",
                            "WrappedFunctionCreate: error getting length",
                        ));
                    }
                    _ => JsValue::number(0.0),
                }
            } else {
                JsValue::number(0.0)
            }
        } else {
            JsValue::number(0.0)
        };

        let computed_length = match length_val.as_number() {
            Some(n) => {
                if n == f64::INFINITY {
                    f64::INFINITY
                } else if n == f64::NEG_INFINITY || n < 0.0 {
                    0.0
                } else {
                    n.trunc().max(0.0)
                }
            }
            None => 0.0,
        };

        let name_str = if let Some(tf) = (target_func)
            .as_object_id()
            .map(|id| crate::types::JsObject { id })
        {
            match self.get_object_property(tf.id, "name", target_func) {
                Completion::Normal(v) => v.as_string().map_or_else(String::new, |s| s.to_string()),
                Completion::Throw(_) => {
                    return Err(self.create_error_in_realm(
                        caller_realm_id,
                        "TypeError",
                        "WrappedFunctionCreate: error getting name",
                    ));
                }
                _ => String::new(),
            }
        } else {
            String::new()
        };

        if let Some(obj) = self.get_object_cell(func_id) {
            obj.borrow_mut().insert_property(
                "length".to_string(),
                PropertyDescriptor::data(JsValue::number(computed_length), false, false, true),
            );
            obj.borrow_mut().insert_property(
                "name".to_string(),
                PropertyDescriptor::data(
                    JsValue::string(crate::types::JsString::from_str(&name_str)),
                    false,
                    false,
                    true,
                ),
            );
        }

        Ok(JsValue::object(func_id))
    }
}
