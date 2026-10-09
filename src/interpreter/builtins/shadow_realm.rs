use super::super::*;

impl Interpreter {
    pub(super) fn setup_shadow_realm(&mut self) {
        let my_realm_id = self.current_realm_id;
        let proto_id = self.create_object_id();
        let op = self.realm().object_prototype;
        {
            let mut p = self.get_object_cell_expect(proto_id).borrow_mut();
            p.class_name = "ShadowRealm".to_string();
            p.prototype_id = op;
        }

        // ShadowRealm.prototype[Symbol.toStringTag] = "ShadowRealm"
        self.define_to_string_tag(proto_id, "ShadowRealm");

        // ShadowRealm.prototype.evaluate
        let evaluate_fn = self.create_function(JsFunction::native(
            "evaluate".to_string(),
            1,
            move |interp, this, args| {
                let eval_realm_id = if let Some(shadow_realm_id) = this.as_object_id()
                    && let Some(obj) = interp.get_object_cell(shadow_realm_id)
                    && let Some(realm_id) = obj.borrow().shadow_realm_id()
                {
                    realm_id
                } else {
                    return Completion::Throw(interp.create_error_in_realm(
                        my_realm_id,
                        "TypeError",
                        "ShadowRealm.prototype.evaluate called on non-ShadowRealm",
                    ));
                };

                let source_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let source_text = if let Some(source) = source_val.as_string() {
                    source.to_string()
                } else {
                    return Completion::Throw(interp.create_error_in_realm(
                        my_realm_id,
                        "TypeError",
                        "ShadowRealm.prototype.evaluate: sourceText must be a string",
                    ));
                };

                interp.perform_realm_eval(&source_text, my_realm_id, eval_realm_id)
            },
        ));
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_builtin("evaluate".to_string(), evaluate_fn);

        // ShadowRealm.prototype.importValue
        let import_value_fn = self.create_function(JsFunction::native(
            "importValue".to_string(),
            2,
            move |interp, this, args| {
                let eval_realm_id = if let Some(shadow_realm_id) = this.as_object_id()
                    && let Some(obj) = interp.get_object_cell(shadow_realm_id)
                    && let Some(realm_id) = obj.borrow().shadow_realm_id()
                {
                    realm_id
                } else {
                    return Completion::Throw(interp.create_error_in_realm(
                        my_realm_id,
                        "TypeError",
                        "ShadowRealm.prototype.importValue called on non-ShadowRealm",
                    ));
                };

                let specifier_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let export_name_val = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);

                let specifier = match interp.to_string_value(&specifier_val) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };

                let export_name = if let Some(export_name) = export_name_val.as_string() {
                    export_name.to_string()
                } else {
                    return Completion::Throw(interp.create_error_in_realm(
                        my_realm_id,
                        "TypeError",
                        "ShadowRealm.prototype.importValue: exportName must be a string",
                    ));
                };

                let caller_realm_id = my_realm_id;
                let referrer = interp.current_module_path.clone();

                let old_realm = interp.current_realm_id;
                interp.current_realm_id = eval_realm_id;

                let module_path = match interp.resolve_module_specifier(
                    &specifier,
                    referrer.as_ref().and_then(ModuleKey::file_path),
                ) {
                    Ok(p) => p,
                    Err(_) => {
                        interp.current_realm_id = old_realm;
                        let err = interp.create_error_in_realm(
                            caller_realm_id,
                            "TypeError",
                            "ShadowRealm importValue: cannot resolve module",
                        );
                        return interp.create_rejected_promise(err);
                    }
                };

                let module = match interp.load_module_for_type(
                    &module_path,
                    None,
                    super::ModuleLoadMode::Evaluate,
                ) {
                    Ok(m) => m,
                    Err(_) => {
                        interp.current_realm_id = old_realm;
                        let err = interp.create_error_in_realm(
                            caller_realm_id,
                            "TypeError",
                            "ShadowRealm importValue: module load error",
                        );
                        return interp.create_rejected_promise(err);
                    }
                };
                let resolved_canon = module_path.canonicalize();
                let mut stack = vec![];
                if interp
                    .inner_module_evaluation(&resolved_canon, &mut stack, 0)
                    .is_err()
                {
                    interp.current_realm_id = old_realm;
                    let err = interp.create_error_in_realm(
                        caller_realm_id,
                        "TypeError",
                        "ShadowRealm importValue: module evaluation error",
                    );
                    return interp.create_rejected_promise(err);
                }
                interp.drain_microtasks();
                interp.current_realm_id = old_realm;

                // Get the named export
                let export_val = {
                    let m = module.borrow();
                    if let Some(v) = m.exports.get(&export_name) {
                        Some(v.clone())
                    } else {
                        // Also check export_bindings -> env
                        if let Some(binding_name) = m.export_bindings.get(&export_name) {
                            m.env.borrow().get(binding_name)
                        } else {
                            None
                        }
                    }
                };

                let export_val = match export_val {
                    Some(v) => v,
                    None => {
                        let err = interp.create_error_in_realm(
                            caller_realm_id,
                            "TypeError",
                            &format!(
                                "ShadowRealm importValue: export '{}' not found",
                                export_name
                            ),
                        );
                        return interp.create_rejected_promise(err);
                    }
                };

                match interp.get_wrapped_value(caller_realm_id, &export_val) {
                    Ok(wrapped) => interp.create_resolved_promise(wrapped),
                    Err(e) => interp.create_rejected_promise(e),
                }
            },
        ));
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_builtin("importValue".to_string(), import_value_fn);

        let proto_val = JsValue::object(proto_id);

        // ShadowRealm constructor
        let proto_val_for_ctor = proto_val.clone();
        let shadow_realm_ctor = self.create_function(JsFunction::constructor(
            "ShadowRealm".to_string(),
            0,
            move |interp, _this, _args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("ShadowRealm must be called with 'new'"),
                    );
                }

                let new_realm_id = interp.create_new_realm();

                let obj_id = interp.create_object_id();
                {
                    let mut o = interp.get_object_cell_expect(obj_id).borrow_mut();
                    o.class_name = "ShadowRealm".to_string();
                    o.kind = crate::interpreter::types::ObjectKind::ShadowRealm(new_realm_id);
                    if let Some(proto_id) = proto_val_for_ctor.as_object_id() {
                        o.prototype_id = Some(proto_id);
                    }
                }
                Completion::Normal(JsValue::object(obj_id))
            },
        ));

        // Set ShadowRealm.prototype on constructor
        if let Some(constructor_id) = shadow_realm_ctor.as_object_id()
            && let Some(ctor_obj) = self.get_object_cell(constructor_id)
        {
            ctor_obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val.clone(), false, false, false),
            );
        }

        // Set ShadowRealm.prototype.constructor = ShadowRealm
        if let Some(proto_id) = proto_val.as_object_id()
            && let Some(proto_obj) = self.get_object_cell(proto_id)
        {
            proto_obj
                .borrow_mut()
                .insert_builtin("constructor".to_string(), shadow_realm_ctor.clone());
        }

        // Store prototype in realm
        self.realm_mut().shadow_realm_prototype = Some(proto_id);

        // Register ShadowRealm as global
        let global_env = self.realm().global_env.clone();
        global_env
            .borrow_mut()
            .declare("ShadowRealm", BindingKind::Var);
        let _ = global_env
            .borrow_mut()
            .set("ShadowRealm", shadow_realm_ctor);
    }
}
