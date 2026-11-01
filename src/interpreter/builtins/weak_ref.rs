use super::super::*;

impl Interpreter {
    pub(crate) fn setup_weakref(&mut self) {
        let proto_id = self.create_object_id();
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .class_name = "WeakRef";

        // WeakRef.prototype.deref
        self.define_method(proto_id, "deref", 0, |interp, this, _args| {
            if let Some(o) = (this).as_object_id()
                && let Some(target) = interp
                    .get_object_cell(o)
                    .and_then(|obj| obj.borrow().weak_ref_target().cloned())
            {
                interp.keep_during_job(&target);
                return Completion::Normal(target);
            }
            Completion::Throw(interp.bad_this(this, "WeakRef.prototype.deref requires a WeakRef"))
        });

        // @@toStringTag
        self.define_to_string_tag(proto_id, "WeakRef");

        // WeakRef constructor
        let weakref_ctor = self.create_function(JsFunction::constructor(
            "WeakRef".to_string(),
            1,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    let err = interp.create_type_error("Constructor WeakRef requires 'new'");
                    return Completion::Throw(err);
                }
                let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !interp.can_be_held_weakly(&target) {
                    return Completion::Throw(interp.create_type_error(
                        "WeakRef: target must be an object or non-registered symbol",
                    ));
                }
                // OrdinaryCreateFromConstructor(NewTarget, "%WeakRef.prototype%")
                let proto = match interp
                    .get_prototype_from_new_target_realm(|realm| realm.weakref_prototype)
                {
                    Ok(p) => p,
                    Err(e) => return Completion::Throw(e),
                };
                let obj_id = interp.create_object_id();
                interp
                    .get_object_cell_expect(obj_id)
                    .borrow_mut()
                    .class_name = "WeakRef";
                if let Some(p) = proto {
                    interp
                        .get_object_cell_expect(obj_id)
                        .borrow_mut()
                        .set_proto(Some(p));
                }
                interp.keep_during_job(&target);
                interp.get_object_cell_expect(obj_id).borrow_mut().kind =
                    crate::interpreter::types::ObjectKind::WeakRef(Box::new(target));
                Completion::Normal(JsValue::object(obj_id))
            },
        ));

        if let Some(ctor_obj) = (weakref_ctor).as_object_id()
            && let Some(obj) = self.get_object_cell(ctor_obj)
        {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(JsValue::object(proto_id), false, false, false),
            );
        }

        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_property(
                "constructor".to_string(),
                PropertyDescriptor::data(weakref_ctor.clone(), true, false, true),
            );

        self.realm()
            .global_env
            .borrow_mut()
            .declare("WeakRef", BindingKind::Var);
        let _ = self
            .realm()
            .global_env
            .borrow_mut()
            .set("WeakRef", weakref_ctor);

        self.realm_mut().weakref_prototype = Some(proto_id);
    }

    pub(crate) fn setup_finalization_registry(&mut self) {
        let proto_id = self.create_object_id();
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .class_name = "FinalizationRegistry";

        // FinalizationRegistry.prototype.register
        self.define_method(proto_id, "register", 2,
            |interp, this, args| {
                if let Some(o) = (this).as_object_id()
                    && let Some(obj) = interp.get_object_cell(o)
                {
                    // Check [[Cells]] internal slot: class_name + map_data.is_some()
                    let has_cells = {
                        let b = obj.borrow();
                        b.class_name == "FinalizationRegistry"
                            && b.finalization_registry().is_some()
                    };
                    if has_cells {
                        let target = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        if !interp.can_be_held_weakly(&target) {
                            return Completion::Throw(interp.registry_error(
                                "FinalizationRegistry.register: target must be an object or non-registered symbol",
                                "FinalizationRegistry.prototype.register: invalid target",
                            ));
                        }
                        let held_value = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                        // SameValue(target, heldValue) => TypeError
                        if same_value(&target, &held_value) {
                            return Completion::Throw(interp.registry_error(
                                "FinalizationRegistry.register: target and heldValue must not be the same",
                                "FinalizationRegistry.prototype.register: target and holdings must not be same",
                            ));
                        }
                        let unregister_token = args.get(2).cloned().unwrap_or(JsValue::UNDEFINED);
                        // If CanBeHeldWeakly(unregisterToken) is false:
                        //   If unregisterToken is not undefined, throw TypeError
                        if !interp.can_be_held_weakly(&unregister_token)
                            && !(unregister_token).is_undefined() {
                                return Completion::Throw(interp.unregister_token_error(
                                    &unregister_token,
                                    "FinalizationRegistry.register: unregisterToken must be an object, non-registered symbol, or undefined",
                                ));
                            }
                        // Store cell: map_data stores (target, heldValue), set_data stores unregisterToken
                        let token_entry = if (unregister_token).is_undefined() {
                            None
                        } else {
                            Some(unregister_token)
                        };
                        if let Some(obj_rc) = interp.get_object_cell(o)
                            && let Some((cells, tokens)) =
                                obj_rc.borrow_mut().finalization_registry_mut()
                        {
                            cells.push(Some((target, held_value)));
                            tokens.push(token_entry);
                        }
                        return Completion::Normal(JsValue::UNDEFINED);
                    }
                }
                Completion::Throw(interp.create_type_error(
                    "FinalizationRegistry.prototype.register requires a FinalizationRegistry",
                ))
            },
        );

        // FinalizationRegistry.prototype.unregister
        self.define_method(proto_id, "unregister", 1,
            |interp, this, args| {
                if let Some(o) = (this).as_object_id()
                    && let Some(obj) = interp.get_object_cell(o)
                {
                    let has_cells = {
                        let b = obj.borrow();
                        b.class_name == "FinalizationRegistry"
                            && b.finalization_registry().is_some()
                    };
                    if has_cells {
                        let token = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                        if !interp.can_be_held_weakly(&token) {
                            return Completion::Throw(interp.unregister_token_error(
                                &token,
                                "FinalizationRegistry.unregister: unregisterToken must be an object or non-registered symbol",
                            ));
                        }
                        // Remove cells whose unregisterToken matches
                        let mut removed = false;
                        if let Some(obj_rc) = interp.get_object_cell(o)
                            && let Some((cells, tokens)) =
                                obj_rc.borrow_mut().finalization_registry_mut()
                        {
                            for i in 0..cells.len() {
                                let tok_matches = tokens
                                    .get(i)
                                    .and_then(|t| t.as_ref())
                                    .is_some_and(|tok| same_value(tok, &token));
                                let cell_some = cells.get(i).is_some_and(|c| c.is_some());
                                if cell_some && tok_matches {
                                    cells[i] = None;
                                    tokens[i] = None;
                                    removed = true;
                                }
                            }
                        }
                        return Completion::Normal(JsValue::boolean(removed));
                    }
                }
                Completion::Throw(interp.create_type_error(
                    "FinalizationRegistry.prototype.unregister requires a FinalizationRegistry",
                ))
            },
        );

        // FinalizationRegistry.prototype.cleanupSome
        self.define_method(proto_id, "cleanupSome", 0, |interp, this, _args| {
            if let Some(o) = (this).as_object_id()
                && let Some(obj) = interp.get_object_cell(o)
            {
                let has_cells = {
                    let b = obj.borrow();
                    b.class_name == "FinalizationRegistry" && b.finalization_registry().is_some()
                };
                if has_cells {
                    return Completion::Normal(JsValue::UNDEFINED);
                }
            }
            Completion::Throw(interp.create_type_error(
                "FinalizationRegistry.prototype.cleanupSome requires a FinalizationRegistry",
            ))
        });

        // @@toStringTag
        self.define_to_string_tag(proto_id, "FinalizationRegistry");

        // FinalizationRegistry constructor
        let fr_ctor = self.create_function(JsFunction::constructor(
            "FinalizationRegistry".to_string(),
            1,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Constructor FinalizationRegistry requires 'new'"),
                    );
                }
                let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !callback.is_object() {
                    return Completion::Throw(interp.create_type_error(
                        "FinalizationRegistry requires a callable cleanup callback",
                    ));
                }
                if let Some(o) = (callback).as_object_id()
                    && let Some(obj) = interp.get_object_cell(o)
                    && obj.borrow().callable.is_none()
                {
                    return Completion::Throw(interp.create_type_error(
                        "FinalizationRegistry requires a callable cleanup callback",
                    ));
                }
                // OrdinaryCreateFromConstructor(NewTarget, "%FinalizationRegistry.prototype%")
                let proto = match interp.get_prototype_from_new_target_realm(|realm| {
                    realm.finalization_registry_prototype
                }) {
                    Ok(p) => p,
                    Err(e) => return Completion::Throw(e),
                };
                let obj_id = interp.create_object_id();
                interp
                    .get_object_cell_expect(obj_id)
                    .borrow_mut()
                    .class_name = "FinalizationRegistry";
                if let Some(p) = proto {
                    interp
                        .get_object_cell_expect(obj_id)
                        .borrow_mut()
                        .set_proto(Some(p));
                }
                interp
                    .get_object_cell_expect(obj_id)
                    .borrow_mut()
                    .set_primitive_value(callback);
                // Initialize [[Cells]] as empty FinalizationRegistry slot data.
                interp.get_object_cell_expect(obj_id).borrow_mut().kind =
                    crate::interpreter::types::ObjectKind::FinalizationRegistry(Box::default());
                let id = obj_id;
                Completion::Normal(JsValue::object(id))
            },
        ));

        if let Some(ctor_obj) = (fr_ctor).as_object_id()
            && let Some(obj) = self.get_object_cell(ctor_obj)
        {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(JsValue::object(proto_id), false, false, false),
            );
        }

        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_property(
                "constructor".to_string(),
                PropertyDescriptor::data(fr_ctor.clone(), true, false, true),
            );

        self.realm()
            .global_env
            .borrow_mut()
            .declare("FinalizationRegistry", BindingKind::Var);
        let _ = self
            .realm()
            .global_env
            .borrow_mut()
            .set("FinalizationRegistry", fr_ctor);

        self.realm_mut().finalization_registry_prototype = Some(proto_id);
    }

    /// AddToKeptObjects (ECMA-262 §9.10.3): a WeakRef target observed during a
    /// job stays alive until the job ends.
    pub(crate) fn keep_during_job(&mut self, target: &JsValue) {
        if let Some(id) = target.as_object_id() {
            self.kept_alive.insert(id);
        }
    }

    /// HostEnqueueFinalizationRegistryCleanupJob: run the registry's cleanup
    /// callback for its cells with an empty target as a job of its own.
    pub(crate) fn enqueue_finalization_cleanup(&mut self, registry_id: u64) {
        let job: scheduler::MicrotaskJob =
            Box::new(move |interp| interp.cleanup_finalization_registry(registry_id));
        self.scheduler
            .enqueue_microtask((vec![JsValue::object(registry_id)], job));
    }

    /// CleanupFinalizationRegistry (ECMA-262 §26.2.3.1): each cell is removed
    /// before its callback runs, so an abrupt completion leaves the rest for
    /// the next job.
    fn cleanup_finalization_registry(&mut self, registry_id: u64) -> Completion {
        let Some(cell) = self.get_object_cell(registry_id) else {
            return Completion::Normal(JsValue::UNDEFINED);
        };
        let callback = cell.borrow().primitive_value().cloned();
        if let Some(data) = cell.borrow_mut().finalization_registry_data_mut() {
            data.cleanup_queued = false;
        }
        let Some(callback) = callback else {
            return Completion::Normal(JsValue::UNDEFINED);
        };
        let mut scan_from = 0;
        loop {
            let held = self.get_object_cell(registry_id).and_then(|cell| {
                let mut obj = cell.borrow_mut();
                let data = obj.finalization_registry_data_mut()?;
                let Some(index) = data
                    .cells
                    .iter()
                    .skip(scan_from)
                    .position(|c| c.as_ref().is_some_and(|(target, _)| target.is_undefined()))
                    .map(|offset| scan_from + offset)
                else {
                    let cells = std::mem::take(&mut data.cells);
                    let tokens = std::mem::take(&mut data.tokens);
                    for (cell, token) in cells.into_iter().zip(tokens) {
                        if cell.is_some() {
                            data.cells.push(cell);
                            data.tokens.push(token);
                        }
                    }
                    return None;
                };
                scan_from = index;
                data.tokens[index] = None;
                data.cells[index].take().map(|(_, held)| held)
            });
            let Some(held) = held else { break };
            let frame = self.gc_root_frame();
            self.gc_root_value(&held);
            let result = self.call_function(&callback, &JsValue::UNDEFINED, &[held]);
            self.gc_unroot_frame(frame);
            if !matches!(result, Completion::Normal(_)) {
                self.requeue_finalization_cleanup_if_pending(registry_id);
                return result;
            }
        }
        Completion::Normal(JsValue::UNDEFINED)
    }

    fn requeue_finalization_cleanup_if_pending(&mut self, registry_id: u64) {
        let pending = self
            .get_object_cell(registry_id)
            .and_then(|cell| {
                let mut obj = cell.borrow_mut();
                let data = obj.finalization_registry_data_mut()?;
                let pending = !data.cleanup_queued
                    && data
                        .cells
                        .iter()
                        .flatten()
                        .any(|(target, _)| target.is_undefined());
                data.cleanup_queued |= pending;
                Some(pending)
            })
            .unwrap_or(false);
        if pending {
            self.enqueue_finalization_cleanup(registry_id);
        }
    }
}
