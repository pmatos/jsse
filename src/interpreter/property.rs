use super::*;
impl Interpreter {
    /// Canonical [[Set]] entry point (§10.1.9 OrdinarySet + exotic dispatch).
    /// Handles proxy `set` trap, TypedArray integer-index element set,
    /// accessor setter invocation, prototype-chain walk, and receiver logic.
    /// Returns `Ok(true)` on success, `Ok(false)` if the set was rejected
    /// (e.g. read-only property), `Err(e)` if an exception was thrown.
    #[allow(dead_code)]
    pub(crate) fn set_object_property<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
        value: JsValue,
        receiver: &JsValue,
    ) -> Result<bool, JsValue> {
        // Module namespace exotic [[Set]] (§10.4.6.9) always rejects the set,
        // even though exported bindings have writable own data descriptors.
        if self
            .get_object_cell(obj_id)
            .is_some_and(|obj| obj.borrow().module_namespace().is_some())
        {
            return Ok(false);
        }
        self.proxy_set(obj_id, key, value, receiver)
    }

    /// Canonical [[DefineOwnProperty]] entry point.
    ///
    /// Implements §10.1.6 OrdinaryDefineOwnProperty with exotic dispatch:
    /// proxy `defineProperty` trap, Array exotic, TypedArray exotic,
    /// module namespace, and ordinary define.
    ///
    /// `desc_val` is the property descriptor as a JsValue (object or undefined).
    #[allow(dead_code)]
    pub(crate) fn define_object_property<K: Into<JsPropertyKey>>(
        &mut self,
        obj_id: u64,
        key: K,
        desc_val: &JsValue,
    ) -> Result<bool, JsValue> {
        self.proxy_define_own_property(obj_id, key, desc_val)
    }

    /// Canonical [[Delete]] entry point (§10.1.5 OrdinaryDelete + exotic dispatch).
    /// Handles proxy `deleteProperty` trap, String exotic, and ordinary delete.
    #[allow(dead_code)]
    pub(crate) fn delete_object_property<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
    ) -> Result<bool, JsValue> {
        self.proxy_delete_property(obj_id, key)
    }

    pub(crate) fn get_object_property<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
        this_val: &JsValue,
    ) -> Completion {
        // Single-borrow fast path: classify object and check own property in one borrow
        if let Some(obj) = self.get_object_cell(obj_id) {
            let b = obj.borrow();
            let is_proxy = b.proxy().is_some();
            let has_module_ns = b.module_namespace().is_some();

            if !is_proxy && !has_module_ns {
                // Fast path for ordinary objects (the common case)
                let is_ta = b.typed_array_info().is_some();

                // TypedArray: canonical numeric index strings must not walk prototype
                if is_ta
                    && let Some(index) =
                        crate::interpreter::types::canonical_numeric_index_string(key)
                {
                    use crate::interpreter::types::{
                        is_valid_integer_index, typed_array_get_index,
                    };
                    let ta = b.typed_array_info().unwrap();
                    if is_valid_integer_index(ta, index) {
                        return Completion::Normal(typed_array_get_index(ta, index as usize));
                    }
                    return Completion::Normal(JsValue::Undefined);
                }

                // Check own property
                let own_desc = b.get_own_property_full(key);
                match own_desc {
                    Some(ref d)
                        if d.get.is_some() && !matches!(d.get, Some(JsValue::Undefined)) =>
                    {
                        let getter = d.get.clone().unwrap();
                        drop(b);
                        return self.call_function(&getter, this_val, &[]);
                    }
                    Some(ref d) if d.get.is_some() => {
                        return Completion::Normal(JsValue::Undefined);
                    }
                    Some(ref d) => {
                        return Completion::Normal(d.value.clone().unwrap_or(JsValue::Undefined));
                    }
                    None => {}
                }

                // Own property not found — walk prototype chain
                let proto = b.prototype_id;
                drop(b);
                if let Some(proto_rc) = proto {
                    let proto_id = proto_rc;
                    return self.get_object_property(proto_id, key, this_val);
                }
                return Completion::Normal(JsValue::Undefined);
            }
            drop(b);
        }

        // Slow path: proxy or module namespace objects
        self.get_object_property_slow(obj_id, key, this_val)
    }

    fn get_object_property_slow<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
        this_val: &JsValue,
    ) -> Completion {
        // Check if object is a proxy
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            let key_val = self.symbol_key_to_jsvalue(key);
            let receiver = this_val.clone();
            match self.invoke_proxy_trap(obj_id, "get", vec![target_val.clone(), key_val, receiver])
            {
                Ok(Some(v)) => {
                    // Invariant checks
                    if let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object_cell(t.id)
                    {
                        let target_desc = tobj.borrow().get_own_property(key);
                        if let Some(ref desc) = target_desc
                            && desc.configurable == Some(false)
                        {
                            if desc.is_data_descriptor()
                                && desc.writable == Some(false)
                                && !same_value(
                                    &v,
                                    desc.value.as_ref().unwrap_or(&JsValue::Undefined),
                                )
                            {
                                return Completion::Throw(self.create_type_error(
                                        "'get' on proxy: property is a read-only and non-configurable data property on the proxy target but the proxy did not return its actual value",
                                    ));
                            }
                            if desc.is_accessor_descriptor()
                                && matches!(
                                    desc.get.as_ref().unwrap_or(&JsValue::Undefined),
                                    JsValue::Undefined
                                )
                                && !matches!(v, JsValue::Undefined)
                            {
                                return Completion::Throw(self.create_type_error(
                                        "'get' on proxy: property is a non-configurable accessor property on the proxy target and does not have a getter function, but the trap did not return 'undefined'",
                                    ));
                            }
                        }
                    }
                    return Completion::Normal(v);
                }
                Ok(None) => {
                    // No trap, fall through to target
                    if let JsValue::Object(ref t) = target_val {
                        return self.get_object_property(t.id, key, this_val);
                    }
                    return Completion::Normal(JsValue::Undefined);
                }
                Err(e) => return Completion::Throw(e),
            }
        }

        // Module namespace: look up live binding from environment
        {
            let ns_data = self
                .get_object_cell(obj_id)
                .and_then(|obj| obj.borrow().module_namespace().cloned());
            if let Some(ns_data) = ns_data {
                // Deferred namespace: IsSymbolLikeNamespaceKey check
                if ns_data.deferred
                    && !key
                        .as_property_key_str()
                        .is_some_and(|key| Self::is_symbol_like_namespace_key(key, true))
                    && let Err(e) = self.ensure_deferred_namespace_evaluation(obj_id)
                {
                    return Completion::Throw(e);
                }
                if let Some(key_str) = key.as_property_key_str()
                    && let Some(binding_name) = ns_data.export_to_binding.get(key_str)
                {
                    let module_path = ns_data.module_path.clone();
                    match self.resolve_module_export_value(
                        binding_name,
                        &ns_data.env,
                        module_path.as_deref(),
                        key_str,
                    ) {
                        Ok(val) => return Completion::Normal(val),
                        Err(e) => return Completion::Throw(e),
                    }
                }
                // Fallback: check module's exports directly
                if let Some(key_str) = key.as_property_key_str()
                    && let Some(ref module_path) = ns_data.module_path
                    && let Some(module) = self.module_registry_get(module_path)
                    && let Some(val) = module.borrow().exports.get(key_str)
                {
                    return Completion::Normal(val.clone());
                }
            }
        }

        // Fallback: own property + prototype chain (for rare non-proxy, non-module cases)
        let own_desc = if let Some(obj) = self.get_object_cell(obj_id) {
            obj.borrow().get_own_property_full(key)
        } else {
            None
        };
        match own_desc {
            Some(ref d) if d.get.is_some() && !matches!(d.get, Some(JsValue::Undefined)) => {
                let getter = d.get.clone().unwrap();
                self.call_function(&getter, this_val, &[])
            }
            Some(ref d) if d.get.is_some() => Completion::Normal(JsValue::Undefined),
            Some(ref d) => Completion::Normal(d.value.clone().unwrap_or(JsValue::Undefined)),
            None => {
                let proto = if let Some(obj) = self.get_object_cell(obj_id) {
                    obj.borrow().prototype_id
                } else {
                    None
                };
                if let Some(proto_rc) = proto {
                    let proto_id = proto_rc;
                    self.get_object_property(proto_id, key, this_val)
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
        }
    }

    /// Proxy-aware [[HasProperty]] - checks proxy `has` trap, recurses on target if no trap.
    pub(crate) fn proxy_has_property<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
    ) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            let key_val = self.symbol_key_to_jsvalue(key);
            match self.invoke_proxy_trap(obj_id, "has", vec![target_val.clone(), key_val]) {
                Ok(Some(v)) => {
                    let trap_result = self.to_boolean_val(&v);
                    if !trap_result
                        && let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object_cell(t.id)
                    {
                        let target_desc = tobj.borrow().get_own_property(key);
                        if let Some(ref desc) = target_desc {
                            if desc.configurable == Some(false) {
                                return Err(self.create_type_error(
                                        "'has' on proxy: trap returned falsish for property which exists in the proxy target as non-configurable",
                                    ));
                            }
                            if !tobj.borrow().extensible {
                                return Err(self.create_type_error(
                                        "'has' on proxy: trap returned falsish for property but the proxy target is not extensible",
                                    ));
                            }
                        }
                    }
                    Ok(trap_result)
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_has_property(t.id, key);
                    }
                    Ok(false)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object(obj_id) {
            // Deferred namespace: trigger evaluation on [[HasProperty]] with non-symbol-like key
            {
                let is_deferred_ns = obj
                    .borrow()
                    .module_namespace()
                    .as_ref()
                    .is_some_and(|ns| ns.deferred);
                if is_deferred_ns
                    && !key
                        .as_property_key_str()
                        .is_some_and(|key| Self::is_symbol_like_namespace_key(key, true))
                {
                    self.ensure_deferred_namespace_evaluation(obj_id)?;
                }
            }
            // TypedArray §10.4.5.3 [[HasProperty]]: numeric indices handled by IsValidIntegerIndex only
            {
                let b = obj.borrow();
                if b.typed_array_info().is_some()
                    && let Some(index) =
                        crate::interpreter::types::canonical_numeric_index_string(key)
                {
                    return Ok(is_valid_integer_index(b.typed_array_info().unwrap(), index));
                }
            }
            if obj.borrow().has_own_property(key) {
                return Ok(true);
            }
            // Walk prototype chain, checking for proxies
            let proto = obj.borrow().prototype_id;
            if let Some(proto_rc) = proto {
                let proto_id = proto_rc;
                return self.proxy_has_property(proto_id, key);
            }
            Ok(false)
        } else {
            Ok(false)
        }
    }

    /// Iterative prototype-chain walk of `get_property`. Returns
    /// `JsValue::Undefined` when the key is not found anywhere in the chain.
    /// Each frame borrows the slot's `RefCell` directly via `get_object_cell_expect`,
    /// avoiding an `Rc::clone` per prototype hop. The walk holds no state across
    /// `&mut self` calls, so the `&self`-tied lifetime is safe.
    pub(crate) fn get_property_on_id<K: PropertyKeyLike + ?Sized>(
        &self,
        start_id: u64,
        key: &K,
    ) -> JsValue {
        let mut current = Some(start_id);
        while let Some(id) = current {
            let b = self.get_object_cell_expect(id).borrow();
            if let Some(v) = b.own_property_lookup(key) {
                return v;
            }
            let next = b.prototype_id;
            drop(b);
            current = next;
        }
        JsValue::Undefined
    }

    /// Iterative prototype-chain walk of `get_property_descriptor`.
    pub(crate) fn get_property_descriptor_on_id<K: PropertyKeyLike + ?Sized>(
        &self,
        start_id: u64,
        key: &K,
    ) -> Option<PropertyDescriptor> {
        let mut current = Some(start_id);
        while let Some(id) = current {
            let b = self.get_object_cell_expect(id).borrow();
            if let Some(d) = b.own_property_descriptor_lookup(key) {
                return Some(d);
            }
            let next = b.prototype_id;
            drop(b);
            current = next;
        }
        None
    }

    /// Iterative prototype-chain walk of `has_property`. TypedArray canonical
    /// numeric indices short-circuit the walk (per §10.4.5.2).
    // Documented chain-walking HasProperty primitive (sibling of
    // `own_has_property`); retained as the canonical API even when currently
    // uncalled — CreateDataProperty's extensibility guard now correctly uses
    // the own-only variant.
    #[allow(dead_code)]
    pub(crate) fn has_property_on_id<K: PropertyKeyLike + ?Sized>(
        &self,
        start_id: u64,
        key: &K,
    ) -> bool {
        let mut current = Some(start_id);
        while let Some(id) = current {
            let b = self.get_object_cell_expect(id).borrow();
            if let Some(result) = b.own_has_property(key) {
                return result;
            }
            let next = b.prototype_id;
            drop(b);
            current = next;
        }
        false
    }

    /// Iterative prototype-chain walk of `enumerable_keys_with_proto`. Merges
    /// each frame's emit with a global shadow set so non-enumerable own keys on
    /// one frame correctly suppress enumerable inherited keys with matching
    /// names on subsequent frames.
    pub(crate) fn enumerable_keys_with_proto_on_id(&self, start_id: u64) -> Vec<JsPropertyKey> {
        let mut global_seen: HashSet<JsPropertyKey> = HashSet::default();
        let mut result: Vec<JsPropertyKey> = Vec::new();
        let mut current = Some(start_id);
        while let Some(id) = current {
            let b = self.get_object_cell_expect(id).borrow();
            let (keys, shadow) = b.own_enumerable_keys_with_shadow();
            for k in keys {
                if global_seen.insert(k.clone()) {
                    result.push(k);
                }
            }
            for k in shadow {
                global_seen.insert(k);
            }
            let next = b.prototype_id;
            drop(b);
            current = next;
        }
        result
    }

    /// §10.4.2.4 ArraySetLength(A, Desc)
    pub(crate) fn array_set_length(
        &mut self,
        obj_id: usize,
        desc: PropertyDescriptor,
    ) -> Result<bool, JsValue> {
        // 1. If Desc does not have [[Value]], just do OrdinaryDefineOwnProperty(A, "length", Desc)
        if desc.value.is_none() {
            let obj_rc = self.get_object_cell(obj_id as u64).unwrap();
            return Ok(obj_rc
                .borrow_mut()
                .define_own_property("length".to_string(), desc));
        }

        // 2. Let newLenDesc be a copy of Desc.
        let desc_value = desc.value.clone().unwrap();
        let mut new_len_desc = desc;

        // 3. Let newLen be ? ToUint32(Desc.[[Value]]).
        //    ToUint32 internally calls ToNumber — this is valueOf call #1
        let num_for_uint32 = self.to_number_value(&desc_value)?;
        let new_len = crate::types::number_ops::to_uint32(num_for_uint32);

        // 4. Let numberLen be ? ToNumber(Desc.[[Value]]).
        //    This is a separate ToNumber call — valueOf call #2
        let number_len = self.to_number_value(&desc_value)?;

        // 5. If SameValueZero(newLen, numberLen) is false, throw RangeError.
        if (new_len as f64) != number_len {
            return Err(self.create_error("RangeError", "Invalid array length"));
        }

        // 5. Set newLenDesc.[[Value]] to newLen (as a Number).
        new_len_desc.value = Some(JsValue::Number(new_len as f64));

        // 6. Let oldLenDesc be OrdinaryGetOwnProperty(A, "length").
        let (old_len, old_len_writable) = {
            let obj_rc = self.get_object_cell(obj_id as u64).unwrap();
            let obj = obj_rc.borrow();
            let old_len_desc = obj.properties.get("length").cloned();
            match old_len_desc {
                Some(ref d) => {
                    let ol = d
                        .value
                        .as_ref()
                        .and_then(|v| {
                            if let JsValue::Number(n) = v {
                                Some(*n as u32)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);
                    let w = d.writable.unwrap_or(true);
                    (ol, w)
                }
                None => (0, true),
            }
        };

        // 7. If newLen >= oldLen, return OrdinaryDefineOwnProperty(A, "length", newLenDesc).
        if new_len >= old_len {
            let obj_rc = self.get_object_cell(obj_id as u64).unwrap();
            let result = obj_rc
                .borrow_mut()
                .define_own_property("length".to_string(), new_len_desc);
            return Ok(result);
        }

        // 8. If oldLenDesc.[[Writable]] is false, return false.
        if !old_len_writable {
            return Ok(false);
        }

        // 9. If newLenDesc.[[Writable]] is absent or true, let newWritable be true.
        //    Else, need to defer setting writable to false until after deletions.
        let new_writable = !matches!(new_len_desc.writable, Some(false));

        // 10. If newWritable is false, set newLenDesc.[[Writable]] to true (we'll set it false later).
        if !new_writable {
            new_len_desc.writable = Some(true);
        }

        // 11. Let succeeded be OrdinaryDefineOwnProperty(A, "length", newLenDesc).
        {
            let obj_rc = self.get_object_cell(obj_id as u64).unwrap();
            let succeeded = obj_rc
                .borrow_mut()
                .define_own_property("length".to_string(), new_len_desc);
            // 12. If succeeded is false, return false.
            if !succeeded {
                return Ok(false);
            }
        }

        // 13. For each property key P that is an array index, delete from oldLen-1 down to newLen.
        //     Stop if a deletion fails (non-configurable).
        let mut actual_new_len = new_len;
        {
            let obj_rc = self.get_object_cell(obj_id as u64).unwrap();
            let mut obj = obj_rc.borrow_mut();

            // Collect array index keys >= newLen and < oldLen, sorted descending.
            let mut idx_keys: Vec<(u32, String)> = obj
                .properties
                .keys()
                .filter_map(|k| {
                    k.parse::<u64>()
                        .ok()
                        .filter(|&idx| idx <= 0xFFFF_FFFE && k.eq_str(&idx.to_string()))
                        .map(|idx| idx as u32)
                        .filter(|&idx| idx >= new_len && idx < old_len)
                        .map(|idx| (idx, k.to_string()))
                })
                .collect();
            idx_keys.sort_by_key(|a| std::cmp::Reverse(a.0));

            for (idx, k) in &idx_keys {
                let is_non_configurable = obj
                    .properties
                    .get(k.as_str())
                    .is_some_and(|d| d.configurable == Some(false));
                if is_non_configurable {
                    // 13.d.iii. Set newLenDesc.[[Value]] to index + 1.
                    actual_new_len = idx + 1;
                    break;
                } else {
                    obj.remove_property(k.as_str());
                }
            }

            // Also truncate array_elements.
            if let Some(elements) = obj.array_elements_mut() {
                elements.truncate(actual_new_len as usize);
            }
        }

        // If we were blocked by a non-configurable element, update length and handle writable.
        if actual_new_len != new_len {
            let obj_rc = self.get_object_cell(obj_id as u64).unwrap();
            let mut obj = obj_rc.borrow_mut();
            if let Some(len_desc) = obj.properties.get_mut("length") {
                len_desc.value = Some(JsValue::Number(actual_new_len as f64));
            }
            // 13.d.iii.2. If newWritable is false, set length writable to false.
            if !new_writable && let Some(len_desc) = obj.properties.get_mut("length") {
                len_desc.writable = Some(false);
            }
            return Ok(false);
        }

        // 14. If newWritable is false, set [[Writable]] on length to false.
        if !new_writable {
            let obj_rc = self.get_object_cell(obj_id as u64).unwrap();
            let mut obj = obj_rc.borrow_mut();
            if let Some(len_desc) = obj.properties.get_mut("length") {
                len_desc.writable = Some(false);
            }
        }

        Ok(true)
    }

    /// §10.4.2.1 [[DefineOwnProperty]](P, Desc) for Array exotic objects
    pub(crate) fn array_define_own_property<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: usize,
        key: &K,
        desc: PropertyDescriptor,
    ) -> Result<bool, JsValue> {
        // 1. If P is "length", return ArraySetLength(A, Desc).
        if key.as_property_key_str() == Some("length") {
            return self.array_set_length(obj_id, desc);
        }

        // 2. If P is an array index (canonical numeric string with value <= 0xFFFFFFFE)...
        if let Some(key_str) = key.as_property_key_str()
            && let Ok(index) = key_str.parse::<u64>()
            && index <= 0xFFFF_FFFE
            && index.to_string() == key_str
        {
            let index_u32 = index as u32;

            // 2.a. Let oldLen be the current length value.
            let (old_len, length_writable) = {
                let obj_rc = self.get_object_cell(obj_id as u64).unwrap();
                let obj = obj_rc.borrow();
                let len_desc = obj.properties.get("length");
                let ol = len_desc
                    .and_then(|d| d.value.as_ref())
                    .and_then(|v| {
                        if let JsValue::Number(n) = v {
                            Some(*n as u32)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
                let w = len_desc.and_then(|d| d.writable).unwrap_or(true);
                (ol, w)
            };

            // 2.b. If index >= oldLen and length is non-writable, return false.
            if index_u32 >= old_len && !length_writable {
                return Ok(false);
            }

            // 2.c. Let succeeded be OrdinaryDefineOwnProperty(A, P, Desc).
            let succeeded = {
                let obj_rc = self.get_object_cell(obj_id as u64).unwrap();
                obj_rc
                    .borrow_mut()
                    .define_own_property(key.to_js_property_key(), desc.clone())
            };

            // 2.d. If succeeded is false, return false.
            if !succeeded {
                return Ok(false);
            }

            // 2.e. If index >= oldLen, set length to index + 1.
            if index_u32 >= old_len {
                let new_len = index_u32 + 1;
                let obj_rc = self.get_object_cell(obj_id as u64).unwrap();
                let mut obj = obj_rc.borrow_mut();
                if let Some(len_desc) = obj.properties.get_mut("length") {
                    len_desc.value = Some(JsValue::Number(new_len as f64));
                }
                if let Some(elems) = obj.array_elements_mut() {
                    let val = desc.value.unwrap_or(JsValue::Undefined);
                    let idx = index_u32 as usize;
                    if idx < elems.len() {
                        elems[idx] = val;
                    } else if idx <= elems.len() + 1024 {
                        while elems.len() < idx {
                            elems.push(JsValue::Undefined);
                        }
                        elems.push(val);
                    }
                }
            } else {
                let obj_rc = self.get_object_cell(obj_id as u64).unwrap();
                let mut obj = obj_rc.borrow_mut();
                if let Some(elems) = obj.array_elements_mut() {
                    let idx = index_u32 as usize;
                    if idx < elems.len()
                        && let Some(ref val) = desc.value
                    {
                        elems[idx] = val.clone();
                    }
                }
            }

            return Ok(true);
        }

        // 3. Return OrdinaryDefineOwnProperty(A, P, Desc).
        let obj_rc = self.get_object_cell(obj_id as u64).unwrap();
        Ok(obj_rc
            .borrow_mut()
            .define_own_property(key.to_js_property_key(), desc))
    }

    /// Proxy-aware [[Set]] - checks proxy `set` trap, recurses on target if no trap.
    pub(crate) fn proxy_set<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
        value: JsValue,
        receiver: &JsValue,
    ) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            let key_val = self.symbol_key_to_jsvalue(key);
            match self.invoke_proxy_trap(
                obj_id,
                "set",
                vec![target_val.clone(), key_val, value.clone(), receiver.clone()],
            ) {
                Ok(Some(v)) => {
                    if self.to_boolean_val(&v) {
                        if let JsValue::Object(ref t) = target_val
                            && let Some(tobj) = self.get_object(t.id)
                        {
                            let target_desc = tobj.borrow().get_own_property(key);
                            if let Some(ref desc) = target_desc
                                && desc.configurable == Some(false)
                            {
                                if desc.is_data_descriptor()
                                    && desc.writable == Some(false)
                                    && !same_value(
                                        &value,
                                        desc.value.as_ref().unwrap_or(&JsValue::Undefined),
                                    )
                                {
                                    return Err(self.create_type_error(
                                            "'set' on proxy: trap returned truish for property which exists in the proxy target as a non-configurable and non-writable data property with a different value",
                                        ));
                                }
                                if desc.is_accessor_descriptor()
                                    && matches!(
                                        desc.set.as_ref().unwrap_or(&JsValue::Undefined),
                                        JsValue::Undefined
                                    )
                                {
                                    return Err(self.create_type_error(
                                            "'set' on proxy: trap returned truish for property which exists in the proxy target as a non-configurable and non-writable accessor property without a setter",
                                        ));
                                }
                            }
                        }
                        Ok(true)
                    } else {
                        Ok(false)
                    }
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_set(t.id, key, value, receiver);
                    }
                    Ok(false)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object(obj_id) {
            // TypedArray [[Set]] §10.4.5.5
            let is_ta = obj.borrow().typed_array_info().is_some();
            if is_ta && let Some(index) = canonical_numeric_index_string(key) {
                let same_val = if let JsValue::Object(ref r) = *receiver {
                    r.id == obj_id
                } else {
                    false
                };
                if same_val {
                    // SameValue(O, Receiver): IntegerIndexedElementSet
                    let is_bigint = obj
                        .borrow()
                        .typed_array_info()
                        .map(|ta| ta.kind.is_bigint())
                        .unwrap_or(false);
                    let num_val = if is_bigint {
                        self.to_bigint_value(&value)?
                    } else {
                        JsValue::Number(self.to_number_value(&value)?)
                    };
                    let obj_ref = obj.borrow();
                    let ta = obj_ref.typed_array_info().unwrap();
                    if is_valid_integer_index(ta, index) {
                        let ta_clone = ta.clone();
                        drop(obj_ref);
                        typed_array_set_index(&ta_clone, index as usize, &num_val);
                    }
                    return Ok(true);
                } else {
                    // Different receiver: if invalid index return true without coercing
                    let valid = {
                        let obj_ref = obj.borrow();
                        let ta = obj_ref.typed_array_info().unwrap();
                        is_valid_integer_index(ta, index)
                    };
                    if !valid {
                        return Ok(true);
                    }
                    // Valid index, different receiver: fall through to OrdinarySet below
                    // OrdinarySet will find writable data descriptor from TypedArray [[GetOwnProperty]],
                    // then CreateDataProperty(receiver, P, V)
                }
            }
            // OrdinarySetWithOwnDescriptor
            let own_desc = obj.borrow().get_own_property(key);
            if let Some(ref desc) = own_desc {
                if desc.is_accessor_descriptor() {
                    // Call setter with receiver as this
                    if let Some(ref setter) = desc.set
                        && !matches!(setter, JsValue::Undefined)
                    {
                        let setter = setter.clone();
                        match self.call_function(&setter, receiver, &[value]) {
                            Completion::Normal(_) => return Ok(true),
                            Completion::Throw(e) => return Err(e),
                            _ => return Ok(true),
                        }
                    }
                    return Ok(false);
                }
                // Data descriptor
                if desc.writable == Some(false) {
                    return Ok(false);
                }
                // OrdinarySetWithOwnDescriptor step 3.c: use Receiver.[[GetOwnProperty]] / [[DefineOwnProperty]]
                let recv_id = if let JsValue::Object(r) = receiver {
                    Some(r.id)
                } else {
                    None
                };
                if recv_id == Some(obj_id) {
                    // Common case: receiver is the same object, direct set
                    return Ok(obj.borrow_mut().set_property_value(key, value));
                }
                // Receiver differs: call Receiver.[[GetOwnProperty]](P) and [[DefineOwnProperty]]
                if let Some(rid) = recv_id {
                    let existing = self.proxy_get_own_property_descriptor(rid, key)?;
                    if matches!(existing, JsValue::Undefined) {
                        // CreateDataProperty(Receiver, P, V)
                        let desc = crate::interpreter::types::PropertyDescriptor {
                            value: Some(value),
                            writable: Some(true),
                            enumerable: Some(true),
                            configurable: Some(true),
                            get: None,
                            set: None,
                        };
                        let desc_val = self.from_property_descriptor(&desc);
                        return self.proxy_define_own_property(
                            rid,
                            key.to_js_property_key(),
                            &desc_val,
                        );
                    } else {
                        // existingDescriptor found: check accessor or non-writable
                        let existing_desc = match self.to_property_descriptor(&existing) {
                            Ok(d) => d,
                            Err(Some(e)) => return Err(e),
                            Err(None) => return Ok(false),
                        };
                        if existing_desc.is_accessor_descriptor() {
                            return Ok(false);
                        }
                        if existing_desc.writable == Some(false) {
                            return Ok(false);
                        }
                        // [[DefineOwnProperty]](P, {Value: V})
                        let val_desc = crate::interpreter::types::PropertyDescriptor {
                            value: Some(value),
                            writable: None,
                            enumerable: None,
                            configurable: None,
                            get: None,
                            set: None,
                        };
                        let desc_val = self.from_property_descriptor(&val_desc);
                        return self.proxy_define_own_property(
                            rid,
                            key.to_js_property_key(),
                            &desc_val,
                        );
                    }
                }
                return Ok(obj.borrow_mut().set_property_value(key, value));
            }
            // No own property, walk prototype chain
            let proto = obj.borrow().prototype_id;
            if let Some(proto_rc) = proto {
                let proto_id = proto_rc;
                return self.proxy_set(proto_id, key, value, receiver);
            }
            // No prototype: OrdinarySetWithOwnDescriptor with synthetic {writable:true,...} ownDesc.
            // Per spec step 1.c.i + 2.c: call Receiver.[[GetOwnProperty]](P) then act on result.
            if let JsValue::Object(recv_o) = receiver {
                let recv_id = recv_o.id;
                let is_proxy_recv = self
                    .get_object_cell(recv_id)
                    .is_some_and(|o| o.borrow().is_proxy() || o.borrow().is_proxy_revoked());
                if is_proxy_recv {
                    let existing = self.proxy_get_own_property_descriptor(recv_id, key)?;
                    if matches!(existing, JsValue::Undefined) {
                        // CreateDataProperty(Receiver, P, V)
                        let create_desc = crate::interpreter::types::PropertyDescriptor {
                            value: Some(value),
                            writable: Some(true),
                            enumerable: Some(true),
                            configurable: Some(true),
                            get: None,
                            set: None,
                        };
                        let desc_val = self.from_property_descriptor(&create_desc);
                        return self.proxy_define_own_property(
                            recv_id,
                            key.to_js_property_key(),
                            &desc_val,
                        );
                    } else {
                        let existing_desc = match self.to_property_descriptor(&existing) {
                            Ok(d) => d,
                            Err(Some(e)) => return Err(e),
                            Err(None) => return Ok(false),
                        };
                        if existing_desc.is_accessor_descriptor() {
                            return Ok(false);
                        }
                        if existing_desc.writable == Some(false) {
                            return Ok(false);
                        }
                        let val_desc = crate::interpreter::types::PropertyDescriptor {
                            value: Some(value),
                            writable: None,
                            enumerable: None,
                            configurable: None,
                            get: None,
                            set: None,
                        };
                        let desc_val = self.from_property_descriptor(&val_desc);
                        return self.proxy_define_own_property(
                            recv_id,
                            key.to_js_property_key(),
                            &desc_val,
                        );
                    }
                }
                if let Some(recv_obj) = self.get_object_cell(recv_id) {
                    return Ok(recv_obj.borrow_mut().set_property_value(key, value));
                }
            }
            Ok(obj.borrow_mut().set_property_value(key, value))
        } else {
            Ok(false)
        }
    }

    pub(crate) fn has_proxy_in_prototype_chain(&self, obj_id: u64) -> bool {
        let mut current = Some(obj_id);
        while let Some(id) = current {
            if self.get_proxy_info(id).is_some() {
                return true;
            }
            let Some(obj) = self.get_object_cell(id) else {
                return false;
            };
            let b = obj.borrow();
            current = b.prototype_id;
        }
        false
    }

    /// Proxy-aware [[Delete]] - checks proxy `deleteProperty` trap, recurses on target if no trap.
    pub(crate) fn proxy_delete_property<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
    ) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            let key_val = self.symbol_key_to_jsvalue(key);
            match self.invoke_proxy_trap(
                obj_id,
                "deleteProperty",
                vec![target_val.clone(), key_val],
            ) {
                Ok(Some(v)) => {
                    let trap_result = self.to_boolean_val(&v);
                    if trap_result
                        && let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object_cell(t.id)
                    {
                        let target_desc = tobj.borrow().get_own_property(key);
                        if let Some(ref desc) = target_desc {
                            if desc.configurable == Some(false) {
                                return Err(self.create_type_error(
                                        "'deleteProperty' on proxy: trap returned truish for property which is non-configurable in the proxy target",
                                    ));
                            }
                            if !tobj.borrow().extensible {
                                return Err(self.create_type_error(
                                        "'deleteProperty' on proxy: trap returned truish for property but the proxy target is not extensible",
                                    ));
                            }
                        }
                    }
                    Ok(trap_result)
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_delete_property(t.id, key);
                    }
                    Ok(true)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object_cell(obj_id) {
            // String exotic [[Delete]]: "length" and valid indices are non-configurable
            {
                let borrow = obj.borrow();
                if borrow.class_name == "String"
                    && let Some(JsValue::String(ref s)) = borrow.primitive_value
                {
                    if key.as_property_key_str() == Some("length") {
                        return Ok(false);
                    }
                    if let Some(key_str) = key.as_property_key_str()
                        && let Ok(idx) = key_str.parse::<usize>()
                    {
                        let char_len = s.to_string().chars().count();
                        if idx < char_len {
                            return Ok(false);
                        }
                    }
                }
            }
            let mut m = obj.borrow_mut();
            if let Some(desc) = m.properties.get(key)
                && desc.configurable == Some(false)
            {
                return Ok(false);
            }
            m.remove_property(key);
            if let Some(key_str) = key.as_property_key_str()
                && let Some(elems) = m.array_elements_mut()
                && let Ok(idx) = key_str.parse::<usize>()
                && idx < elems.len()
            {
                elems[idx] = JsValue::Undefined;
            }
            Ok(true)
        } else {
            Ok(true)
        }
    }

    /// §10.4.5.6 TypedArray [[DefineOwnProperty]](P, Desc)
    /// Returns Ok(None) if not a typed array numeric index (caller should use generic path).
    /// Returns Ok(Some(bool)) if handled by TypedArray exotic logic.
    /// Returns Err(JsValue) if an error occurred (e.g., ToBigInt/ToNumber throws).
    pub(crate) fn typed_array_define_own_property<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
        desc: &PropertyDescriptor,
    ) -> Result<Option<bool>, JsValue> {
        use crate::interpreter::types::{canonical_numeric_index_string, is_valid_integer_index};

        let (is_ta, is_bigint) = {
            if let Some(obj) = self.get_object_cell(obj_id) {
                let b = obj.borrow();
                if let Some(ta) = b.typed_array_info() {
                    (true, ta.kind.is_bigint())
                } else {
                    (false, false)
                }
            } else {
                (false, false)
            }
        };

        if !is_ta {
            return Ok(None);
        }

        let index = match canonical_numeric_index_string(key) {
            Some(idx) => idx,
            None => return Ok(None),
        };

        // §10.4.5.6 step 3b.i: if not a valid integer index, return false
        {
            let valid = if let Some(obj) = self.get_object_cell(obj_id) {
                let b = obj.borrow();
                b.typed_array_info()
                    .as_ref()
                    .map(|ta| is_valid_integer_index(ta, index))
                    .unwrap_or(false)
            } else {
                false
            };
            if !valid {
                return Ok(Some(false));
            }
        }

        // §10.4.5.6 step 3b.ii: accessor descriptors not allowed
        if desc.get.is_some() || desc.set.is_some() {
            return Ok(Some(false));
        }
        // §10.4.5.6 step 3b.iii: configurable must not be false
        if desc.configurable == Some(false) {
            return Ok(Some(false));
        }
        // §10.4.5.6 step 3b.iv: enumerable must not be false
        if desc.enumerable == Some(false) {
            return Ok(Some(false));
        }
        // §10.4.5.6 step 3b.v: writable must not be false
        if desc.writable == Some(false) {
            return Ok(Some(false));
        }

        // §10.4.5.6 step 3b.vi: if [[Value]] present, call IntegerIndexedElementSet
        if let Some(ref value) = desc.value {
            // IntegerIndexedElementSet: ToNumber/ToBigInt first (may throw), then check valid
            let num_val = if is_bigint {
                self.to_bigint_value(value)?
            } else {
                JsValue::Number(self.to_number_value(value)?)
            };
            // After conversion, re-read ta info (buffer may have been detached during conversion)
            if let Some(obj) = self.get_object_cell(obj_id) {
                let b = obj.borrow();
                if let Some(ta) = b.typed_array_info()
                    && is_valid_integer_index(ta, index)
                {
                    let ta_clone2 = ta.clone();
                    drop(b);
                    crate::interpreter::types::typed_array_set_index(
                        &ta_clone2,
                        index as usize,
                        &num_val,
                    );
                }
            }
            return Ok(Some(true));
        }

        Ok(Some(true))
    }

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
            match self.invoke_proxy_trap(
                obj_id,
                "defineProperty",
                vec![target_val.clone(), key_val, desc_val.clone()],
            ) {
                Ok(Some(v)) => {
                    let trap_result = self.to_boolean_val(&v);
                    if !trap_result {
                        return Ok(false);
                    }
                    if let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object_cell(t.id)
                    {
                        let target_desc = tobj.borrow().get_own_property(&key);
                        let target_extensible = tobj.borrow().extensible;
                        let desc = self.to_property_descriptor(desc_val).ok();
                        let setting_config_false =
                            desc.as_ref().is_some_and(|d| d.configurable == Some(false));

                        if let Some(ref desc) = desc {
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
                                if !is_compatible_property_desc(target_extensible, desc, td) {
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
                    Ok(true)
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_define_own_property(t.id, key, desc_val);
                    }
                    Ok(false)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object_cell(obj_id) {
            // Deferred namespace: trigger evaluation on [[DefineOwnProperty]] with non-symbol-like key
            {
                let is_deferred_ns = obj
                    .borrow()
                    .module_namespace()
                    .as_ref()
                    .is_some_and(|ns| ns.deferred);
                if is_deferred_ns && !Self::is_symbol_like_namespace_key(&key, true) {
                    self.ensure_deferred_namespace_evaluation(obj_id)?;
                }
            }
            let obj = self.get_object(obj_id).unwrap();
            let is_array = obj.borrow().class_name == "Array";
            match self.to_property_descriptor(desc_val) {
                Ok(desc) => {
                    if is_array {
                        self.array_define_own_property(obj_id as usize, &key, desc)
                    } else {
                        Ok(obj.borrow_mut().define_own_property(key, desc))
                    }
                }
                Err(Some(e)) => Err(e),
                Err(None) => Ok(false),
            }
        } else {
            Ok(false)
        }
    }

    /// Proxy-aware [[GetOwnProperty]] - checks proxy `getOwnPropertyDescriptor` trap, recurses on target if no trap.
    pub(crate) fn proxy_get_own_property_descriptor<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
    ) -> Result<JsValue, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            let key_val = self.symbol_key_to_jsvalue(key);
            match self.invoke_proxy_trap(
                obj_id,
                "getOwnPropertyDescriptor",
                vec![target_val.clone(), key_val],
            ) {
                Ok(Some(v)) => {
                    // Step 11: If Type(trapResultObj) is neither Object nor Undefined, throw TypeError
                    if !matches!(v, JsValue::Object(_) | JsValue::Undefined) {
                        return Err(self.create_type_error(
                            "'getOwnPropertyDescriptor' on proxy: trap returned neither Object nor undefined",
                        ));
                    }
                    if let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object_cell(t.id)
                    {
                        let target_desc = tobj.borrow().get_own_property(key);
                        let target_extensible = tobj.borrow().extensible;
                        if matches!(v, JsValue::Undefined) {
                            if let Some(ref td) = target_desc {
                                if td.configurable == Some(false) {
                                    return Err(self.create_type_error(
                                        "'getOwnPropertyDescriptor' on proxy: trap returned undefined for property which is non-configurable in the proxy target",
                                    ));
                                }
                                if !target_extensible {
                                    return Err(self.create_type_error(
                                        "'getOwnPropertyDescriptor' on proxy: trap returned undefined for property which exists in the non-extensible proxy target",
                                    ));
                                }
                            }
                        } else if matches!(v, JsValue::Object(_)) {
                            let result_desc = match self.to_property_descriptor(&v) {
                                Ok(d) => d,
                                Err(Some(e)) => return Err(e),
                                Err(None) => return Ok(JsValue::Undefined),
                            };
                            // Step 22: If resultDesc.[[Configurable]] is false
                            if result_desc.configurable == Some(false) {
                                // 22a: If targetDesc is undefined or targetDesc.[[Configurable]] is true
                                if target_desc.is_none()
                                    || target_desc
                                        .as_ref()
                                        .is_some_and(|td| td.configurable == Some(true))
                                {
                                    return Err(self.create_type_error(
                                            "'getOwnPropertyDescriptor' on proxy: trap reported non-configurable for a property that is either non-existent or configurable in the proxy target",
                                        ));
                                }
                            }

                            if let Some(ref td) = target_desc {
                                if td.configurable == Some(false) {
                                    // Step 21a: resultDesc configurable:true for non-configurable target
                                    if result_desc.configurable == Some(true) {
                                        return Err(self.create_type_error(
                                                "'getOwnPropertyDescriptor' on proxy: trap returned descriptor with configurable: true for non-configurable property in the proxy target",
                                            ));
                                    }
                                    // Step 21b: writable:true for non-configurable non-writable target
                                    if td.is_data_descriptor()
                                        && td.writable == Some(false)
                                        && result_desc.writable == Some(true)
                                    {
                                        return Err(self.create_type_error(
                                                "'getOwnPropertyDescriptor' on proxy: trap returned descriptor with writable: true for non-configurable non-writable property in the proxy target",
                                            ));
                                    }
                                    // Enumerable must match for non-configurable target
                                    if result_desc.enumerable.is_some()
                                        && result_desc.enumerable != td.enumerable
                                    {
                                        return Err(self.create_type_error(
                                                "'getOwnPropertyDescriptor' on proxy: trap returned descriptor with incompatible enumerable for non-configurable property in the proxy target",
                                            ));
                                    }
                                    // Type mismatch: data vs accessor
                                    if td.is_data_descriptor() != result_desc.is_data_descriptor()
                                        && td.is_accessor_descriptor()
                                            != result_desc.is_accessor_descriptor()
                                    {
                                        return Err(self.create_type_error(
                                                "'getOwnPropertyDescriptor' on proxy: trap returned descriptor with different type than non-configurable property in the proxy target",
                                            ));
                                    }
                                    // Non-configurable accessor: getter/setter must match
                                    if td.is_accessor_descriptor()
                                        && result_desc.is_accessor_descriptor()
                                    {
                                        let td_get = td.get.as_ref();
                                        let rd_get = result_desc.get.as_ref();
                                        if rd_get.is_some()
                                            && !Self::same_value_option(td_get, rd_get)
                                        {
                                            return Err(self.create_type_error(
                                                    "'getOwnPropertyDescriptor' on proxy: trap returned descriptor with different getter for non-configurable property in the proxy target",
                                                ));
                                        }
                                        let td_set = td.set.as_ref();
                                        let rd_set = result_desc.set.as_ref();
                                        if rd_set.is_some()
                                            && !Self::same_value_option(td_set, rd_set)
                                        {
                                            return Err(self.create_type_error(
                                                    "'getOwnPropertyDescriptor' on proxy: trap returned descriptor with different setter for non-configurable property in the proxy target",
                                                ));
                                        }
                                    }
                                    // Non-configurable non-writable data: value must match
                                    if td.is_data_descriptor()
                                        && td.writable == Some(false)
                                        && result_desc.is_data_descriptor()
                                        && result_desc.writable != Some(true)
                                        && let (Some(tv), Some(rv)) =
                                            (&td.value, &result_desc.value)
                                        && !crate::interpreter::helpers::same_value(tv, rv)
                                    {
                                        return Err(self.create_type_error(
                                            "'getOwnPropertyDescriptor' on proxy: trap returned descriptor with different value for non-configurable non-writable property in the proxy target",
                                        ));
                                    }
                                    // Step 21b: non-configurable non-writable result but writable target
                                    if result_desc.is_data_descriptor()
                                        && result_desc.writable == Some(false)
                                        && td.writable == Some(true)
                                    {
                                        return Err(self.create_type_error(
                                                "'getOwnPropertyDescriptor' on proxy: trap returned non-configurable non-writable descriptor for a configurable or writable property in the proxy target",
                                            ));
                                    }
                                }
                            } else if !target_extensible {
                                return Err(self.create_type_error(
                                        "'getOwnPropertyDescriptor' on proxy: trap returned descriptor for property which does not exist in the non-extensible proxy target",
                                    ));
                            }
                            return Ok(self.from_property_descriptor(&result_desc));
                        }
                    }
                    Ok(v)
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_get_own_property_descriptor(t.id, key);
                    }
                    Ok(JsValue::Undefined)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object(obj_id) {
            // §10.4.6.4 [[GetOwnProperty]] step 4: namespace [[Get]] can throw for TDZ
            self.check_namespace_tdz(obj_id, key)?;
            let desc = obj.borrow().get_own_property(key);
            match desc {
                Some(d) => Ok(self.from_property_descriptor(&d)),
                None => Ok(JsValue::Undefined),
            }
        } else {
            Ok(JsValue::Undefined)
        }
    }

    /// Proxy-aware [[OwnPropertyKeys]] - checks proxy `ownKeys` trap, recurses on target if no trap.
    /// Returns all own property keys (for getOwnPropertyNames).
    pub(crate) fn proxy_own_keys(&mut self, obj_id: u64) -> Result<Vec<JsValue>, JsValue> {
        const MAX_PROXY_OWNKEYS_RESULT_LEN: usize = 1_000_000;

        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            match self.invoke_proxy_trap(obj_id, "ownKeys", vec![target_val.clone()]) {
                Ok(Some(v)) => {
                    if !matches!(v, JsValue::Object(_)) {
                        return Err(
                            self.create_type_error("CreateListFromArrayLike called on non-object")
                        );
                    }
                    if let JsValue::Object(arr) = &v {
                        let arr_id = arr.id;
                        // Use [[Get]] for length (spec: CreateListFromArrayLike)
                        let len_val = match self.get_object_property(arr_id, "length", &v) {
                            Completion::Normal(lv) => lv,
                            Completion::Throw(e) => return Err(e),
                            _ => JsValue::Undefined,
                        };
                        let len = match len_val {
                            JsValue::Number(n) if n.is_finite() && n > 0.0 => {
                                let len = n.floor() as usize;
                                if len > MAX_PROXY_OWNKEYS_RESULT_LEN {
                                    return Err(self.create_type_error(
                                        "'ownKeys' on proxy: trap result length exceeds supported limit",
                                    ));
                                }
                                len
                            }
                            JsValue::Number(_) => 0,
                            _ => {
                                return Err(self.create_type_error(
                                    "ownKeys trap result length is not a number",
                                ));
                            }
                        };
                        // Use [[Get]] for each element
                        let mut keys: Vec<JsValue> = Vec::with_capacity(len);
                        for i in 0..len {
                            let elem = match self.get_object_property(arr_id, &i.to_string(), &v) {
                                Completion::Normal(ev) => ev,
                                Completion::Throw(e) => return Err(e),
                                _ => JsValue::Undefined,
                            };
                            keys.push(elem);
                        }
                        for key in &keys {
                            if !matches!(key, JsValue::String(_) | JsValue::Symbol(_)) {
                                return Err(self.create_type_error(
                                    "'ownKeys' on proxy: trap returned non-string/symbol key",
                                ));
                            }
                        }
                        let mut seen = HashSet::new();
                        for key in &keys {
                            let key_str = to_property_key_string(key);
                            if !seen.insert(key_str) {
                                return Err(self.create_type_error(
                                    "'ownKeys' on proxy: trap returned duplicate entries",
                                ));
                            }
                        }
                        self.validate_ownkeys_invariant(&v, &target_val)?;
                        Ok(keys)
                    } else {
                        Ok(vec![])
                    }
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_own_keys(t.id);
                    }
                    Ok(vec![])
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object_cell(obj_id) {
            // Deferred namespace: trigger evaluation on [[OwnPropertyKeys]]
            {
                let is_deferred_ns = obj
                    .borrow()
                    .module_namespace()
                    .as_ref()
                    .is_some_and(|ns| ns.deferred);
                if is_deferred_ns {
                    self.ensure_deferred_namespace_evaluation(obj_id)?;
                }
            }
            // OrdinaryOwnPropertyKeys: integer indices (sorted), then string keys (in creation order), then symbol keys
            let obj = self.get_object(obj_id).unwrap();
            let b = obj.borrow();

            // String exotic objects (§10.4.3.3): virtual char indices included
            let is_string_wrapper =
                b.class_name == "String" && matches!(b.primitive_value, Some(JsValue::String(_)));
            let string_len = if is_string_wrapper {
                if let Some(JsValue::String(ref s)) = b.primitive_value {
                    s.code_units.len()
                } else {
                    0
                }
            } else {
                0
            };

            let mut int_keys_set: std::collections::BTreeMap<u64, String> =
                std::collections::BTreeMap::new();
            let mut str_keys: Vec<JsPropertyKey> = Vec::new();
            let mut sym_keys: Vec<JsPropertyKey> = Vec::new();

            // String exotic: char indices 0..len are virtual integer indices
            if is_string_wrapper {
                for i in 0..string_len {
                    int_keys_set.insert(i as u64, i.to_string());
                }
            }

            // TypedArray [[OwnPropertyKeys]]: virtual integer indices
            if let Some(ta) = b.typed_array_info() {
                use crate::interpreter::types::{is_typed_array_out_of_bounds, typed_array_length};
                if !is_typed_array_out_of_bounds(ta) {
                    let len = typed_array_length(ta);
                    for i in 0..len {
                        int_keys_set.insert(i as u64, i.to_string());
                    }
                }
            }

            if let Some(elems) = b.array_elements() {
                for (i, value) in elems.iter().enumerate() {
                    if matches!(value, JsValue::Undefined) || i > 0xFFFF_FFFE {
                        continue;
                    }
                    int_keys_set.insert(i as u64, i.to_string());
                }
            }

            for k in &b.property_order {
                if k.starts_with("Symbol(") {
                    sym_keys.push(k.clone());
                } else if let Ok(n) = k.parse::<u64>() {
                    if k.eq_str(&n.to_string()) {
                        // This is an integer index - add/overwrite (string char indices take precedence, but we let btreemap handle uniqueness)
                        int_keys_set.insert(n, k.to_string());
                    } else {
                        str_keys.push(k.clone());
                    }
                } else {
                    // Skip "length" for string wrappers - it's virtual, added separately
                    if is_string_wrapper && k.eq_str("length") {
                        continue;
                    }
                    str_keys.push(k.clone());
                }
            }

            let mut result: Vec<JsValue> = Vec::new();
            for (_, k) in int_keys_set {
                result.push(JsValue::String(JsString::from_str(&k)));
            }
            for k in str_keys {
                result.push(JsValue::String(k.to_js_string()));
            }
            // String exotic: "length" is a virtual non-enumerable string key (after other str keys, before symbols)
            if is_string_wrapper {
                result.push(JsValue::String(JsString::from_str("length")));
            }
            for k in sym_keys {
                result.push(self.symbol_key_to_jsvalue(&k));
            }
            Ok(result)
        } else {
            Ok(vec![])
        }
    }

    /// Proxy-aware enumerable keys with prototype chain walk for for-in loops.
    pub(crate) fn proxy_enumerable_keys_with_proto(
        &mut self,
        obj_id: u64,
    ) -> Result<Vec<JsPropertyKey>, JsValue> {
        let mut seen = HashSet::new();
        let mut keys = Vec::new();
        let mut current_id = Some(obj_id);

        while let Some(cid) = current_id {
            // Get own keys for current object (proxy-aware)
            let own_keys = self.proxy_own_keys(cid)?;
            for key in &own_keys {
                if let JsValue::String(s) = key {
                    let key_str = JsPropertyKey::from_js_string(s);
                    if key_str.starts_with("Symbol(") {
                        continue;
                    }
                    if seen.contains(&key_str) {
                        continue;
                    }
                    // Check enumerability via proxy-aware [[GetOwnProperty]]
                    let desc_val = self.proxy_get_own_property_descriptor(cid, &key_str)?;
                    seen.insert(key_str.clone());
                    if !matches!(desc_val, JsValue::Undefined)
                        && let Ok(desc) = self.to_property_descriptor(&desc_val)
                        && desc.enumerable != Some(false)
                    {
                        keys.push(key_str);
                    }
                }
            }

            // Walk prototype chain (proxy-aware)
            match self.proxy_get_prototype_of(cid) {
                Ok(JsValue::Object(proto_ref)) => {
                    current_id = Some(proto_ref.id);
                }
                Ok(_) => current_id = None,
                Err(e) => return Err(e),
            }
        }
        Ok(keys)
    }

    /// Proxy-aware [[GetPrototypeOf]] - checks proxy `getPrototypeOf` trap, recurses on target if no trap.
    pub(crate) fn proxy_get_prototype_of(&mut self, obj_id: u64) -> Result<JsValue, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            match self.invoke_proxy_trap(obj_id, "getPrototypeOf", vec![target_val.clone()]) {
                Ok(Some(v)) => {
                    if !matches!(v, JsValue::Object(_) | JsValue::Null) {
                        return Err(self.create_type_error(
                            "'getPrototypeOf' on proxy: trap returned neither object nor null",
                        ));
                    }
                    // Step 8: extensibleTarget = ? IsExtensible(target)
                    if let JsValue::Object(ref t) = target_val {
                        let extensible_target = self.proxy_is_extensible(t.id)?;
                        if !extensible_target {
                            // Step 10: targetProto = ? target.[[GetPrototypeOf]]()
                            let actual_proto = self.proxy_get_prototype_of(t.id)?;
                            let same = match (&v, &actual_proto) {
                                (JsValue::Object(a), JsValue::Object(b)) => a.id == b.id,
                                (JsValue::Null, JsValue::Null) => true,
                                _ => false,
                            };
                            if !same {
                                return Err(self.create_type_error(
                                    "'getPrototypeOf' on proxy: proxy target is non-extensible but the trap did not return its actual prototype",
                                ));
                            }
                        }
                    }
                    Ok(v)
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_get_prototype_of(t.id);
                    }
                    Ok(JsValue::Null)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object_cell(obj_id) {
            if let Some(id) = obj.borrow().prototype_id {
                Ok(JsValue::Object(crate::types::JsObject { id }))
            } else {
                Ok(JsValue::Null)
            }
        } else {
            Ok(JsValue::Null)
        }
    }

    /// Proxy-aware [[SetPrototypeOf]] - checks proxy `setPrototypeOf` trap, recurses on target if no trap.
    pub(crate) fn proxy_set_prototype_of(
        &mut self,
        obj_id: u64,
        proto: &JsValue,
    ) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            match self.invoke_proxy_trap(
                obj_id,
                "setPrototypeOf",
                vec![target_val.clone(), proto.clone()],
            ) {
                Ok(Some(v)) => {
                    if !self.to_boolean_val(&v) {
                        return Ok(false);
                    }
                    // Step 8: IsExtensible(target) — may throw, must use proxy-aware check
                    let target_id = if let JsValue::Object(ref t) = target_val {
                        t.id
                    } else {
                        return Ok(true);
                    };
                    let extensible_target = self.proxy_is_extensible(target_id)?;
                    // Step 9: if extensible, no invariant to check
                    if extensible_target {
                        return Ok(true);
                    }
                    // Step 10: GetPrototypeOf(target) — may throw
                    let actual_proto = self.proxy_get_prototype_of(target_id)?;
                    let same = match (proto, &actual_proto) {
                        (JsValue::Object(a), JsValue::Object(b)) => a.id == b.id,
                        (JsValue::Null, JsValue::Null) => true,
                        _ => false,
                    };
                    if !same {
                        return Err(self.create_type_error(
                            "'setPrototypeOf' on proxy: trap returned truish for setting a new prototype on the non-extensible proxy target",
                        ));
                    }
                    Ok(true)
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_set_prototype_of(t.id, proto);
                    }
                    Ok(true)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object_cell(obj_id) {
            // OrdinarySetPrototypeOf
            let current_proto_id = obj.borrow().prototype_id;
            let new_proto_id = if let JsValue::Object(p) = proto {
                Some(p.id)
            } else {
                None
            };
            let same = (matches!(proto, JsValue::Null) && current_proto_id.is_none())
                || matches!((new_proto_id, current_proto_id), (Some(a), Some(b)) if a == b);
            if same {
                return Ok(true);
            }
            if !obj.borrow().extensible {
                return Ok(false);
            }
            // Cycle check
            if let JsValue::Object(p) = proto {
                let mut check_id = Some(p.id);
                while let Some(cid) = check_id {
                    if cid == obj_id {
                        return Ok(false);
                    }
                    check_id = self
                        .get_object_cell(cid)
                        .and_then(|o| o.borrow().prototype_id.as_ref().copied());
                }
            }
            match proto {
                JsValue::Null => {
                    obj.borrow_mut().prototype_id = None;
                }
                JsValue::Object(p) => {
                    if let Some(po) = self.get_object_cell(p.id) {
                        obj.borrow_mut().prototype_id = Some(po.borrow().id.unwrap());
                    }
                }
                _ => {}
            }
            Ok(true)
        } else {
            Ok(true)
        }
    }

    /// Proxy-aware [[IsExtensible]] - checks proxy `isExtensible` trap, recurses on target if no trap.
    pub(crate) fn proxy_is_extensible(&mut self, obj_id: u64) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            match self.invoke_proxy_trap(obj_id, "isExtensible", vec![target_val.clone()]) {
                Ok(Some(v)) => {
                    let trap_result = self.to_boolean_val(&v);
                    if let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object_cell(t.id)
                    {
                        let target_extensible = tobj.borrow().extensible;
                        if trap_result != target_extensible {
                            return Err(self.create_type_error(
                                "'isExtensible' on proxy: trap result does not reflect extensibility of proxy target",
                            ));
                        }
                    }
                    Ok(trap_result)
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_is_extensible(t.id);
                    }
                    Ok(false)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object_cell(obj_id) {
            Ok(obj.borrow().extensible)
        } else {
            Ok(false)
        }
    }

    /// Proxy-aware [[PreventExtensions]] - checks proxy `preventExtensions` trap, recurses on target if no trap.
    pub(crate) fn proxy_prevent_extensions(&mut self, obj_id: u64) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            match self.invoke_proxy_trap(obj_id, "preventExtensions", vec![target_val.clone()]) {
                Ok(Some(v)) => {
                    let trap_result = self.to_boolean_val(&v);
                    if trap_result
                        && let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object_cell(t.id)
                        && tobj.borrow().extensible
                    {
                        return Err(self.create_type_error(
                                "'preventExtensions' on proxy: trap returned truish but the proxy target is extensible",
                            ));
                    }
                    Ok(trap_result)
                }
                Ok(None) => {
                    if let JsValue::Object(ref t) = target_val {
                        return self.proxy_prevent_extensions(t.id);
                    }
                    Ok(false)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object_cell(obj_id) {
            obj.borrow_mut().extensible = false;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Proxy-aware [[DefineOwnProperty]] - checks proxy `defineProperty` trap, recurses on target if no trap.
/// IsCompatiblePropertyDescriptor (§10.1.6.3)
fn is_compatible_property_desc(
    _extensible: bool,
    desc: &PropertyDescriptor,
    current: &PropertyDescriptor,
) -> bool {
    // Step 3: If current.[[Configurable]] is false:
    if current.configurable == Some(false) {
        // 3a: If Desc.[[Configurable]] is true, return false
        if desc.configurable == Some(true) {
            return false;
        }
        // 3b: If Desc has [[Enumerable]] and it differs from current
        if let Some(desc_enum) = desc.enumerable
            && current.enumerable != Some(desc_enum)
        {
            return false;
        }
    }
    // Step 4: If IsGenericDescriptor(Desc) is true, return true
    let is_generic = !desc.is_data_descriptor() && !desc.is_accessor_descriptor();
    if is_generic {
        return true;
    }
    // Step 5: If IsDataDescriptor(current) != IsDataDescriptor(Desc)
    if current.is_data_descriptor() != desc.is_data_descriptor() {
        // 5a: If current.[[Configurable]] is false, return false
        if current.configurable == Some(false) {
            return false;
        }
        return true;
    }
    // Step 6: Both are data descriptors
    if current.is_data_descriptor() && desc.is_data_descriptor() {
        if current.configurable == Some(false) && current.writable == Some(false) {
            // 6a.i: If Desc.[[Writable]] is true, return false
            if desc.writable == Some(true) {
                return false;
            }
            // 6a.ii: If Desc has [[Value]] and SameValue(Desc.[[Value]], current.[[Value]]) is false
            if let Some(ref desc_val) = desc.value {
                let current_val = current.value.as_ref().unwrap_or(&JsValue::Undefined);
                if !same_value(desc_val, current_val) {
                    return false;
                }
            }
        }
        return true;
    }
    // Step 7: Both are accessor descriptors
    if current.configurable == Some(false) {
        // 7a.i: If Desc has [[Set]] and SameValue(Desc.[[Set]], current.[[Set]]) is false
        if let Some(ref desc_set) = desc.set {
            let current_set = current.set.as_ref().unwrap_or(&JsValue::Undefined);
            if !same_value(desc_set, current_set) {
                return false;
            }
        }
        // 7a.ii: If Desc has [[Get]] and SameValue(Desc.[[Get]], current.[[Get]]) is false
        if let Some(ref desc_get) = desc.get {
            let current_get = current.get.as_ref().unwrap_or(&JsValue::Undefined);
            if !same_value(desc_get, current_get) {
                return false;
            }
        }
    }
    true
}
