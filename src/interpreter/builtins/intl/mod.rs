use super::super::*;

impl Interpreter {
    pub(crate) fn setup_intl(&mut self) {
        let intl_obj = self.create_object();
        let intl_id = intl_obj.borrow().id.unwrap();

        // @@toStringTag = "Intl" (per spec 8.1.1)
        {
            let key = "Symbol(Symbol.toStringTag)".to_string();
            let desc = PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("Intl"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            };
            intl_obj.borrow_mut().property_order.push(key.clone());
            intl_obj.borrow_mut().properties.insert(key, desc);
        }

        let intl_val = JsValue::Object(crate::types::JsObject { id: intl_id });
        self.global_env
            .borrow_mut()
            .declare("Intl", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("Intl", intl_val);
    }
}
