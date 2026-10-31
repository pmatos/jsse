use super::*;

mod proxy_define;

/// Longest prototype chain the named-property fast paths walk before handing
/// the access to the generic path (which has its own cycle and depth handling).
const PROTO_WALK_LIMIT: usize = 16;

/// Formats `n` into `buf` and returns the digits as a `&str`, without a heap
/// allocation. Used on the dense-array numeric-read fast path, where an
/// owned `String` per read would defeat the point of the fast path.
fn format_u32_stack(n: u32, buf: &mut [u8; 10]) -> &str {
    let mut i = buf.len();
    let mut n = n;
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    // Safety of `unwrap`: every byte written above is one of b'0'..=b'9'.
    std::str::from_utf8(&buf[i..]).unwrap()
}

/// Result of [`Interpreter::named_get_fast`].
pub(super) enum NamedGet {
    Value(JsValue),
    Getter(JsValue),
    Slow,
}

/// The observable result of [[Set]].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SetOutcome {
    Rejected,
    Succeeded,
}

impl SetOutcome {
    pub(crate) fn succeeded(self) -> bool {
        self == Self::Succeeded
    }

    fn from_success(succeeded: bool) -> Self {
        if succeeded {
            Self::Succeeded
        } else {
            Self::Rejected
        }
    }
}

impl Interpreter {
    /// Advance the resource counter shared by Proxy-mediated prototype-chain
    /// walks. The counter is local to one internal-method operation, so user
    /// code that starts a nested operation gets an independent budget.
    pub(crate) fn advance_proxy_chain_depth(&mut self, depth: usize) -> Result<usize, JsValue> {
        if depth >= PROXY_CHAIN_DEPTH_LIMIT {
            return Err(self.create_error("RangeError", "Maximum call stack size exceeded"));
        }
        Ok(depth + 1)
    }

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
        self.set_object_property_with_outcome(obj_id, key, value, receiver)
            .map(SetOutcome::succeeded)
    }

    /// Canonical [[Set]] entry point retaining whether the operation itself
    /// reached a successful own data-property write on the Receiver. A plain
    /// Boolean cannot distinguish that path from a successful setter or Proxy
    /// trap, and inspecting the descriptor after user code returns is racy with
    /// a setter that redefines the property.
    pub(crate) fn set_object_property_with_outcome<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
        value: JsValue,
        receiver: &JsValue,
    ) -> Result<SetOutcome, JsValue> {
        // Module namespace exotic [[Set]] (§10.4.6.9) always rejects the set,
        // even though exported bindings have writable own data descriptors.
        if self
            .get_object_cell(obj_id)
            .is_some_and(|obj| obj.borrow().module_namespace().is_some())
        {
            return Ok(SetOutcome::Rejected);
        }
        self.proxy_set_with_outcome(obj_id, key, value, receiver)
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
        self.get_object_property_with_proxy_depth(obj_id, key, this_val, 0)
    }

    /// Returns the result of a computed numeric-index read when the receiver's
    /// object kind makes it safe to bypass `ToPropertyKey` and the property MOP.
    pub(super) fn numeric_index_fast_get(&self, obj_val: &JsValue, index: f64) -> Option<JsValue> {
        let obj_id = obj_val.as_object_id()?;
        let obj_rc = self.get_object_cell(obj_id)?;
        let obj_borrow = obj_rc.borrow();

        if let Some(ta) = obj_borrow.typed_array_info() {
            use crate::interpreter::types::{is_valid_integer_index, typed_array_get_index};

            if is_valid_integer_index(ta, index) {
                return Some(typed_array_get_index(ta, index as usize));
            }
            // Any canonical numeric index on a typed array that is out of
            // range returns undefined without walking the prototype chain.
            // Raw -0 must fall through: ToPropertyKey(-0) produces "0", so it
            // addresses element zero rather than the canonical string "-0".
            let trunc = index.trunc();
            if index == trunc && !index.is_nan() && !index.is_sign_negative() {
                return Some(JsValue::UNDEFINED);
            }
        }

        if let Some(elems) = obj_borrow.array_elements() {
            let trunc = index.trunc();
            // -0 reaches here too: ToPropertyKey(-0) is "0", element zero.
            // 2^32 - 1 and above are ordinary string keys, not elements.
            if index == trunc && (0.0..4_294_967_295.0).contains(&index) {
                let idx = index as usize;
                let may_hold_index_key = obj_borrow.properties.may_hold_index_key();
                match elems.get(idx) {
                    Some(value) if !value.is_undefined() => {
                        // No index-keyed descriptor exists, so none shadows the element.
                        if !may_hold_index_key {
                            return Some(value.clone());
                        }
                        let mut buf = [0u8; 10];
                        let key_str = format_u32_stack(idx as u32, &mut buf);
                        if !obj_borrow.properties.contains_key(key_str) {
                            return Some(value.clone());
                        }
                    }
                    // A hole, or an index past the backing vector: with no own
                    // index descriptor it is absent, and with nothing inherited
                    // that could answer an index [[Get]] it reads undefined.
                    _ => {
                        if !may_hold_index_key
                            && crate::interpreter::builtins::array_dense::proto_chain_without_index_keys(
                                self,
                                obj_borrow.prototype_id,
                            )
                        {
                            return Some(JsValue::UNDEFINED);
                        }
                    }
                }
            }
        }

        if matches!(
            obj_borrow.kind,
            ObjectKind::Ordinary | ObjectKind::Arguments(_)
        ) && index == index.trunc()
            && (0.0..4_294_967_295.0).contains(&index)
        {
            if let Some(map) = obj_borrow.parameter_map()
                && let Some(value) = map.value_at(index as usize)
            {
                return Some(value);
            }
            if obj_borrow.properties.may_hold_index_key() {
                let mut buf = [0u8; 10];
                let key_str = format_u32_stack(index as u32, &mut buf);
                if let Some(record) = obj_borrow.properties.get(key_str)
                    && let PropertyRead::Value(value) = record.read()
                {
                    return Some(value.clone());
                }
            }
        }

        None
    }

    /// Complete a computed numeric-index write that needs neither user code
    /// nor a property trap: a numeric store into an in-bounds numeric
    /// TypedArray element, or a store into a plain dense Array whose target
    /// element is present, or is a hole or the next index and no prototype
    /// can own it. Returns `false` with no side effect when the generic
    /// [[Set]] must run (accessors, Proxies, frozen or sealed arrays,
    /// read-only `length`, `undefined` values, inherited setters).
    #[inline(never)]
    pub(super) fn numeric_index_fast_set(
        &self,
        obj_val: &JsValue,
        index: f64,
        value: &JsValue,
    ) -> bool {
        self.numeric_index_fast_set_inlined(obj_val, index, value)
    }

    /// `numeric_index_fast_set` expanded at the call site, for the bytecode
    /// VM's `SetElement`, where an out-of-line call costs about 7 Ir per store.
    #[inline(always)]
    pub(super) fn numeric_index_fast_set_inlined(
        &self,
        obj_val: &JsValue,
        index: f64,
        value: &JsValue,
    ) -> bool {
        use crate::interpreter::types::DenseArraySet;
        let Some(obj_rc) = obj_val
            .as_object_id()
            .and_then(|obj_id| self.get_object_cell(obj_id))
        else {
            return false;
        };
        let (plan, proto_id, idx) = {
            let obj_borrow = obj_rc.borrow();
            if let Some(ta) = obj_borrow.typed_array_info() {
                if value.as_number().is_none()
                    || ta.kind.is_bigint()
                    || !crate::interpreter::types::is_valid_integer_index(ta, index)
                {
                    return false;
                }
                crate::interpreter::types::typed_array_set_index(ta, index as usize, value);
                return true;
            }
            if index != index.trunc() || index < 0.0 || index >= 4_294_967_295.0 {
                return false;
            }
            let idx = index as usize;
            let Some(plan) = obj_borrow.dense_array_set_plan(idx, value) else {
                return false;
            };
            (plan, obj_borrow.prototype_id, idx)
        };
        if plan == DenseArraySet::Create
            && !crate::interpreter::builtins::array_dense::proto_chain_index_free(self, proto_id)
        {
            return false;
        }
        self.gc_write_barrier_element(obj_rc, idx, value);
        obj_rc
            .borrow_mut_untracked()
            .apply_dense_array_set(idx, value, plan)
    }

    /// Completes the named store `base.key = value` of the bytecode VM without
    /// the generic [[Set]]: an own writable data property is overwritten in
    /// place, or an absent property is created on an extensible receiver whose
    /// prototype chain is ordinary and does not mention `key`. Returns `false`
    /// with no observable effect when the generic path must run.
    #[inline(always)]
    pub(super) fn named_store_fast(
        &self,
        base: &JsValue,
        key: &JsPropertyKey,
        value: &JsValue,
        hint: Option<&Cell<u32>>,
    ) -> bool {
        let Some(cell) = base
            .as_object_id()
            .and_then(|object_id| self.get_object_cell(object_id))
        else {
            return false;
        };
        let mut object = cell.borrow_mut_untracked();
        let done = match object.store_named_fast(key, value, hint) {
            NamedStore::Stored => true,
            NamedStore::Bail => false,
            NamedStore::Absent => {
                let addable = object.can_add_named_fast()
                    && self.proto_chain_lacks_key(object.prototype_id, key);
                if addable {
                    object.add_named_fast(key, value);
                }
                addable
            }
        };
        if done {
            self.gc_write_barrier_value(cell, value);
        }
        done
    }

    /// `base.key++`/`--` on an own writable Number data property, applied in
    /// place; the old and new values. `None` leaves everything untouched.
    #[inline(always)]
    pub(super) fn named_update_fast(
        &self,
        base: &JsValue,
        key: &JsPropertyKey,
        delta: f64,
        hint: Option<&Cell<u32>>,
    ) -> Option<(f64, f64)> {
        let cell = self.get_object_cell(base.as_object_id()?)?;
        cell.borrow_mut_untracked()
            .add_to_named_number(key, delta, hint)
    }

    /// Whether no object on the prototype chain starting at `start` owns
    /// `key` or answers named lookups from outside its property map.
    fn proto_chain_lacks_key(&self, start: Option<u64>, key: &JsPropertyKey) -> bool {
        let mut next = start;
        for _ in 0..PROTO_WALK_LIMIT {
            let Some(proto_id) = next else {
                return true;
            };
            let Some(cell) = self.get_object_cell(proto_id) else {
                return false;
            };
            let proto = cell.borrow_untracked();
            if !proto.named_miss_is_ordinary() || proto.properties.contains_key(key) {
                return false;
            }
            next = proto.prototype_id;
        }
        false
    }

    /// The named read `base.key` resolved without the generic [[Get]] when the
    /// receiver and every object up to the property's holder are ordinary:
    /// the value, or the getter to call with `base`. `Slow` when the generic
    /// path must run (a primitive base, a Proxy or other exotic object on the
    /// way, an Array-index-like name).
    #[inline(always)]
    pub(super) fn named_get_fast(
        &self,
        base: &JsValue,
        key: &JsPropertyKey,
        hint: Option<&Cell<u32>>,
    ) -> NamedGet {
        let Some(mut object_id) = base.as_object_id() else {
            return NamedGet::Slow;
        };
        if key_may_be_array_index(key) {
            return NamedGet::Slow;
        }
        for _ in 0..PROTO_WALK_LIMIT {
            let Some(cell) = self.get_object_cell(object_id) else {
                return NamedGet::Slow;
            };
            let object = cell.borrow_untracked();
            if object.kind.is_named_access_exotic() {
                return NamedGet::Slow;
            }
            if let Some(record) = object.properties.get_hinted(key, hint) {
                return match record.read() {
                    PropertyRead::Value(value) => NamedGet::Value(value.clone()),
                    PropertyRead::Getter(getter) => NamedGet::Getter(getter.clone()),
                };
            }
            if !object.named_miss_is_ordinary() {
                return NamedGet::Slow;
            }
            match object.prototype_id {
                Some(proto_id) => object_id = proto_id,
                None => return NamedGet::Value(JsValue::UNDEFINED),
            }
        }
        NamedGet::Slow
    }

    fn get_object_property_with_proxy_depth<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
        this_val: &JsValue,
        proxy_depth: usize,
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
                    return Completion::Normal(JsValue::UNDEFINED);
                }

                let own = if b.parameter_map().is_none() {
                    match b.properties.get(key) {
                        Some(record) => Some((record.getter().cloned(), record.value().cloned())),
                        None => b
                            .get_own_exotic_property(key)
                            .map(|desc| (desc.get, desc.value)),
                    }
                } else {
                    b.get_own_property_full(key)
                        .map(|desc| (desc.get, desc.value))
                };
                match own {
                    Some((Some(getter), _)) if !getter.is_undefined() => {
                        drop(b);
                        return self.call_getter(&getter, this_val);
                    }
                    Some((Some(_), _)) => return Completion::Normal(JsValue::UNDEFINED),
                    Some((None, value)) => {
                        return Completion::Normal(value.unwrap_or(JsValue::UNDEFINED));
                    }
                    None => {}
                }

                // Own property not found — walk prototype chain
                let proto = b.prototype_id;
                drop(b);
                if let Some(proto_rc) = proto {
                    let proto_id = proto_rc;
                    return self.get_object_property_with_proxy_depth(
                        proto_id,
                        key,
                        this_val,
                        proxy_depth,
                    );
                }
                return Completion::Normal(JsValue::UNDEFINED);
            }
            drop(b);
        }

        // Slow path: proxy or module namespace objects
        self.get_object_property_slow(obj_id, key, this_val, proxy_depth)
    }

    fn get_object_property_slow<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
        this_val: &JsValue,
        proxy_depth: usize,
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
                    if let Some(target_id) = target_val.as_object_id()
                        && let Some(tobj) = self.get_object_cell(target_id)
                    {
                        let target_desc = tobj.borrow().get_own_property(key);
                        if let Some(ref desc) = target_desc
                            && desc.configurable == Some(false)
                        {
                            if desc.is_data_descriptor()
                                && desc.writable == Some(false)
                                && !same_value(
                                    &v,
                                    desc.value.as_ref().unwrap_or(JsValue::undefined_ref()),
                                )
                            {
                                return Completion::Throw(self.create_type_error(
                                        "'get' on proxy: property is a read-only and non-configurable data property on the proxy target but the proxy did not return its actual value",
                                    ));
                            }
                            if desc.is_accessor_descriptor()
                                && desc.get.as_ref().is_none_or(|g| g.is_undefined())
                                && !v.is_undefined()
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
                    if let Some(target_id) = target_val.as_object_id() {
                        let next_depth = match self.advance_proxy_chain_depth(proxy_depth) {
                            Ok(depth) => depth,
                            Err(e) => return Completion::Throw(e),
                        };
                        return self.get_object_property_with_proxy_depth(
                            target_id, key, this_val, next_depth,
                        );
                    }
                    return Completion::Normal(JsValue::UNDEFINED);
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
                    && !Self::is_symbol_like_namespace_key(key, true)
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
                        module_path.as_ref(),
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
            Some(ref d) if d.get.as_ref().is_some_and(|g| !g.is_undefined()) => {
                let getter = d.get.clone().unwrap();
                self.call_getter(&getter, this_val)
            }
            Some(ref d) if d.get.is_some() => Completion::Normal(JsValue::UNDEFINED),
            Some(ref d) => Completion::Normal(d.value.clone().unwrap_or(JsValue::UNDEFINED)),
            None => {
                let proto = if let Some(obj) = self.get_object_cell(obj_id) {
                    obj.borrow().prototype_id
                } else {
                    None
                };
                if let Some(proto_rc) = proto {
                    let proto_id = proto_rc;
                    self.get_object_property_with_proxy_depth(proto_id, key, this_val, proxy_depth)
                } else {
                    Completion::Normal(JsValue::UNDEFINED)
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
        self.proxy_has_property_with_proxy_depth(obj_id, key, 0)
    }

    fn proxy_has_property_with_proxy_depth<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
        proxy_depth: usize,
    ) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            let key_val = self.symbol_key_to_jsvalue(key);
            match self.invoke_proxy_trap(obj_id, "has", vec![target_val.clone(), key_val]) {
                Ok(Some(v)) => {
                    let trap_result = self.to_boolean_val(&v);
                    if !trap_result
                        && let Some(target_id) = target_val.as_object_id()
                        && let Some(tobj) = self.get_object_cell(target_id)
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
                    if let Some(target_id) = target_val.as_object_id() {
                        let next_depth = self.advance_proxy_chain_depth(proxy_depth)?;
                        return self
                            .proxy_has_property_with_proxy_depth(target_id, key, next_depth);
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
                if is_deferred_ns && !Self::is_symbol_like_namespace_key(key, true) {
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
                return self.proxy_has_property_with_proxy_depth(proto_id, key, proxy_depth);
            }
            Ok(false)
        } else {
            Ok(false)
        }
    }

    /// Iterative prototype-chain walk of `get_property`. Returns
    /// `JsValue::UNDEFINED` when the key is not found anywhere in the chain.
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
        JsValue::UNDEFINED
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
        // Common case: nothing on the prototype chain is enumerable, so the
        // start object's own enumerable keys (distinct by construction) are
        // the whole answer and no shadow set is needed.
        if let Some(keys) = self.own_keys_when_chain_is_not_enumerable(start_id) {
            return keys;
        }
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

    fn own_keys_when_chain_is_not_enumerable(&self, start_id: u64) -> Option<Vec<JsPropertyKey>> {
        let mut next = {
            let b = self.get_object_cell(start_id)?.borrow();
            b.prototype_id
        };
        while let Some(id) = next {
            let b = self.get_object_cell(id)?.borrow();
            if b.has_enumerable_string_key_hint() != Some(false) {
                return None;
            }
            next = b.prototype_id;
        }
        self.get_object_cell(start_id)?
            .borrow()
            .ordinary_own_property_keys(crate::interpreter::types::OwnKeyFilter::EnumerableStrings)
    }

    /// True when `start_id` or any object on its prototype chain is a Proxy
    /// (revoked or not) or a module namespace, whose keys and prototype only
    /// the proxy-aware walk can observe.
    fn chain_has_proxy_or_namespace(&self, start_id: u64) -> bool {
        let mut current = Some(start_id);
        while let Some(id) = current {
            let Some(cell) = self.get_object_cell(id) else {
                return false;
            };
            let b = cell.borrow();
            if b.proxy().is_some() || b.module_namespace().is_some() {
                return true;
            }
            current = b.prototype_id;
        }
        false
    }

    /// The keys `EnumerateObjectProperties` walks for for-in over `obj_id`:
    /// proxy-aware, own keys before inherited, shadowed and non-enumerable
    /// names excluded. Shared by the tree-walker's `exec_for_in` and the
    /// state-machine drivers' for-in enumerator so the two cannot diverge.
    pub(crate) fn for_in_enumerable_keys(
        &mut self,
        obj_id: u64,
    ) -> Result<Vec<JsPropertyKey>, JsValue> {
        let needs_proxy_path = self.chain_has_proxy_or_namespace(obj_id);
        if needs_proxy_path {
            self.proxy_enumerable_keys_with_proto(obj_id)
        } else if self.get_object_cell(obj_id).is_some() {
            Ok(self.enumerable_keys_with_proto_on_id(obj_id))
        } else {
            Ok(Vec::new())
        }
    }

    /// §10.4.2.4 ArraySetLength(A, Desc)
    ///
    /// Roots `obj_id` for the whole operation: ToUint32/ToNumber on
    /// `desc.value` below can invoke a user `valueOf` that reaches a GC
    /// safepoint, and every step after that re-derefs `obj_id` — an
    /// unrooted, ephemeral receiver (e.g. an array literal used only as a
    /// destructuring-assignment target) can otherwise be collected out from
    /// under this function, turning `get_object_cell(obj_id).unwrap()` into
    /// a panic.
    ///
    /// Unroots only this receiver on the way out (mirrors
    /// `call_function_inner`'s operand cleanup), not a bulk
    /// `gc_unroot_frame`: the `valueOf` call above can itself leave a
    /// *persistent* root on the stack — e.g. `Atomics.waitAsync`'s resolver,
    /// meant to survive until the async completion settles — and a frame
    /// truncate would discard that too.
    pub(crate) fn array_set_length(
        &mut self,
        obj_id: usize,
        desc: PropertyDescriptor,
    ) -> Result<bool, JsValue> {
        let receiver = JsValue::object(obj_id as u64);
        self.gc_root_value(&receiver);
        let result = self.array_set_length_inner(obj_id, desc);
        self.gc_unroot_value(&receiver);
        result
    }

    fn array_set_length_inner(
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
        new_len_desc.value = Some(JsValue::number(new_len as f64));

        // 6. Let oldLenDesc be OrdinaryGetOwnProperty(A, "length").
        let (old_len, old_len_writable) = {
            let obj_rc = self.get_object_cell(obj_id as u64).unwrap();
            let obj = obj_rc.borrow();
            let old_len_desc = obj.properties.get("length").cloned();
            match old_len_desc {
                Some(ref d) => {
                    let ol = d
                        .value()
                        .and_then(JsValue::as_number)
                        .map(|n| n as u32)
                        .unwrap_or(0);
                    let w = d.writable().unwrap_or(true);
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
                    .is_some_and(|d| d.configurable() == Some(false));
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
                len_desc.set_value(Some(JsValue::number(actual_new_len as f64)));
            }
            // 13.d.iii.2. If newWritable is false, set length writable to false.
            if !new_writable && let Some(len_desc) = obj.properties.get_mut("length") {
                len_desc.set_writable(Some(false));
            }
            return Ok(false);
        }

        // 14. If newWritable is false, set [[Writable]] on length to false.
        if !new_writable {
            let obj_rc = self.get_object_cell(obj_id as u64).unwrap();
            let mut obj = obj_rc.borrow_mut();
            if let Some(len_desc) = obj.properties.get_mut("length") {
                len_desc.set_writable(Some(false));
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
        if key.as_property_key_bytes() == b"length" {
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
                    .and_then(|d| d.value())
                    .and_then(JsValue::as_number)
                    .map(|n| n as u32)
                    .unwrap_or(0);
                let w = len_desc.and_then(|d| d.writable()).unwrap_or(true);
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
                    len_desc.set_value(Some(JsValue::number(new_len as f64)));
                }
                if let Some(elems) = obj.array_elements_mut() {
                    let val = desc.value.unwrap_or(JsValue::UNDEFINED);
                    let idx = index_u32 as usize;
                    if idx < elems.len() {
                        elems[idx] = val;
                    } else if idx <= elems.len() + 1024 {
                        while elems.len() < idx {
                            elems.push(JsValue::UNDEFINED);
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
        self.proxy_set_with_outcome(obj_id, key, value, receiver)
            .map(SetOutcome::succeeded)
    }

    fn proxy_set_with_outcome<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
        value: JsValue,
        receiver: &JsValue,
    ) -> Result<SetOutcome, JsValue> {
        self.proxy_set_with_outcome_and_proxy_depth(obj_id, key, value, receiver, 0)
    }

    fn proxy_set_with_outcome_and_proxy_depth<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
        value: JsValue,
        receiver: &JsValue,
        proxy_depth: usize,
    ) -> Result<SetOutcome, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let (target_id, trap) = self.resolve_proxy_trap(obj_id, "set")?;
            let target_val = JsValue::object(target_id);
            let Some((handler_val, trap_val)) = trap else {
                let next_depth = self.advance_proxy_chain_depth(proxy_depth)?;
                return self.proxy_set_with_outcome_and_proxy_depth(
                    target_id, key, value, receiver, next_depth,
                );
            };
            let key_val = self.symbol_key_to_jsvalue(key);
            let args = [target_val.clone(), key_val, value.clone(), receiver.clone()];
            let trap_call = self.call_noted_trap("set", &trap_val, &handler_val, &args);
            match trap_call {
                Ok(v) => {
                    if self.to_boolean_val(&v) {
                        if let Some(tobj) = self.get_object(target_id) {
                            let target_desc = tobj.borrow().get_own_property(key);
                            if let Some(ref desc) = target_desc
                                && desc.configurable == Some(false)
                            {
                                if desc.is_data_descriptor()
                                    && desc.writable == Some(false)
                                    && !same_value(
                                        &value,
                                        desc.value.as_ref().unwrap_or(JsValue::undefined_ref()),
                                    )
                                {
                                    return Err(self.create_type_error(
                                            "'set' on proxy: trap returned truish for property which exists in the proxy target as a non-configurable and non-writable data property with a different value",
                                        ));
                                }
                                if desc.is_accessor_descriptor()
                                    && desc.set.as_ref().is_none_or(|s| s.is_undefined())
                                {
                                    return Err(self.create_type_error(
                                            "'set' on proxy: trap returned truish for property which exists in the proxy target as a non-configurable and non-writable accessor property without a setter",
                                        ));
                                }
                            }
                        }
                        Ok(SetOutcome::Succeeded)
                    } else {
                        Ok(SetOutcome::Rejected)
                    }
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object(obj_id) {
            // TypedArray [[Set]] §10.4.5.5
            let is_ta = obj.borrow().typed_array_info().is_some();
            if is_ta && let Some(index) = canonical_numeric_index_string(key) {
                let same_val = receiver.as_object_id() == Some(obj_id);
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
                        JsValue::number(self.to_number_value(&value)?)
                    };
                    let obj_ref = obj.borrow();
                    let ta = obj_ref.typed_array_info().unwrap();
                    if is_valid_integer_index(ta, index) {
                        typed_array_set_index(ta, index as usize, &num_val);
                    }
                    return Ok(SetOutcome::Succeeded);
                } else {
                    // Different receiver: if invalid index return true without coercing
                    let valid = {
                        let obj_ref = obj.borrow();
                        let ta = obj_ref.typed_array_info().unwrap();
                        is_valid_integer_index(ta, index)
                    };
                    if !valid {
                        return Ok(SetOutcome::Succeeded);
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
                        && !setter.is_undefined()
                    {
                        let setter = setter.clone();
                        match self.call_function(&setter, receiver, &[value]) {
                            Completion::Normal(_) => return Ok(SetOutcome::Succeeded),
                            Completion::Throw(e) => return Err(e),
                            _ => return Ok(SetOutcome::Succeeded),
                        }
                    }
                    return Ok(SetOutcome::Rejected);
                }
                // Data descriptor
                if desc.writable == Some(false) {
                    return Ok(SetOutcome::Rejected);
                }
                // OrdinarySetWithOwnDescriptor step 3.c: use Receiver.[[GetOwnProperty]] / [[DefineOwnProperty]]
                let recv_id = receiver.as_object_id();
                if recv_id == Some(obj_id) {
                    // Common case: receiver is the same object. Ordinary objects
                    // (and ordinary Array properties) can take the direct route;
                    // only Array's "length" is exotic and must go through
                    // [[DefineOwnProperty]] (ArraySetLength, §10.4.2.4) or its
                    // ToUint32 coercion / RangeError / element-deletion semantics
                    // are silently skipped. Gate on the key too, not just the
                    // class — routing every existing-element write (e.g. `a[0]++`)
                    // through descriptor allocation regressed a 200k-iteration
                    // increment loop by ~1.7-2x.
                    if key.as_property_key_bytes() == b"length"
                        && obj.borrow().class_name == "Array"
                    {
                        // Call ArraySetLength directly with a native descriptor —
                        // round-tripping through from_property_descriptor +
                        // to_property_descriptor would build a JS object whose
                        // [[Prototype]] is Object.prototype, and ToPropertyDescriptor
                        // reads fields via [[HasProperty]] (prototype-chain walk), so
                        // an unrelated Object.prototype mutation (e.g. a non-callable
                        // "get") would leak into this internal, value-only descriptor.
                        let val_desc = crate::interpreter::types::PropertyDescriptor {
                            value: Some(value),
                            writable: None,
                            enumerable: None,
                            configurable: None,
                            get: None,
                            set: None,
                        };
                        return self
                            .array_set_length(obj_id as usize, val_desc)
                            .map(SetOutcome::from_success);
                    }
                    self.gc_write_barrier_value(&obj, &value);
                    let succeeded = obj.borrow_mut_untracked().set_property_value(key, value);
                    return Ok(SetOutcome::from_success(succeeded));
                }
                // Receiver differs: call Receiver.[[GetOwnProperty]](P) and [[DefineOwnProperty]]
                if let Some(rid) = recv_id {
                    let succeeded = self.set_on_receiver_native(rid, key, value)?;
                    return Ok(SetOutcome::from_success(succeeded));
                }
                // Receiver is not an Object: [[Set]] must return false, never
                // fall back to writing the property onto `obj` itself — `obj`
                // may be a shared prototype reached by walking up from the
                // (non-object) receiver's own [[Prototype]] chain.
                return Ok(SetOutcome::Rejected);
            }
            // No own property, walk prototype chain
            let proto = obj.borrow().prototype_id;
            if let Some(proto_rc) = proto {
                let proto_id = proto_rc;
                return self.proxy_set_with_outcome_and_proxy_depth(
                    proto_id,
                    key,
                    value,
                    receiver,
                    proxy_depth,
                );
            }
            // No prototype: OrdinarySetWithOwnDescriptor with synthetic {writable:true,...} ownDesc.
            // Per spec step 1.c.i + 2.c: call Receiver.[[GetOwnProperty]](P) then act on result.
            if let Some(recv_id) = receiver.as_object_id() {
                // Which exotic [[GetOwnProperty]]/[[DefineOwnProperty]] the
                // receiver has, read in one borrow that ends before the
                // `&mut self` calls below.
                let (is_module_namespace_recv, is_proxy_recv) =
                    self.get_object_cell(recv_id).map_or((false, false), |o| {
                        let b = o.borrow();
                        (
                            b.module_namespace().is_some(),
                            b.is_proxy() || b.is_proxy_revoked(),
                        )
                    });
                // Module namespace exotic [[GetOwnProperty]] triggers deferred
                // evaluation / TDZ checks before its [[DefineOwnProperty]]
                // necessarily rejects the new data property.
                if is_module_namespace_recv {
                    self.check_namespace_tdz(recv_id, key)?;
                    return Ok(SetOutcome::Rejected);
                }
                if is_proxy_recv {
                    let succeeded = self.set_on_receiver_native(recv_id, key, value)?;
                    return Ok(SetOutcome::from_success(succeeded));
                }
                if let Some(recv_obj) = self.get_object_cell(recv_id) {
                    self.gc_write_barrier_value(recv_obj, &value);
                    let succeeded = recv_obj
                        .borrow_mut_untracked()
                        .set_property_value(key, value);
                    return Ok(SetOutcome::from_success(succeeded));
                }
            }
            // Receiver is not an Object (or resolved to no live object): [[Set]]
            // must return false, never fall back to writing onto `obj` itself.
            Ok(SetOutcome::Rejected)
        } else {
            Ok(SetOutcome::Rejected)
        }
    }

    /// OrdinarySetWithOwnDescriptor steps 2.c-2.e against an object `receiver`:
    /// `Receiver.[[GetOwnProperty]](P)` followed by `CreateDataProperty` or
    /// `[[DefineOwnProperty]](P, {[[Value]]: V})`, all on internal descriptor
    /// records so a Proxy receiver only builds JS objects for traps that exist.
    pub(crate) fn set_on_receiver_native<K: PropertyKeyLike + ?Sized>(
        &mut self,
        receiver_id: u64,
        key: &K,
        value: JsValue,
    ) -> Result<bool, JsValue> {
        let desc = match self.proxy_get_own_property_native(receiver_id, key)? {
            None => PropertyDescriptor {
                value: Some(value),
                writable: Some(true),
                enumerable: Some(true),
                configurable: Some(true),
                get: None,
                set: None,
            },
            Some(existing) => {
                if existing.is_accessor_descriptor() || existing.writable == Some(false) {
                    return Ok(false);
                }
                PropertyDescriptor {
                    value: Some(value),
                    writable: None,
                    enumerable: None,
                    configurable: None,
                    get: None,
                    set: None,
                }
            }
        };
        self.proxy_define_own_property_native(receiver_id, key.to_js_property_key(), desc)
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
                        && let Some(target_id) = target_val.as_object_id()
                        && let Some(tobj) = self.get_object_cell(target_id)
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
                    if let Some(target_id) = target_val.as_object_id() {
                        return self.proxy_delete_property(target_id, key);
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
                    && let Some(len) = borrow
                        .primitive_value
                        .as_ref()
                        .and_then(|v| v.with_string(|s| s.len()))
                {
                    if key.as_property_key_bytes() == b"length" {
                        return Ok(false);
                    }
                    if crate::interpreter::types::string_exotic_index(key, len).is_some() {
                        return Ok(false);
                    }
                }
            }
            let mut m = obj.borrow_mut();
            if let Some(desc) = m.properties.get(key)
                && desc.configurable() == Some(false)
            {
                return Ok(false);
            }
            m.remove_property(key);
            if let Some(key_str) = key.as_property_key_str()
                && let Some(elems) = m.array_elements_mut()
                && let Some(idx) = crate::interpreter::types::array_element_index(key_str)
                && idx < elems.len()
            {
                elems[idx] = JsValue::UNDEFINED;
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
                JsValue::number(self.to_number_value(value)?)
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

    /// Whether `key` is an enumerable own property of `obj_id`, via the same [[GetOwnProperty]]
    /// path as `proxy_get_own_property_descriptor`. Non-proxy objects skip the descriptor-object
    /// round trip that the generic path performs.
    pub(crate) fn proxy_own_property_is_enumerable<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
    ) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_none()
            && let Some(obj) = self.get_object(obj_id)
        {
            self.check_namespace_tdz(obj_id, key)?;
            let enumerable = obj
                .borrow()
                .get_own_property(key)
                .is_some_and(|d| d.enumerable == Some(true));
            return Ok(enumerable);
        }
        let descriptor_value = self.proxy_get_own_property_descriptor(obj_id, key)?;
        if descriptor_value.is_undefined() {
            return Ok(false);
        }
        match self.to_property_descriptor(&descriptor_value) {
            Ok(descriptor) => Ok(descriptor.enumerable == Some(true)),
            Err(Some(error)) => Err(error),
            Err(None) => Ok(false),
        }
    }

    /// One step of EnumerableOwnProperties / CopyDataProperties over a key
    /// snapshot of an ordinary object: re-reads `key`'s enumerability (an
    /// earlier getter may have deleted or hidden it) and, if it is still an
    /// enumerable own property, returns its [[Get]] value.
    pub(crate) fn get_own_enumerable_value(
        &mut self,
        obj_id: u64,
        key: &JsPropertyKey,
        receiver: &JsValue,
    ) -> Result<Option<JsValue>, JsValue> {
        let enumerable = self
            .get_object_cell(obj_id)
            .and_then(|c| c.borrow().own_property_enumerable(key));
        if enumerable != Some(true) {
            return Ok(None);
        }
        match self.get_object_property(obj_id, key, receiver) {
            Completion::Normal(v) => Ok(Some(v)),
            Completion::Throw(e) => Err(e),
            _ => Ok(Some(JsValue::UNDEFINED)),
        }
    }

    /// `proxy_get_own_property_descriptor(..) !== undefined` without
    /// materialising a descriptor object for non-Proxy objects.
    pub(crate) fn proxy_has_own_property<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
    ) -> Result<bool, JsValue> {
        if self.get_proxy_info(obj_id).is_none()
            && let Some(obj) = self.get_object(obj_id)
        {
            self.check_namespace_tdz(obj_id, key)?;
            return Ok(obj.borrow().own_property_enumerable(key).is_some());
        }
        Ok(!self
            .proxy_get_own_property_descriptor(obj_id, key)?
            .is_undefined())
    }

    /// Proxy-aware [[GetOwnProperty]] returning a JS descriptor object (or `undefined`).
    pub(crate) fn proxy_get_own_property_descriptor<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
    ) -> Result<JsValue, JsValue> {
        Ok(match self.proxy_get_own_property_native(obj_id, key)? {
            Some(d) => self.from_property_descriptor(&d),
            None => JsValue::UNDEFINED,
        })
    }

    /// Proxy-aware [[GetOwnProperty]] yielding the internal descriptor record: the
    /// spec's Property Descriptor, with no JS object built unless a
    /// `getOwnPropertyDescriptor` trap is actually present. Forwards natively to the
    /// target when the handler has no such trap (the lookup itself stays observable).
    pub(crate) fn proxy_get_own_property_native<K: PropertyKeyLike + ?Sized>(
        &mut self,
        obj_id: u64,
        key: &K,
    ) -> Result<Option<PropertyDescriptor>, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let (target_id, trap) = self.resolve_proxy_trap(obj_id, "getOwnPropertyDescriptor")?;
            let target_val = JsValue::object(target_id);
            let Some((handler_val, trap_val)) = trap else {
                return self.proxy_get_own_property_native(target_id, key);
            };
            let key_val = self.symbol_key_to_jsvalue(key);
            let trap_call =
                match self.call_function(&trap_val, &handler_val, &[target_val.clone(), key_val]) {
                    Completion::Normal(v) => Ok(v),
                    Completion::Throw(e) => Err(e),
                    _ => Ok(JsValue::UNDEFINED),
                };
            match trap_call {
                Ok(v) => {
                    // Step 11: If Type(trapResultObj) is neither Object nor Undefined, throw TypeError
                    if !v.is_object() && !v.is_undefined() {
                        return Err(self.create_type_error(
                            "'getOwnPropertyDescriptor' on proxy: trap returned neither Object nor undefined",
                        ));
                    }
                    self.gc_root_value(&v);
                    let frame = self.gc_root_frame();
                    let result = self.validate_proxy_gopd_trap_result(target_id, key, &v);
                    self.gc_unroot_frame(frame);
                    self.gc_unroot_value(&v);
                    result
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object(obj_id) {
            // §10.4.6.4 [[GetOwnProperty]] step 4: namespace [[Get]] can throw for TDZ
            self.check_namespace_tdz(obj_id, key)?;
            let desc = obj.borrow().get_own_property(key);
            Ok(desc)
        } else {
            Ok(None)
        }
    }

    /// Steps 11-20 of Proxy [[GetOwnProperty]] once the trap has returned `v`.
    /// `v` and the target's descriptor values stay rooted across the nested
    /// target traps (`[[GetOwnProperty]]`, `[[IsExtensible]]`), which run user code
    /// that can reach a GC safepoint before `v` is read back.
    fn validate_proxy_gopd_trap_result<K: PropertyKeyLike + ?Sized>(
        &mut self,
        target_id: u64,
        key: &K,
        v: &JsValue,
    ) -> Result<Option<PropertyDescriptor>, JsValue> {
        let target_desc = self.proxy_get_own_property_native(target_id, key)?;
        if let Some(td) = target_desc.as_ref() {
            for val in [&td.value, &td.get, &td.set].into_iter().flatten() {
                self.gc_root_value(val);
            }
        }
        if v.is_undefined() {
            let Some(td) = target_desc else {
                return Ok(None);
            };
            if td.configurable == Some(false) {
                return Err(self.create_type_error(
                    "'getOwnPropertyDescriptor' on proxy: trap returned undefined for property which is non-configurable in the proxy target",
                ));
            }
            if !self.proxy_is_extensible(target_id)? {
                return Err(self.create_type_error(
                    "'getOwnPropertyDescriptor' on proxy: trap returned undefined for property which exists in the non-extensible proxy target",
                ));
            }
            return Ok(None);
        }
        let target_extensible = self.proxy_is_extensible(target_id)?;
        let mut result_desc = match self.to_property_descriptor(v) {
            Ok(d) => d,
            Err(Some(e)) => return Err(e),
            Err(None) => return Ok(None),
        };
        complete_property_descriptor(&mut result_desc);
        if !is_compatible_property_desc(target_extensible, &result_desc, target_desc.as_ref()) {
            return Err(self.create_type_error(
                "'getOwnPropertyDescriptor' on proxy: trap returned a descriptor incompatible with the proxy target's property",
            ));
        }
        if result_desc.configurable == Some(false) {
            match target_desc.as_ref() {
                None => {
                    return Err(self.create_type_error(
                        "'getOwnPropertyDescriptor' on proxy: trap reported non-configurable for a property that does not exist in the proxy target",
                    ));
                }
                Some(td) if td.configurable == Some(true) => {
                    return Err(self.create_type_error(
                        "'getOwnPropertyDescriptor' on proxy: trap reported non-configurable for a property that is configurable in the proxy target",
                    ));
                }
                Some(td) => {
                    if result_desc.writable == Some(false) && td.writable == Some(true) {
                        return Err(self.create_type_error(
                            "'getOwnPropertyDescriptor' on proxy: trap returned non-configurable non-writable descriptor for a writable property in the proxy target",
                        ));
                    }
                }
            }
        }
        Ok(Some(result_desc))
    }

    /// Proxy-aware [[OwnPropertyKeys]] - checks proxy `ownKeys` trap, recurses on target if no trap.
    /// Returns all own property keys (for getOwnPropertyNames).
    pub(crate) fn proxy_own_keys(&mut self, obj_id: u64) -> Result<Vec<JsValue>, JsValue> {
        const MAX_PROXY_OWNKEYS_RESULT_LEN: usize = 1_000_000;

        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            match self.invoke_proxy_trap(obj_id, "ownKeys", vec![target_val.clone()]) {
                Ok(Some(v)) => {
                    if !v.is_object() {
                        return Err(
                            self.create_type_error("CreateListFromArrayLike called on non-object")
                        );
                    }
                    if let Some(arr_id) = v.as_object_id() {
                        // Use [[Get]] for length (spec: CreateListFromArrayLike)
                        let len_val = match self.get_object_property(arr_id, "length", &v) {
                            Completion::Normal(lv) => lv,
                            Completion::Throw(e) => return Err(e),
                            _ => JsValue::UNDEFINED,
                        };
                        let len = match len_val.as_number() {
                            Some(n) if n.is_finite() && n > 0.0 => {
                                let len = n.floor() as usize;
                                if len > MAX_PROXY_OWNKEYS_RESULT_LEN {
                                    return Err(self.create_type_error(
                                        "'ownKeys' on proxy: trap result length exceeds supported limit",
                                    ));
                                }
                                len
                            }
                            Some(_) => 0,
                            None => {
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
                                _ => JsValue::UNDEFINED,
                            };
                            keys.push(elem);
                        }
                        for key in &keys {
                            if !key.is_string() && !key.is_symbol() {
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
                    if let Some(target_id) = target_val.as_object_id() {
                        return self.proxy_own_keys(target_id);
                    }
                    Ok(vec![])
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object_cell(obj_id) {
            // Ordinary, Array and Arguments objects: keys come straight from
            // the element vector and the property map.
            let fast = obj
                .borrow()
                .ordinary_own_property_keys(crate::interpreter::types::OwnKeyFilter::AllKeys);
            if let Some(keys) = fast {
                let mut result = Vec::with_capacity(keys.len());
                for k in &keys {
                    if k.is_symbol() {
                        result.push(self.symbol_key_to_jsvalue(k));
                    } else {
                        result.push(JsValue::string(
                            crate::interpreter::key_intern::key_to_js_string(k),
                        ));
                    }
                }
                return Ok(result);
            }
            let obj = self.get_object_cell(obj_id).unwrap();
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
            let is_string_wrapper = b.class_name == "String"
                && b.primitive_value.as_ref().is_some_and(JsValue::is_string);
            let string_len = if is_string_wrapper {
                b.primitive_value
                    .as_ref()
                    .and_then(|v| v.with_string(|s| s.len()))
                    .unwrap_or(0)
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
                    if value.is_undefined() || i > 0xFFFF_FFFE {
                        continue;
                    }
                    int_keys_set.insert(i as u64, i.to_string());
                }
            }

            for k in b.properties.keys() {
                if k.is_symbol() {
                    sym_keys.push(k.clone());
                } else if let Some(n) = parse_array_index(k) {
                    // String char indices take precedence, but the BTreeMap
                    // handles uniqueness and ascending numeric order.
                    int_keys_set.insert(n as u64, k.to_string());
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
                result.push(JsValue::from_str(&k));
            }
            // A String exotic's virtual "length" property exists before any
            // subsequently created ordinary string properties.
            if is_string_wrapper {
                result.push(JsValue::from_str("length"));
            }
            for k in str_keys {
                result.push(JsValue::string(k.to_js_string()));
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
        let mut proxy_depth = 0;

        while let Some(cid) = current_id {
            // Get own keys for current object (proxy-aware)
            let own_keys = self.proxy_own_keys(cid)?;
            for key in &own_keys {
                if let Some(s) = key.as_string() {
                    let key_str = crate::interpreter::key_intern::key_from_js_string(&s);
                    if seen.contains(&key_str) {
                        continue;
                    }
                    // Check enumerability via proxy-aware [[GetOwnProperty]]
                    let desc_val = self.proxy_get_own_property_descriptor(cid, &key_str)?;
                    seen.insert(key_str.clone());
                    if !desc_val.is_undefined()
                        && let Ok(desc) = self.to_property_descriptor(&desc_val)
                        && desc.enumerable != Some(false)
                    {
                        keys.push(key_str);
                    }
                }
            }

            // Walk prototype chain (proxy-aware)
            if self.get_proxy_info(cid).is_some() {
                proxy_depth = self.advance_proxy_chain_depth(proxy_depth)?;
            }
            current_id = self.proxy_get_prototype_of(cid)?.as_object_id();
        }
        Ok(keys)
    }

    /// Proxy-aware [[GetPrototypeOf]] - checks proxy `getPrototypeOf` trap, recurses on target if no trap.
    pub(crate) fn proxy_get_prototype_of(&mut self, obj_id: u64) -> Result<JsValue, JsValue> {
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            match self.invoke_proxy_trap(obj_id, "getPrototypeOf", vec![target_val.clone()]) {
                Ok(Some(v)) => {
                    if !v.is_object() && !v.is_null() {
                        return Err(self.create_type_error(
                            "'getPrototypeOf' on proxy: trap returned neither object nor null",
                        ));
                    }
                    // Step 8: extensibleTarget = ? IsExtensible(target)
                    if let Some(target_id) = target_val.as_object_id() {
                        let extensible_target = self.proxy_is_extensible(target_id)?;
                        if !extensible_target {
                            // Step 10: targetProto = ? target.[[GetPrototypeOf]]()
                            let actual_proto = self.proxy_get_prototype_of(target_id)?;
                            let same = match (v.as_object_id(), actual_proto.as_object_id()) {
                                (Some(a), Some(b)) => a == b,
                                (None, None) => v.is_null() && actual_proto.is_null(),
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
                    if let Some(target_id) = target_val.as_object_id() {
                        return self.proxy_get_prototype_of(target_id);
                    }
                    Ok(JsValue::NULL)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object_cell(obj_id) {
            if let Some(id) = obj.borrow().prototype_id {
                Ok(JsValue::object(id))
            } else {
                Ok(JsValue::NULL)
            }
        } else {
            Ok(JsValue::NULL)
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
                    let Some(target_id) = target_val.as_object_id() else {
                        return Ok(true);
                    };
                    let extensible_target = self.proxy_is_extensible(target_id)?;
                    // Step 9: if extensible, no invariant to check
                    if extensible_target {
                        return Ok(true);
                    }
                    // Step 10: GetPrototypeOf(target) — may throw
                    let actual_proto = self.proxy_get_prototype_of(target_id)?;
                    let same = match (proto.as_object_id(), actual_proto.as_object_id()) {
                        (Some(a), Some(b)) => a == b,
                        (None, None) => proto.is_null() && actual_proto.is_null(),
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
                    if let Some(target_id) = target_val.as_object_id() {
                        return self.proxy_set_prototype_of(target_id, proto);
                    }
                    Ok(true)
                }
                Err(e) => Err(e),
            }
        } else if let Some(obj) = self.get_object_cell(obj_id) {
            // OrdinarySetPrototypeOf
            let current_proto_id = obj.borrow().prototype_id;
            let new_proto_id = proto.as_object_id();
            let same = (proto.is_null() && current_proto_id.is_none())
                || matches!((new_proto_id, current_proto_id), (Some(a), Some(b)) if a == b);
            if same {
                return Ok(true);
            }
            if !obj.borrow().extensible {
                return Ok(false);
            }
            // Cycle check
            if let Some(proto_id) = proto.as_object_id() {
                let mut check_id = Some(proto_id);
                while let Some(cid) = check_id {
                    if cid == obj_id {
                        return Ok(false);
                    }
                    check_id = self
                        .get_object_cell(cid)
                        .and_then(|o| o.borrow().prototype_id.as_ref().copied());
                }
            }
            if proto.is_null() {
                obj.borrow_mut().prototype_id = None;
            } else if let Some(new_id) = proto.as_object_id()
                && let Some(po) = self.get_object_cell(new_id)
            {
                obj.borrow_mut().prototype_id = Some(po.borrow().id.unwrap());
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
                    if let Some(target_id) = target_val.as_object_id()
                        && let Some(tobj) = self.get_object_cell(target_id)
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
                    if let Some(target_id) = target_val.as_object_id() {
                        return self.proxy_is_extensible(target_id);
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
                        && let Some(target_id) = target_val.as_object_id()
                        && let Some(tobj) = self.get_object_cell(target_id)
                        && tobj.borrow().extensible
                    {
                        return Err(self.create_type_error(
                                "'preventExtensions' on proxy: trap returned truish but the proxy target is extensible",
                            ));
                    }
                    Ok(trap_result)
                }
                Ok(None) => {
                    if let Some(target_id) = target_val.as_object_id() {
                        return self.proxy_prevent_extensions(target_id);
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

/// CompletePropertyDescriptor (§6.2.6.6)
fn complete_property_descriptor(desc: &mut PropertyDescriptor) {
    if !desc.is_accessor_descriptor() {
        desc.value.get_or_insert(JsValue::UNDEFINED);
        desc.writable.get_or_insert(false);
    } else {
        desc.get.get_or_insert(JsValue::UNDEFINED);
        desc.set.get_or_insert(JsValue::UNDEFINED);
    }
    desc.enumerable.get_or_insert(false);
    desc.configurable.get_or_insert(false);
}

/// IsCompatiblePropertyDescriptor (§10.1.6.3)
fn is_compatible_property_desc(
    extensible: bool,
    desc: &PropertyDescriptor,
    current: Option<&PropertyDescriptor>,
) -> bool {
    // Step 2: If current is undefined, return extensible
    let Some(current) = current else {
        return extensible;
    };
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
            if let Some(ref desc_val) = desc.value
                && !same_value(
                    desc_val,
                    current.value.as_ref().unwrap_or(JsValue::undefined_ref()),
                )
            {
                return false;
            }
        }
        return true;
    }
    // Step 7: Both are accessor descriptors
    if current.configurable == Some(false) {
        // 7a.i: If Desc has [[Set]] and SameValue(Desc.[[Set]], current.[[Set]]) is false
        if let Some(ref desc_set) = desc.set
            && !same_value(
                desc_set,
                current.set.as_ref().unwrap_or(JsValue::undefined_ref()),
            )
        {
            return false;
        }
        // 7a.ii: If Desc has [[Get]] and SameValue(Desc.[[Get]], current.[[Get]]) is false
        if let Some(ref desc_get) = desc.get
            && !same_value(
                desc_get,
                current.get.as_ref().unwrap_or(JsValue::undefined_ref()),
            )
        {
            return false;
        }
    }
    true
}
