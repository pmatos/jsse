use super::super::*;

impl Interpreter {
    pub(crate) fn setup_proxy(&mut self) {
        // Proxy constructor
        let proxy_fn = self.create_function(JsFunction::constructor(
            "Proxy".to_string(),
            2,
            |interp, _this, args| {
                // Must be called with new (we check new.target)
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Constructor Proxy requires 'new'"),
                    );
                }
                let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                let handler = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                if !matches!(target, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp.create_type_error("Cannot create proxy with a non-object as target"),
                    );
                }
                if !matches!(handler, JsValue::Object(_)) {
                    return Completion::Throw(
                        interp
                            .create_type_error("Cannot create proxy with a non-object as handler"),
                    );
                }
                let proxy_obj_id = interp.create_object_id();
                interp
                    .get_object_cell_expect(proxy_obj_id)
                    .borrow_mut()
                    .class_name = "Proxy".to_string();
                if let JsValue::Object(ref t) = target
                    && let JsValue::Object(ref h) = handler
                    && let Some(target_rc) = interp.get_object_cell(t.id)
                {
                    let callable = target_rc.borrow().callable.clone();
                    let mut proxy = interp.get_object_cell_expect(proxy_obj_id).borrow_mut();
                    if callable.is_some() {
                        proxy.callable = callable;
                    }
                    proxy.kind = crate::interpreter::types::ObjectKind::Proxy(
                        crate::interpreter::types::ProxyData::active(t.id, h.id),
                    );
                }
                let proxy_id = proxy_obj_id;
                Completion::Normal(JsValue::Object(crate::types::JsObject { id: proxy_id }))
            },
        ));

        // Override eval_new behavior: Proxy constructor returns proxy_obj, not new_obj
        // The proxy constructor already returns an Object, so eval_new will use it.

        // Per spec §26.2.2: Proxy constructor has no prototype property
        if let JsValue::Object(ref pf) = proxy_fn
            && let Some(proxy_func_obj) = self.get_object_cell(pf.id)
        {
            proxy_func_obj.borrow_mut().remove_property("prototype");
        }

        // Proxy.revocable(target, handler)
        if let JsValue::Object(ref pf) = proxy_fn
            && let Some(proxy_func_obj) = self.get_object(pf.id)
        {
            let revocable_fn = self.create_function(JsFunction::native(
                "revocable".to_string(),
                2,
                |interp, _this, args| {
                    let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let handler = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    if !matches!(target, JsValue::Object(_)) {
                        return Completion::Throw(
                            interp.create_type_error(
                                "Cannot create proxy with a non-object as target",
                            ),
                        );
                    }
                    if !matches!(handler, JsValue::Object(_)) {
                        return Completion::Throw(interp.create_type_error(
                            "Cannot create proxy with a non-object as handler",
                        ));
                    }
                    let proxy_obj_id = interp.create_object_id();
                    interp
                        .get_object_cell_expect(proxy_obj_id)
                        .borrow_mut()
                        .class_name = "Proxy".to_string();
                    if let JsValue::Object(ref t) = target
                        && let JsValue::Object(ref h) = handler
                        && let Some(target_rc) = interp.get_object_cell(t.id)
                    {
                        let callable = target_rc.borrow().callable.clone();
                        let mut obj = interp.get_object_cell_expect(proxy_obj_id).borrow_mut();
                        if callable.is_some() {
                            obj.callable = callable;
                        }
                        obj.kind = crate::interpreter::types::ObjectKind::Proxy(
                            crate::interpreter::types::ProxyData::active(t.id, h.id),
                        );
                    }
                    let proxy_id = proxy_obj_id;
                    let proxy_val = JsValue::Object(crate::types::JsObject { id: proxy_id });

                    // Create revoke function that captures proxy_id
                    let revoke_fn = interp.create_function(JsFunction::native(
                        "".to_string(),
                        0,
                        move |interp2, _this2, _args2| {
                            if let Some(p) = interp2.get_object_cell(proxy_id)
                                && let crate::interpreter::types::ObjectKind::Proxy(ref mut pd) =
                                    p.borrow_mut().kind
                            {
                                pd.revoke();
                            }
                            Completion::Normal(JsValue::Undefined)
                        },
                    ));

                    let result_id = interp.create_object_id();
                    interp
                        .get_object_cell_expect(result_id)
                        .borrow_mut()
                        .insert_builtin("proxy".to_string(), proxy_val);
                    interp
                        .get_object_cell_expect(result_id)
                        .borrow_mut()
                        .insert_builtin("revoke".to_string(), revoke_fn);
                    Completion::Normal(JsValue::Object(crate::types::JsObject { id: result_id }))
                },
            ));
            proxy_func_obj
                .borrow_mut()
                .insert_builtin("revocable".to_string(), revocable_fn);
        }

        self.realm()
            .global_env
            .borrow_mut()
            .declare("Proxy", BindingKind::Var);
        let env = self.realm().global_env.clone();
        let _ = self.env_set(&env, "Proxy", proxy_fn);
    }
}
