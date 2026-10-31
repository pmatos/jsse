//! Proxy [[DefineOwnProperty]] (§10.5.6): the trap call and the invariant checks after it.

use super::{super::*, is_compatible_property_desc};

impl Interpreter {
    pub(crate) fn proxy_define_own_property<K: Into<JsPropertyKey>>(
        &mut self,
        obj_id: u64,
        key: K,
        desc_val: &JsValue,
    ) -> Result<bool, JsValue> {
        let key = key.into();
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            let key_val = self.symbol_key_to_jsvalue(&key);
            // The trap can run a collection (or revoke the proxy); the
            // invariant check afterwards still reads the descriptor object and
            // the target, so both stay rooted across it.
            self.with_gc_root_scope(|interp| {
                interp.gc_root_value(&target_val);
                interp.gc_root_value(desc_val);
                match interp.invoke_proxy_trap(
                    obj_id,
                    "defineProperty",
                    vec![target_val.clone(), key_val, desc_val.clone()],
                ) {
                    Ok(Some(v)) => {
                        if !interp.to_boolean_val(&v) {
                            return Ok(false);
                        }
                        let desc = interp.to_property_descriptor(desc_val).ok();
                        interp.check_proxy_define_invariants(&target_val, &key, desc.as_ref())?;
                        Ok(true)
                    }
                    Ok(None) => {
                        if let Some(target_id) = target_val.as_object_id() {
                            return interp.proxy_define_own_property(target_id, key, desc_val);
                        }
                        Ok(false)
                    }
                    Err(e) => Err(e),
                }
            })
        } else if self.get_object_cell(obj_id).is_some() {
            self.ensure_deferred_namespace_for_define(obj_id, &key)?;
            match self.to_property_descriptor(desc_val) {
                Ok(desc) => self.ordinary_define_own_property_native(obj_id, key, desc),
                Err(Some(e)) => Err(e),
                Err(None) => Ok(false),
            }
        } else {
            Ok(false)
        }
    }

    /// [[DefineOwnProperty]] taking the internal descriptor record directly. A
    /// `defineProperty` trap that is present still receives a descriptor object
    /// (FromPropertyDescriptor, built only then); without one the call forwards to
    /// the target and no JS object is materialised.
    pub(crate) fn proxy_define_own_property_native(
        &mut self,
        obj_id: u64,
        key: JsPropertyKey,
        desc: PropertyDescriptor,
    ) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_none() {
            self.ensure_deferred_namespace_for_define(obj_id, &key)?;
            return self.ordinary_define_own_property_native(obj_id, key, desc);
        }
        let (target_id, trap) = self.resolve_proxy_trap(obj_id, "defineProperty")?;
        let Some((handler_val, trap_val)) = trap else {
            return self.proxy_define_own_property_native(target_id, key, desc);
        };
        let target_val = JsValue::object(target_id);
        let key_val = self.symbol_key_to_jsvalue(&key);
        let desc_val = self.from_property_descriptor(&desc);
        self.with_gc_root_scope(|interp| {
            interp.gc_root_value(&target_val);
            for v in [&desc.value, &desc.get, &desc.set].into_iter().flatten() {
                interp.gc_root_value(v);
            }
            let trap_result = match interp.call_function(
                &trap_val,
                &handler_val,
                &[target_val.clone(), key_val, desc_val],
            ) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::UNDEFINED,
            };
            if !interp.to_boolean_val(&trap_result) {
                return Ok(false);
            }
            interp.check_proxy_define_invariants(&target_val, &key, Some(&desc))?;
            Ok(true)
        })
    }

    /// Deferred namespace: [[DefineOwnProperty]] with a non-symbol-like key
    /// triggers evaluation before anything else observable happens.
    fn ensure_deferred_namespace_for_define(
        &mut self,
        obj_id: u64,
        key: &JsPropertyKey,
    ) -> Result<(), JsValue> {
        let Some(obj) = self.get_object_cell(obj_id) else {
            return Ok(());
        };
        let is_deferred_ns = obj
            .borrow()
            .module_namespace()
            .as_ref()
            .is_some_and(|ns| ns.deferred);
        if is_deferred_ns && !Self::is_symbol_like_namespace_key(key, true) {
            self.ensure_deferred_namespace_evaluation(obj_id)?;
        }
        Ok(())
    }

    /// [[DefineOwnProperty]] of a non-proxy object (Array exotic or ordinary) from a
    /// descriptor record; callers have already run the deferred-namespace step.
    fn ordinary_define_own_property_native(
        &mut self,
        obj_id: u64,
        key: JsPropertyKey,
        desc: PropertyDescriptor,
    ) -> Result<bool, JsValue> {
        let Some(obj) = self.get_object(obj_id) else {
            return Ok(false);
        };
        if obj.borrow().class_name == "Array" {
            self.array_define_own_property(obj_id as usize, &key, desc)
        } else {
            Ok(obj.borrow_mut().define_own_property(key, desc))
        }
    }

    /// [[DefineOwnProperty]] steps 15-22 of the proxy: invariants checked after a
    /// truish `defineProperty` trap result. `desc` is `None` only when the caller
    /// could not form a descriptor record, in which case no invariant applies.
    fn check_proxy_define_invariants(
        &mut self,
        target_val: &JsValue,
        key: &JsPropertyKey,
        desc: Option<&PropertyDescriptor>,
    ) -> Result<(), JsValue> {
        if let Some(target_id) = target_val.as_object_id()
            && let Some(tobj) = self.get_object_cell(target_id)
        {
            let target_desc = tobj.borrow().get_own_property(key);
            let target_extensible = tobj.borrow().extensible;
            let setting_config_false = desc.is_some_and(|d| d.configurable == Some(false));

            if let Some(desc) = desc {
                // Step 19: targetDesc is undefined
                if target_desc.is_none() {
                    if !target_extensible {
                        return Err(self.create_type_error(
                            "'defineProperty' on proxy: trap returned truish for adding property to the non-extensible proxy target",
                        ));
                    }
                    if setting_config_false {
                        return Err(self.create_type_error(
                            "'defineProperty' on proxy: trap returned truish for defining non-configurable property which does not exist on the proxy target",
                        ));
                    }
                }
                // Step 20: targetDesc is not undefined
                if let Some(ref td) = target_desc {
                    // 20a: IsCompatiblePropertyDescriptor check
                    if !is_compatible_property_desc(target_extensible, desc, Some(td)) {
                        return Err(self.create_type_error(
                            "'defineProperty' on proxy: trap returned truish for property descriptor not compatible with the existing property in the proxy target",
                        ));
                    }
                    // 20b: settingConfigFalse + target configurable
                    if setting_config_false && td.configurable == Some(true) {
                        return Err(self.create_type_error(
                            "'defineProperty' on proxy: trap returned truish for defining non-configurable property which is configurable in the proxy target",
                        ));
                    }
                    // 20c: target non-configurable+writable, desc says non-writable
                    if td.is_data_descriptor()
                        && td.configurable == Some(false)
                        && td.writable == Some(true)
                        && desc.writable == Some(false)
                    {
                        return Err(self.create_type_error(
                            "'defineProperty' on proxy: trap returned truish for setting non-writable on a non-configurable writable property in the proxy target",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
