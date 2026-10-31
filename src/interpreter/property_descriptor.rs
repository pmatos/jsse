//! ToPropertyDescriptor (§6.2.6.5).

use super::*;

impl Interpreter {
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn to_property_descriptor(
        &mut self,
        val: &JsValue,
    ) -> Result<PropertyDescriptor, Option<JsValue>> {
        // The field getters run user code that can collect: the descriptor
        // object and the value/get/set read so far (held only in the record
        // being built) stay rooted until the record is returned.
        self.with_gc_root_scope(|interp| {
            interp.gc_root_value(val);
            interp.read_property_descriptor(val)
        })
    }

    fn read_property_descriptor(
        &mut self,
        val: &JsValue,
    ) -> Result<PropertyDescriptor, Option<JsValue>> {
        if let Some(d) = (val).as_object_id().map(|id| crate::types::JsObject { id }) {
            let obj_id = d.id;
            let mut desc = PropertyDescriptor {
                value: None,
                writable: None,
                get: None,
                set: None,
                enumerable: None,
                configurable: None,
            };

            macro_rules! check_field {
                ($field:expr, $key:expr, $assign:expr) => {
                    let has = match self.proxy_has_property(obj_id, $key) {
                        Ok(b) => b,
                        Err(e) => return Err(Some(e)),
                    };
                    if has {
                        match self.get_object_property(obj_id, $key, val) {
                            Completion::Normal(v) => {
                                $assign(v);
                            }
                            Completion::Throw(e) => return Err(Some(e)),
                            _ => {}
                        }
                    }
                };
            }

            // §6.2.6.5 ToPropertyDescriptor — spec-mandated order
            check_field!(desc, "enumerable", |v: JsValue| desc.enumerable =
                Some(self.to_boolean_val(&v)));
            check_field!(desc, "configurable", |v: JsValue| desc.configurable =
                Some(self.to_boolean_val(&v)));
            check_field!(desc, "value", |v: JsValue| {
                self.gc_root_value(&v);
                desc.value = Some(v)
            });
            check_field!(desc, "writable", |v: JsValue| desc.writable =
                Some(self.to_boolean_val(&v)));
            check_field!(desc, "get", |v: JsValue| {
                self.gc_root_value(&v);
                desc.get = Some(v)
            });
            check_field!(desc, "set", |v: JsValue| {
                self.gc_root_value(&v);
                desc.set = Some(v)
            });

            // Validate: get must be callable or undefined
            if let Some(ref getter) = desc.get
                && !(getter).is_undefined()
            {
                let is_callable = if let Some(o) = (getter)
                    .as_object_id()
                    .map(|id| crate::types::JsObject { id })
                    && let Some(obj) = self.get_object_cell(o.id)
                {
                    obj.borrow().callable.is_some()
                } else {
                    false
                };
                if !is_callable {
                    return Err(Some(self.create_type_error("Getter must be a function")));
                }
            }
            if let Some(ref setter) = desc.set
                && !(setter).is_undefined()
            {
                let is_callable = if let Some(o) = (setter)
                    .as_object_id()
                    .map(|id| crate::types::JsObject { id })
                    && let Some(obj) = self.get_object_cell(o.id)
                {
                    obj.borrow().callable.is_some()
                } else {
                    false
                };
                if !is_callable {
                    return Err(Some(self.create_type_error("Setter must be a function")));
                }
            }

            // Cannot have both accessor and data descriptor fields
            if desc.is_accessor_descriptor() && desc.is_data_descriptor() {
                return Err(Some(self.create_type_error(
                    "Invalid property descriptor. Cannot both specify accessors and a value or writable attribute",
                )));
            }

            Ok(desc)
        } else {
            Err(Some(self.create_type_error(
                "Property description must be an object",
            )))
        }
    }
}
