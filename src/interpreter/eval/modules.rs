use super::*;

impl Interpreter {
    /// Resolve a module export value by following re-export chains recursively.
    /// Used by namespace [[Get]] to dynamically resolve live bindings.
    pub(super) fn resolve_module_export_value(
        &mut self,
        binding_name: &str,
        env: &crate::interpreter::types::EnvRef,
        module_path: Option<&std::path::Path>,
        original_key: &str,
    ) -> Result<JsValue, JsValue> {
        self.resolve_module_export_value_inner(
            binding_name,
            env,
            module_path,
            original_key,
            &mut HashSet::default(),
        )
    }

    pub(super) fn resolve_module_export_value_inner(
        &mut self,
        binding_name: &str,
        env: &crate::interpreter::types::EnvRef,
        module_path: Option<&std::path::Path>,
        original_key: &str,
        visited: &mut HashSet<(std::path::PathBuf, String)>,
    ) -> Result<JsValue, JsValue> {
        if let Some(mp) = module_path {
            let key = (mp.to_path_buf(), binding_name.to_string());
            if visited.contains(&key) {
                return Err(self.create_reference_error(&format!(
                    "Cannot access '{}' before initialization",
                    original_key
                )));
            }
            visited.insert(key);
        }

        // Handle *ns: bindings (namespace re-export)
        if binding_name.starts_with("*ns:") {
            if let Some(val) = env.borrow().get(original_key) {
                return Ok(val);
            }
            if let Some(mp) = module_path
                && let Some(module) = self.module_registry.get(mp)
                && let Some(val) = module.borrow().exports.get(original_key)
            {
                return Ok(val.clone());
            }
            return Ok(JsValue::Undefined);
        }

        // Handle *reexport:source:name — follow the chain recursively
        if let Some(rest) = binding_name.strip_prefix("*reexport:") {
            if let Some(colon_idx) = rest.rfind(':') {
                let source = &rest[..colon_idx];
                let export_name = &rest[colon_idx + 1..];
                if let Some(mp) = module_path
                    && let Ok(resolved) = self.resolve_module_specifier(source, Some(mp))
                    && let Ok(source_mod) = self.load_module(&resolved)
                {
                    let source_ref = source_mod.borrow();
                    let source_env = source_ref.env.clone();
                    let source_path = source_ref.path.clone();
                    // Look up what this export resolves to in the source module
                    if let Some(next_binding) = source_ref.export_bindings.get(export_name) {
                        let next_binding = next_binding.clone();
                        drop(source_ref);
                        return self.resolve_module_export_value_inner(
                            &next_binding,
                            &source_env,
                            Some(&source_path),
                            original_key,
                            visited,
                        );
                    }
                    drop(source_ref);
                    // No binding info — try direct env lookup
                    if source_env.borrow().is_in_tdz(export_name) {
                        return Err(self.create_reference_error(&format!(
                            "Cannot access '{}' before initialization",
                            original_key
                        )));
                    }
                    if let Some(val) = source_env.borrow().get(export_name) {
                        return Ok(val);
                    }
                }
            }
            return Ok(JsValue::Undefined);
        }

        // Local binding: look up in the provided environment
        if env.borrow().is_in_tdz(binding_name) {
            return Err(self.create_reference_error(&format!(
                "Cannot access '{}' before initialization",
                original_key
            )));
        }
        if let Some(val) = env.borrow().get(binding_name) {
            return Ok(val);
        }

        Ok(JsValue::Undefined)
    }

    /// Check if accessing `key` on a module namespace object would hit TDZ.
    /// Returns Err(ReferenceError) if the binding is uninitialized.
    /// Returns Ok(()) if the key is safe to access or the object is not a namespace.
    pub(crate) fn check_namespace_tdz(&mut self, obj_id: u64, key: &str) -> Result<(), JsValue> {
        if key.starts_with("Symbol(") {
            return Ok(());
        }
        let ns_data = if let Some(obj) = self.get_object(obj_id) {
            obj.borrow().module_namespace.clone()
        } else {
            None
        };
        if let Some(ns_data) = ns_data {
            // Deferred namespaces: trigger evaluation on non-symbol-like key access
            if ns_data.deferred && !Self::is_symbol_like_namespace_key(key, true) {
                self.ensure_deferred_namespace_evaluation(obj_id)?;
            }
            if let Some(binding_name) = ns_data.export_to_binding.get(key) {
                if binding_name.starts_with("*ns:") {
                    return Ok(());
                }
                if let Some(rest) = binding_name.strip_prefix("*reexport:") {
                    // Check TDZ in source module's environment
                    if let Some(colon_idx) = rest.rfind(':') {
                        let source = &rest[..colon_idx];
                        let export_name = &rest[colon_idx + 1..];
                        if let Some(ref module_path) = ns_data.module_path
                            && let Ok(resolved) =
                                self.resolve_module_specifier(source, Some(module_path))
                            && let Ok(source_mod) = self.load_module(&resolved)
                        {
                            let source_ref = source_mod.borrow();
                            if let Some(binding) = source_ref.export_bindings.get(export_name) {
                                if source_ref.env.borrow().is_in_tdz(binding) {
                                    return Err(self.create_reference_error(&format!(
                                        "Cannot access '{key}' before initialization"
                                    )));
                                }
                            } else if source_ref.env.borrow().is_in_tdz(export_name) {
                                return Err(self.create_reference_error(&format!(
                                    "Cannot access '{key}' before initialization"
                                )));
                            }
                        }
                    }
                    return Ok(());
                }
                if ns_data.env.borrow().is_in_tdz(binding_name) {
                    return Err(self.create_reference_error(&format!(
                        "Cannot access '{key}' before initialization"
                    )));
                }
            }
        }
        Ok(())
    }

    /// IsSymbolLikeNamespaceKey(P, O): true if P is a Symbol, or deferred + "then"
    pub fn is_symbol_like_namespace_key(key: &str, deferred: bool) -> bool {
        key.starts_with("Symbol(") || (deferred && key == "then")
    }

    /// Trigger evaluation of a deferred module namespace when a non-symbol-like key is accessed.
    pub(crate) fn ensure_deferred_namespace_evaluation(
        &mut self,
        obj_id: u64,
    ) -> Result<(), JsValue> {
        let (deferred, module_path) = if let Some(obj) = self.get_object(obj_id) {
            let b = obj.borrow();
            if let Some(ref ns) = b.module_namespace {
                (ns.deferred, ns.module_path.clone())
            } else {
                return Ok(());
            }
        } else {
            return Ok(());
        };

        if !deferred {
            return Ok(());
        }

        let module_path = match module_path {
            Some(p) => p,
            None => return Ok(()),
        };

        let module = match self.module_registry.get(&module_path).cloned() {
            Some(m) => m,
            None => return Ok(()),
        };

        if module.borrow().evaluated {
            // Already evaluated — just clear deferred flag
            if let Some(ref err) = module.borrow().error {
                return Err(err.clone());
            }
            if let Some(obj) = self.get_object(obj_id)
                && let Some(ref mut ns) = obj.borrow_mut().module_namespace
            {
                ns.deferred = false;
            }
            return Ok(());
        }

        // Check ReadyForSyncExecution
        let mut seen = HashSet::default();
        if !self.ready_for_sync_execution(&module_path, &mut seen) {
            return Err(self.create_type_error(
                "Cannot synchronously evaluate a module with top-level await or that is currently being evaluated",
            ));
        }

        // Save and set current_module_path for evaluation
        let prev_path = self.current_module_path.take();
        self.current_module_path = Some(module_path.clone());
        let mut stack = vec![];
        let result = self.inner_module_evaluation(&module_path, &mut stack, 0);
        self.current_module_path = prev_path;

        match result {
            Ok(_) => {
                if let Some(obj) = self.get_object(obj_id)
                    && let Some(ref mut ns) = obj.borrow_mut().module_namespace
                {
                    ns.deferred = false;
                }
                Ok(())
            }
            Err(ref e) => {
                // Per spec §16.2.1.5.3 step 9: mark all modules on stack as evaluated with error
                for m_path in &stack {
                    if let Some(m) = self.module_registry.get(m_path) {
                        let mut mb = m.borrow_mut();
                        mb.evaluated = true;
                        mb.is_evaluating = false;
                        if mb.error.is_none() {
                            mb.error = Some(e.clone());
                        }
                    }
                }
                Err(e.clone())
            }
        }
    }

    pub(crate) fn get_object_property(
        &mut self,
        obj_id: u64,
        key: &str,
        this_val: &JsValue,
    ) -> Completion {
        // Single-borrow fast path: classify object and check own property in one borrow
        if let Some(obj) = self.get_object(obj_id) {
            let b = obj.borrow();
            let is_proxy = b.proxy_target.is_some() || b.proxy_revoked;
            let has_module_ns = b.module_namespace.is_some();

            if !is_proxy && !has_module_ns {
                // Fast path for ordinary objects (the common case)
                let is_ta = b.typed_array_info.is_some();

                // TypedArray: canonical numeric index strings must not walk prototype
                if is_ta
                    && let Some(index) =
                        crate::interpreter::types::canonical_numeric_index_string(key)
                {
                    use crate::interpreter::types::{
                        is_valid_integer_index, typed_array_get_index,
                    };
                    let ta = b.typed_array_info.as_ref().unwrap();
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

    fn get_object_property_slow(
        &mut self,
        obj_id: u64,
        key: &str,
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
                        && let Some(tobj) = self.get_object(t.id)
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
                .get_object(obj_id)
                .and_then(|obj| obj.borrow().module_namespace.clone());
            if let Some(ns_data) = ns_data {
                // Deferred namespace: IsSymbolLikeNamespaceKey check
                if ns_data.deferred
                    && !Self::is_symbol_like_namespace_key(key, true)
                    && let Err(e) = self.ensure_deferred_namespace_evaluation(obj_id)
                {
                    return Completion::Throw(e);
                }
                if let Some(binding_name) = ns_data.export_to_binding.get(key) {
                    let module_path = ns_data.module_path.clone();
                    match self.resolve_module_export_value(
                        binding_name,
                        &ns_data.env,
                        module_path.as_deref(),
                        key,
                    ) {
                        Ok(val) => return Completion::Normal(val),
                        Err(e) => return Completion::Throw(e),
                    }
                }
                // Fallback: check module's exports directly
                if let Some(ref module_path) = ns_data.module_path
                    && let Some(module) = self.module_registry.get(module_path)
                    && let Some(val) = module.borrow().exports.get(key)
                {
                    return Completion::Normal(val.clone());
                }
            }
        }

        // Fallback: own property + prototype chain (for rare non-proxy, non-module cases)
        let own_desc = if let Some(obj) = self.get_object(obj_id) {
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
                let proto = if let Some(obj) = self.get_object(obj_id) {
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
    pub(crate) fn proxy_has_property(&mut self, obj_id: u64, key: &str) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            let key_val = self.symbol_key_to_jsvalue(key);
            match self.invoke_proxy_trap(obj_id, "has", vec![target_val.clone(), key_val]) {
                Ok(Some(v)) => {
                    let trap_result = self.to_boolean_val(&v);
                    if !trap_result
                        && let JsValue::Object(ref t) = target_val
                        && let Some(tobj) = self.get_object(t.id)
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
                    .module_namespace
                    .as_ref()
                    .is_some_and(|ns| ns.deferred);
                if is_deferred_ns && !Self::is_symbol_like_namespace_key(key, true) {
                    self.ensure_deferred_namespace_evaluation(obj_id)?;
                }
            }
            // TypedArray §10.4.5.3 [[HasProperty]]: numeric indices handled by IsValidIntegerIndex only
            {
                let b = obj.borrow();
                if b.typed_array_info.is_some()
                    && let Some(index) =
                        crate::interpreter::types::canonical_numeric_index_string(key)
                {
                    return Ok(is_valid_integer_index(
                        b.typed_array_info.as_ref().unwrap(),
                        index,
                    ));
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

    // === With-scope reference semantics (spec-compliant) ===

    /// Dynamically fetch @@unscopables from `obj_id` and check if `name` is blocked.
    pub(super) fn check_unscopables_dynamic(
        &mut self,
        obj_id: u64,
        name: &str,
    ) -> Result<bool, JsValue> {
        let unscopables_val = {
            let this_val = JsValue::Object(crate::types::JsObject { id: obj_id });
            let key = "Symbol(Symbol.unscopables)";
            match self.get_object_property(obj_id, key, &this_val) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::Undefined,
            }
        };
        if let JsValue::Object(u_ref) = &unscopables_val {
            let u_this = unscopables_val.clone();
            match self.get_object_property(u_ref.id, name, &u_this) {
                Completion::Normal(v) => Ok(self.to_boolean_val(&v)),
                Completion::Throw(e) => Err(e),
                _ => Ok(false),
            }
        } else {
            Ok(false)
        }
    }

    /// HasBinding for with-scopes: walks env chain, for each with-scope checks
    /// proxy_has_property + check_unscopables_dynamic. Returns Ok(Some(obj_id)) if
    /// the name resolves to a with-object, Ok(None) if found in a regular binding
    /// or not found at all, Err on trap error.
    pub(crate) fn resolve_with_has_binding(
        &mut self,
        name: &str,
        env: &EnvRef,
    ) -> Result<Option<u64>, JsValue> {
        let mut current = Some(env.clone());
        while let Some(env_ref) = current {
            let env_borrow = env_ref.borrow();
            if let Some(ref with) = env_borrow.with_object {
                let obj_id = with.obj_id;
                drop(env_borrow);
                match self.proxy_has_property(obj_id, name) {
                    Ok(true) => {
                        if !self.check_unscopables_dynamic(obj_id, name)? {
                            return Ok(Some(obj_id));
                        }
                    }
                    Ok(false) => {}
                    Err(e) => return Err(e),
                }
                let env_borrow = env_ref.borrow();
                current = env_borrow.parent.clone();
                continue;
            }
            if env_borrow.bindings.contains_key(name) {
                return Ok(None);
            }
            if env_borrow.global_object.is_some() {
                return Ok(None);
            }
            current = env_borrow.parent.clone();
        }
        Ok(None)
    }

    /// GetBindingValue for a known with-object: checks HasProperty(stillExists) + Get.
    /// No unscopables check (already done in HasBinding).
    pub(super) fn with_get_binding_value(
        &mut self,
        obj_id: u64,
        name: &str,
        strict: bool,
    ) -> Completion {
        match self.proxy_has_property(obj_id, name) {
            Ok(true) => {
                let this_val = JsValue::Object(crate::types::JsObject { id: obj_id });
                self.get_object_property(obj_id, name, &this_val)
            }
            Ok(false) => {
                if strict {
                    Completion::Throw(
                        self.create_reference_error(&format!("{name} is not defined")),
                    )
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            Err(e) => Completion::Throw(e),
        }
    }

    /// SetMutableBinding for a known with-object: checks HasProperty(stillExists) + Set.
    /// No unscopables check (already done in HasBinding).
    pub(crate) fn with_set_mutable_binding(
        &mut self,
        obj_id: u64,
        name: &str,
        value: JsValue,
        strict: bool,
    ) -> Result<(), JsValue> {
        match self.proxy_has_property(obj_id, name) {
            Ok(true) => {
                let receiver = JsValue::Object(crate::types::JsObject { id: obj_id });
                let success = self.proxy_set(obj_id, name, value, &receiver)?;
                if !success && strict {
                    return Err(self.create_type_error(&format!(
                        "Cannot assign to read only property '{name}'"
                    )));
                }
                Ok(())
            }
            Ok(false) => {
                if strict {
                    Err(self.create_reference_error(&format!("{name} is not defined")))
                } else {
                    let receiver = JsValue::Object(crate::types::JsObject { id: obj_id });
                    self.proxy_set(obj_id, name, value, &receiver)?;
                    Ok(())
                }
            }
            Err(e) => Err(e),
        }
    }

    pub(super) fn dynamic_import(
        &mut self,
        specifier: &str,
        import_type: Option<super::ImportModuleType>,
    ) -> Completion {
        let resolved =
            match self.resolve_module_specifier(specifier, self.current_module_path.as_deref()) {
                Ok(p) => p,
                Err(e) => {
                    return self.create_rejected_promise(e);
                }
            };

        // Text/bytes synthetic modules: load and resolve immediately
        if let Some(ref itype) = import_type {
            let module = match itype {
                super::ImportModuleType::Text => self.load_text_module(&resolved),
                super::ImportModuleType::Bytes => self.load_bytes_module(&resolved),
            };
            return match module {
                Ok(m) => {
                    let ns = self.create_module_namespace(&m);
                    self.create_resolved_promise(ns)
                }
                Err(e) => self.create_rejected_promise(e),
            };
        }

        // If we're NOT inside a static module load, load synchronously
        if self.static_module_load_depth == 0 {
            let module = match self.load_module(&resolved) {
                Ok(m) => m,
                Err(e) => {
                    return self.create_rejected_promise(e);
                }
            };
            let resolved_canon = resolved.canonicalize().unwrap_or(resolved.clone());
            let mut stack = vec![];
            if let Err(ref e) = self.inner_module_evaluation(&resolved_canon, &mut stack, 0) {
                for m_path in &stack {
                    if let Some(m) = self.module_registry.get(m_path) {
                        let mut mb = m.borrow_mut();
                        mb.evaluated = true;
                        mb.is_evaluating = false;
                        if mb.error.is_none() {
                            mb.error = Some(e.clone());
                        }
                    }
                }
                return self.create_rejected_promise(e.clone());
            }
            let (resolve, reject, promise) = self.create_promise_parts();
            return self.settle_dynamic_import_promise(&module, promise, resolve, reject);
        }

        // Inside static module evaluation — defer to microtask so we don't
        // preempt the current module's DFS evaluation order
        let (resolve, reject, promise) = self.create_promise_parts();
        let resolved_path = resolved.clone();
        let promise_root = promise.clone();
        let promise_for_settle = promise.clone();
        let resolve_root = resolve.clone();
        let reject_root = reject.clone();

        self.microtask_queue.push((
            vec![promise_root, resolve_root.clone(), reject_root.clone()],
            Box::new(move |interp: &mut Interpreter| {
                match interp.load_module(&resolved_path) {
                    Ok(m) => {
                        let resolved_canon = resolved_path
                            .canonicalize()
                            .unwrap_or(resolved_path.clone());
                        let mut stack = vec![];
                        if let Err(ref e) =
                            interp.inner_module_evaluation(&resolved_canon, &mut stack, 0)
                        {
                            for m_path in &stack {
                                if let Some(m) = interp.module_registry.get(m_path) {
                                    let mut mb = m.borrow_mut();
                                    mb.evaluated = true;
                                    mb.is_evaluating = false;
                                    if mb.error.is_none() {
                                        mb.error = Some(e.clone());
                                    }
                                }
                            }
                            let _ = interp.call_function(
                                &reject_root,
                                &JsValue::Undefined,
                                std::slice::from_ref(e),
                            );
                            return Completion::Normal(JsValue::Undefined);
                        }
                        let _ = interp.settle_dynamic_import_promise(
                            &m,
                            promise_for_settle.clone(),
                            resolve_root.clone(),
                            reject_root.clone(),
                        );
                    }
                    Err(e) => {
                        let _ = interp.call_function(&reject_root, &JsValue::Undefined, &[e]);
                    }
                }
                Completion::Normal(JsValue::Undefined)
            }),
        ));

        Completion::Normal(promise)
    }

    fn settle_dynamic_import_promise(
        &mut self,
        module: &Rc<RefCell<LoadedModule>>,
        promise: JsValue,
        resolve: JsValue,
        reject: JsValue,
    ) -> Completion {
        if let Some(err) = module.borrow().error.clone() {
            let _ = self.call_function(&reject, &JsValue::Undefined, &[err]);
            return Completion::Normal(promise);
        }

        let evaluation_promise = match self.ensure_module_evaluation_promise(module) {
            Ok(promise) => promise,
            Err(err) => {
                let _ = self.call_function(&reject, &JsValue::Undefined, &[err]);
                return Completion::Normal(promise);
            }
        };

        if let Some(eval_promise) = evaluation_promise {
            let module_for_ns = module.clone();
            let on_fulfilled = self.create_function(JsFunction::native(
                "dynamicImportFulfilled".to_string(),
                1,
                move |interp, _this, _args| {
                    Completion::Normal(interp.create_module_namespace(&module_for_ns))
                },
            ));
            return self.perform_promise_then(
                &eval_promise,
                &on_fulfilled,
                &JsValue::Undefined,
                promise,
                resolve,
                reject,
            );
        }

        let ns = self.create_module_namespace(module);
        let _ = self.call_function(&resolve, &JsValue::Undefined, &[ns]);
        Completion::Normal(promise)
    }

    fn ensure_module_evaluation_promise(
        &mut self,
        module: &Rc<RefCell<LoadedModule>>,
    ) -> Result<Option<JsValue>, JsValue> {
        let root_path = {
            let m = module.borrow();
            m.cycle_root
                .clone()
                .unwrap_or_else(|| m.path.canonicalize().unwrap_or_else(|_| m.path.clone()))
        };
        let root_module = self
            .module_registry
            .get(&root_path)
            .cloned()
            .unwrap_or_else(|| module.clone());

        {
            let root = root_module.borrow();
            if let Some(err) = root.error.clone() {
                return Err(err);
            }
            if root.evaluated || root.async_evaluation_order.is_none() {
                return Ok(None);
            }
            if let Some((promise, _, _)) = root.top_level_capability.clone() {
                return Ok(Some(promise));
            }
        }

        let (resolve, reject, promise) = self.create_promise_parts();
        root_module.borrow_mut().top_level_capability = Some((promise.clone(), resolve, reject));
        Ok(Some(promise))
    }

    pub(crate) fn create_error_in_realm(
        &mut self,
        realm_id: usize,
        error_type: &str,
        msg: &str,
    ) -> JsValue {
        let old_realm = self.current_realm_id;
        self.current_realm_id = realm_id;
        let err = self.create_error(error_type, msg);
        self.current_realm_id = old_realm;
        err
    }

    pub(crate) fn get_wrapped_value(
        &mut self,
        caller_realm_id: usize,
        value: &JsValue,
    ) -> Result<JsValue, JsValue> {
        match value {
            JsValue::Undefined
            | JsValue::Null
            | JsValue::Boolean(_)
            | JsValue::Number(_)
            | JsValue::String(_)
            | JsValue::Symbol(_)
            | JsValue::BigInt(_) => Ok(value.clone()),
            JsValue::Object(_) => {
                if !self.is_callable(value) {
                    return Err(self.create_error_in_realm(
                        caller_realm_id,
                        "TypeError",
                        "ShadowRealm can only pass callable and primitive values across realm boundaries",
                    ));
                }
                self.wrapped_function_create(caller_realm_id, value)
            }
        }
    }

    pub(crate) fn wrapped_function_create(
        &mut self,
        caller_realm_id: usize,
        target_func: &JsValue,
    ) -> Result<JsValue, JsValue> {
        let old_realm = self.current_realm_id;
        self.current_realm_id = caller_realm_id;

        let func_obj = self.create_object();
        let func_id = func_obj.borrow().id.unwrap();
        let fp_id = self.realms[caller_realm_id].function_prototype;
        {
            let mut o = func_obj.borrow_mut();
            o.prototype_id = fp_id;
            o.class_name = "Function".to_string();
            o.callable = Some(JsFunction::native("".to_string(), 0, |_, _, _| {
                Completion::Normal(JsValue::Undefined)
            }));
            if let JsValue::Object(tf) = target_func {
                o.wrapped_target_function_id = Some(tf.id);
            }
            o.wrapped_caller_realm_id = Some(caller_realm_id);
        }
        self.function_realm_map.insert(func_id, caller_realm_id);

        self.current_realm_id = old_realm;

        // CopyNameAndLength (spec §10.4.2.4) — any error becomes TypeError from callerRealm
        let length_val = if let JsValue::Object(tf) = target_func {
            // HasOwnProperty via [[GetOwnProperty]] (invokes proxy trap if proxy)
            let has_own_length = match self.proxy_get_own_property_descriptor(tf.id, "length") {
                Ok(JsValue::Undefined) => false,
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
                    _ => JsValue::Number(0.0),
                }
            } else {
                JsValue::Number(0.0)
            }
        } else {
            JsValue::Number(0.0)
        };

        let computed_length = match length_val {
            JsValue::Number(n) => {
                if n == f64::INFINITY {
                    f64::INFINITY
                } else if n == f64::NEG_INFINITY || n < 0.0 {
                    0.0
                } else {
                    n.trunc().max(0.0)
                }
            }
            _ => 0.0,
        };

        let name_str = if let JsValue::Object(tf) = target_func {
            match self.get_object_property(tf.id, "name", target_func) {
                Completion::Normal(JsValue::String(s)) => s.to_string(),
                Completion::Normal(_) => String::new(),
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

        if let Some(obj) = self.get_object(func_id) {
            obj.borrow_mut().insert_property(
                "length".to_string(),
                PropertyDescriptor::data(JsValue::Number(computed_length), false, false, true),
            );
            obj.borrow_mut().insert_property(
                "name".to_string(),
                PropertyDescriptor::data(
                    JsValue::String(crate::types::JsString::from_str(&name_str)),
                    false,
                    false,
                    true,
                ),
            );
        }

        Ok(JsValue::Object(crate::types::JsObject { id: func_id }))
    }

    pub(crate) fn call_wrapped_function(
        &mut self,
        wrapper_id: u64,
        _this_val: &JsValue,
        args: &[JsValue],
    ) -> Completion {
        let (target, caller_realm_id) = {
            let obj = match self.get_object(wrapper_id) {
                Some(o) => o,
                None => {
                    return Completion::Throw(
                        self.create_type_error("WrappedFunction: missing target"),
                    );
                }
            };
            let b = obj.borrow();
            let target_id = match b.wrapped_target_function_id {
                Some(id) => id,
                None => {
                    return Completion::Throw(
                        self.create_type_error("WrappedFunction: missing target"),
                    );
                }
            };
            let caller_realm_id = b.wrapped_caller_realm_id.unwrap_or(self.current_realm_id);
            (
                JsValue::Object(crate::types::JsObject { id: target_id }),
                caller_realm_id,
            )
        };

        let target_realm_id = match self.get_function_realm(&target) {
            Ok(r) => r,
            Err(e) => return Completion::Throw(e),
        };

        // Wrap arguments into target realm
        let mut wrapped_args = Vec::with_capacity(args.len());
        for arg in args {
            match self.get_wrapped_value(target_realm_id, arg) {
                Ok(v) => wrapped_args.push(v),
                Err(_) => {
                    return Completion::Throw(self.create_error_in_realm(
                        caller_realm_id,
                        "TypeError",
                        "WrappedFunction: argument is not a primitive or callable",
                    ));
                }
            }
        }

        // Call target in its realm
        let old_realm = self.current_realm_id;
        self.current_realm_id = target_realm_id;
        let result = self.call_function(&target, &JsValue::Undefined, &wrapped_args);
        self.current_realm_id = old_realm;

        let result_val = match result {
            Completion::Normal(v) => v,
            Completion::Empty => JsValue::Undefined,
            _ => {
                return Completion::Throw(self.create_error_in_realm(
                    caller_realm_id,
                    "TypeError",
                    "WrappedFunction: error in target function",
                ));
            }
        };

        match self.get_wrapped_value(caller_realm_id, &result_val) {
            Ok(v) => Completion::Normal(v),
            Err(e) => Completion::Throw(e),
        }
    }

    pub(crate) fn perform_realm_eval(
        &mut self,
        source_text: &str,
        caller_realm_id: usize,
        eval_realm_id: usize,
    ) -> Completion {
        use crate::parser::Parser;

        let program = {
            let mut parser = match Parser::new(source_text) {
                Ok(p) => p,
                Err(_) => {
                    return Completion::Throw(self.create_error_in_realm(
                        caller_realm_id,
                        "SyntaxError",
                        "Invalid source text",
                    ));
                }
            };
            match parser.parse_program() {
                Ok(prog) => prog,
                Err(e) => {
                    return Completion::Throw(self.create_error_in_realm(
                        caller_realm_id,
                        "SyntaxError",
                        &e.to_string(),
                    ));
                }
            }
        };

        let old_realm = self.current_realm_id;
        self.current_realm_id = eval_realm_id;
        let is_strict = program.body_is_strict;
        // Per spec §B.3.6.2 PerformShadowRealmEval:
        // Strict: both var and lex are new strict envs (isolates everything)
        // Non-strict: var goes to global, lex is fresh child of global (isolates let/const)
        let global = self.realm().global_env.clone();
        let (var_env, lex_env) = if is_strict {
            let new_env = Environment::new_function_scope(Some(global));
            new_env.borrow_mut().strict = true;
            (new_env.clone(), new_env)
        } else {
            let lex_env = Environment::new(Some(global.clone()));
            (global, lex_env)
        };
        // Hoist var/function declarations to var_env
        if let Err(e) = self.eval_declaration_instantiation(
            &program.body,
            &var_env,
            &lex_env,
            is_strict,
            false,
            &lex_env,
        ) {
            self.current_realm_id = old_realm;
            return Completion::Throw(e);
        }
        // Execute body in lex_env
        self.call_stack_envs.push(lex_env.clone());
        let mut result = Completion::Empty;
        for stmt in &program.body {
            self.gc_safepoint();
            match self.exec_statement(stmt, &lex_env) {
                Completion::Normal(v) => result = Completion::Normal(v),
                Completion::Empty => {}
                other => {
                    result = other;
                    break;
                }
            }
        }
        self.call_stack_envs.pop();
        self.drain_microtasks();
        self.current_realm_id = old_realm;

        let result_val = match result {
            Completion::Normal(v) => v,
            Completion::Empty => JsValue::Undefined,
            _ => {
                return Completion::Throw(self.create_error_in_realm(
                    caller_realm_id,
                    "TypeError",
                    "ShadowRealm evaluate error",
                ));
            }
        };

        match self.get_wrapped_value(caller_realm_id, &result_val) {
            Ok(v) => Completion::Normal(v),
            Err(e) => Completion::Throw(e),
        }
    }
}
