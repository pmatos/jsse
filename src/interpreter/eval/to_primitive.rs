//! ToPrimitive (§7.1.1) and OrdinaryToPrimitive (§7.1.1.1).

use super::*;

impl Interpreter {
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_primitive(
        &mut self,
        val: &JsValue,
        preferred_type: &str,
    ) -> Result<JsValue, JsValue> {
        if let Some(id) = val.as_object_id() {
            // The operand may already be popped off the VM stack, and the
            // @@toPrimitive / toString / valueOf lookups and calls run user code.
            self.with_gc_root_scope(|interp| {
                interp.gc_root_id(id);
                interp.resolve_object_primitive(id, val, preferred_type)
            })
        } else {
            Ok(val.clone())
        }
    }

    fn resolve_object_primitive(
        &mut self,
        id: u64,
        val: &JsValue,
        preferred_type: &str,
    ) -> Result<JsValue, JsValue> {
        // §7.1.1 Step 2-3: Check @@toPrimitive
        let exotic_to_prim = {
            let key = JsPropertyKey::well_known_symbol("toPrimitive");
            match self.get_object_property(id, &key, val) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::UNDEFINED,
            }
        };
        if !(exotic_to_prim).is_nullish() {
            if let Some(fo) = exotic_to_prim
                .as_object_id()
                .map(|id| crate::types::JsObject { id })
                && self
                    .get_object_cell(fo.id)
                    .map(|o| o.borrow().callable.is_some())
                    .unwrap_or(false)
            {
                let hint = JsValue::string(JsString::from_str(preferred_type));
                let result = self.call_function(&exotic_to_prim, val, &[hint]);
                match result {
                    Completion::Normal(v) if !(v).is_object() => {
                        return Ok(v);
                    }
                    Completion::Normal(_) => {
                        return Err(self.create_type_error("@@toPrimitive must return a primitive"));
                    }
                    Completion::Throw(e) => return Err(e),
                    _ => {}
                }
            } else {
                return Err(self.create_type_error("@@toPrimitive is not callable"));
            }
        }

        // §7.1.1.1 OrdinaryToPrimitive
        let methods = if preferred_type == "string" {
            ["toString", "valueOf"]
        } else {
            ["valueOf", "toString"]
        };
        for method_name in &methods {
            let method_val = match self.get_object_property(id, method_name, val) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::UNDEFINED,
            };
            if let Some(fo) = method_val
                .as_object_id()
                .map(|id| crate::types::JsObject { id })
                && self
                    .get_object_cell(fo.id)
                    .map(|o| o.borrow().callable.is_some())
                    .unwrap_or(false)
            {
                let result = self.call_function(&method_val, val, &[]);
                match result {
                    Completion::Normal(v) if !(v).is_object() => {
                        return Ok(v);
                    }
                    Completion::Throw(e) => return Err(e),
                    _ => {}
                }
            }
        }
        Err(self.create_type_error("Cannot convert object to primitive value"))
    }
}
