use super::*;

impl Interpreter {
    /// Evaluate an OptionalChain expression and return (value, this_context).
    /// Used when the optional chain result feeds into a call or nested chain.
    pub(super) fn eval_optional_chain_with_ref(
        &mut self,
        base: &Expression,
        chain: &Expression,
        env: &EnvRef,
    ) -> Result<(JsValue, JsValue), Completion> {
        let (base_val, base_this) = self.eval_oc_base(base, chain, env)?;
        if matches!(base_val, JsValue::Null | JsValue::Undefined) {
            return Ok((JsValue::Undefined, JsValue::Undefined));
        }
        self.eval_oc_tail_with_this_ctx(&base_val, &base_this, chain, env)
    }

    /// Evaluate the base expression of an OptionalChain, returning (value, this).
    pub(super) fn eval_oc_base(
        &mut self,
        base: &Expression,
        chain: &Expression,
        env: &EnvRef,
    ) -> Result<(JsValue, JsValue), Completion> {
        match base {
            Expression::Member(obj_expr, member_prop, _) => {
                if matches!(obj_expr.as_ref(), Expression::Super) {
                    // §13.3.7.1: super property in optional chain — use HomeObject.__proto__
                    let this_val = env.borrow().get("this").unwrap_or(JsValue::Undefined);
                    let key = match member_prop {
                        MemberProperty::Dot(name) => name.clone(),
                        MemberProperty::Computed(expr) => {
                            let v = match self.eval_expr(expr, env) {
                                Completion::Normal(v) => v,
                                other => return Err(other),
                            };
                            match self.to_property_key(&v) {
                                Ok(s) => s,
                                Err(e) => return Err(Completion::Throw(e)),
                            }
                        }
                        MemberProperty::Private(name) => {
                            let branded = self.resolve_private_name(name, env);
                            if let JsValue::Object(ref o) = this_val
                                && let Some(obj) = self.get_object_cell(o.id)
                            {
                                let elem = obj.borrow().private_fields.get(&branded).cloned();
                                match elem {
                                    Some(PrivateElement::Field(v))
                                    | Some(PrivateElement::Method(v)) => {
                                        return Ok((v, this_val));
                                    }
                                    Some(PrivateElement::Accessor { get, .. }) => {
                                        if let Some(getter) = get {
                                            match self.call_function(&getter, &this_val, &[]) {
                                                Completion::Normal(v) => return Ok((v, this_val)),
                                                other => return Err(other),
                                            }
                                        }
                                        return Err(Completion::Throw(self.create_type_error(
                                            &format!("Cannot read private member #{name}"),
                                        )));
                                    }
                                    None => {
                                        return Err(Completion::Throw(self.create_type_error(
                                            &format!("Cannot read private member #{name}"),
                                        )));
                                    }
                                }
                            }
                            return Ok((JsValue::Undefined, this_val));
                        }
                    };
                    let super_base_id = self.get_super_base_id(env);
                    match super_base_id {
                        Some(base_id) => {
                            let val = match self.get_object_property(base_id, &key, &this_val) {
                                Completion::Normal(v) => v,
                                other => return Err(other),
                            };
                            Ok((val, this_val))
                        }
                        None => Err(Completion::Throw(self.create_type_error(&format!(
                            "Cannot read properties of null (reading '{key}')"
                        )))),
                    }
                } else {
                    let obj_val = match self.eval_expr(obj_expr, env) {
                        Completion::Normal(v) => v,
                        other => return Err(other),
                    };
                    let key = match member_prop {
                        MemberProperty::Dot(name) => name.clone(),
                        MemberProperty::Computed(expr) => {
                            let v = match self.eval_expr(expr, env) {
                                Completion::Normal(v) => v,
                                other => return Err(other),
                            };
                            match self.to_property_key(&v) {
                                Ok(s) => s,
                                Err(e) => return Err(Completion::Throw(e)),
                            }
                        }
                        MemberProperty::Private(name) => {
                            let branded = self.resolve_private_name(name, env);
                            if let JsValue::Object(ref o) = obj_val
                                && let Some(obj) = self.get_object_cell(o.id)
                            {
                                let elem = obj.borrow().private_fields.get(&branded).cloned();
                                match elem {
                                    Some(PrivateElement::Field(v))
                                    | Some(PrivateElement::Method(v)) => {
                                        if matches!(v, JsValue::Null | JsValue::Undefined) {
                                            return Ok((JsValue::Undefined, JsValue::Undefined));
                                        }
                                        return self.eval_oc_tail_with_this(&v, chain, env);
                                    }
                                    Some(PrivateElement::Accessor { get, .. }) => {
                                        if let Some(getter) = get {
                                            let v = match self.call_function(&getter, &obj_val, &[])
                                            {
                                                Completion::Normal(v) => v,
                                                other => return Err(other),
                                            };
                                            if matches!(v, JsValue::Null | JsValue::Undefined) {
                                                return Ok((
                                                    JsValue::Undefined,
                                                    JsValue::Undefined,
                                                ));
                                            }
                                            return self.eval_oc_tail_with_this(&v, chain, env);
                                        }
                                        return Ok((JsValue::Undefined, JsValue::Undefined));
                                    }
                                    None => {
                                        return Err(Completion::Throw(self.create_type_error(
                                            &format!("Cannot read private member #{name}"),
                                        )));
                                    }
                                }
                            } else {
                                return Ok((JsValue::Undefined, JsValue::Undefined));
                            }
                        }
                    };
                    let prop_val = match self.access_property_on_value(&obj_val, &key) {
                        Completion::Normal(v) => v,
                        other => return Err(other),
                    };
                    Ok((prop_val, obj_val))
                }
            }
            Expression::OptionalChain(inner_base, inner_chain) => {
                // Nested optional chain: preserve this context from inner chain
                self.eval_optional_chain_with_ref(inner_base, inner_chain, env)
            }
            _ => {
                let val = match self.eval_expr(base, env) {
                    Completion::Normal(v) => v,
                    other => return Err(other),
                };
                Ok((val, JsValue::Undefined))
            }
        }
    }

    /// Evaluate optional chain tail with a known `this` from the base member access.
    /// This is used when the optional chain base is `obj.method?.()` so that
    /// the call uses `obj` as `this`.
    pub(super) fn eval_optional_chain_tail_with_base_this(
        &mut self,
        base_val: &JsValue,
        base_this: &JsValue,
        prop: &Expression,
        env: &EnvRef,
    ) -> Completion {
        match self.eval_oc_tail_with_this_ctx(base_val, base_this, prop, env) {
            Ok((v, _)) => Completion::Normal(v),
            Err(c) => c,
        }
    }

    /// Evaluate optional chain tail, returning (value, this_for_call).
    pub(super) fn eval_oc_tail_with_this(
        &mut self,
        base_val: &JsValue,
        prop: &Expression,
        env: &EnvRef,
    ) -> Result<(JsValue, JsValue), Completion> {
        self.eval_oc_tail_with_this_ctx(base_val, &JsValue::Undefined, prop, env)
    }

    /// Core optional chain tail evaluator with explicit this context.
    /// `chain_this` is the `this` value to use for `?.()` direct calls
    /// (from `obj.method?.()` where chain_this = obj).
    pub(super) fn eval_oc_tail_with_this_ctx(
        &mut self,
        base_val: &JsValue,
        chain_this: &JsValue,
        prop: &Expression,
        env: &EnvRef,
    ) -> Result<(JsValue, JsValue), Completion> {
        match prop {
            Expression::Identifier(name) => {
                if name.is_empty() {
                    // x?.() — direct call placeholder, base_val IS the value
                    // chain_this is the object for `obj.method?.()` calls
                    Ok((base_val.clone(), chain_this.clone()))
                } else {
                    let val = match self.access_property_on_value(base_val, name) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Err(Completion::Throw(e)),
                        other => return Err(other),
                    };
                    Ok((val, base_val.clone()))
                }
            }
            Expression::Call(callee, args, _) => {
                let (func_val, this_val) =
                    self.eval_oc_tail_with_this_ctx(base_val, chain_this, callee, env)?;
                let evaluated_args = match self.eval_spread_args(args, env) {
                    Ok(v) => v,
                    Err(e) => return Err(Completion::Throw(e)),
                };
                match self.call_function(&func_val, &this_val, &evaluated_args) {
                    Completion::Normal(v) => Ok((v, JsValue::Undefined)),
                    other => Err(other),
                }
            }
            Expression::Member(inner, mp, _) => {
                let (inner_val, _) =
                    self.eval_oc_tail_with_this_ctx(base_val, chain_this, inner, env)?;
                // Non-optional member access within optional chain: null/undefined throws
                if matches!(&inner_val, JsValue::Null | JsValue::Undefined) {
                    let key_str = match mp {
                        MemberProperty::Dot(name) => name.clone(),
                        MemberProperty::Computed(_) => "property".to_string(),
                        MemberProperty::Private(name) => format!("#{name}"),
                    };
                    return Err(Completion::Throw(self.create_type_error(&format!(
                        "Cannot read properties of {} (reading '{key_str}')",
                        if matches!(&inner_val, JsValue::Null) {
                            "null"
                        } else {
                            "undefined"
                        }
                    ))));
                }
                match mp {
                    MemberProperty::Dot(name) => {
                        let val = match self.access_property_on_value(&inner_val, name) {
                            Completion::Normal(v) => v,
                            other => return Err(other),
                        };
                        Ok((val, inner_val))
                    }
                    MemberProperty::Computed(expr) => {
                        let key_val = match self.eval_expr(expr, env) {
                            Completion::Normal(v) => v,
                            other => return Err(other),
                        };
                        let key = match self.to_property_key(&key_val) {
                            Ok(s) => s,
                            Err(e) => return Err(Completion::Throw(e)),
                        };
                        let val = match self.access_property_on_value(&inner_val, &key) {
                            Completion::Normal(v) => v,
                            other => return Err(other),
                        };
                        Ok((val, inner_val))
                    }
                    MemberProperty::Private(name) => {
                        let branded = self.resolve_private_name(name, env);
                        if let JsValue::Object(o) = &inner_val
                            && let Some(obj) = self.get_object_cell(o.id)
                        {
                            let elem = obj.borrow().private_fields.get(&branded).cloned();
                            match elem {
                                Some(PrivateElement::Field(v))
                                | Some(PrivateElement::Method(v)) => {
                                    Ok((v, inner_val))
                                }
                                Some(PrivateElement::Accessor { get, .. }) => {
                                    if let Some(getter) = get {
                                        match self.call_function(&getter, &inner_val, &[]) {
                                            Completion::Normal(v) => Ok((v, inner_val)),
                                            other => Err(other),
                                        }
                                    } else {
                                        Err(Completion::Throw(self.create_type_error(&format!(
                                            "Cannot read private member #{name} which has no getter"
                                        ))))
                                    }
                                }
                                None => Err(Completion::Throw(self.create_type_error(&format!(
                                    "Cannot read private member #{name} from an object whose class did not declare it"
                                )))),
                            }
                        } else {
                            Ok((JsValue::Undefined, inner_val))
                        }
                    }
                }
            }
            other => {
                // Computed property access (e.g., x?.[expr])
                let key_val = match self.eval_expr(other, env) {
                    Completion::Normal(v) => v,
                    other => return Err(other),
                };
                let key = match self.to_property_key(&key_val) {
                    Ok(s) => s,
                    Err(e) => return Err(Completion::Throw(e)),
                };
                let val = match self.access_property_on_value(base_val, &key) {
                    Completion::Normal(v) => v,
                    other => return Err(other),
                };
                Ok((val, base_val.clone()))
            }
        }
    }

    /// Handle `delete obj?.prop` and `delete obj?.['prop']` etc.
    pub(super) fn eval_delete_optional_chain(
        &mut self,
        base: &Expression,
        chain: &Expression,
        env: &EnvRef,
    ) -> Completion {
        // Evaluate the base of the optional chain
        let (base_val, _base_this) = match self.eval_oc_base(base, chain, env) {
            Ok(v) => v,
            Err(c) => return c,
        };
        // If base is null/undefined, short-circuit to true
        if matches!(base_val, JsValue::Null | JsValue::Undefined) {
            return Completion::Normal(JsValue::Boolean(true));
        }
        // Walk the chain to find the object and key to delete from
        self.eval_delete_oc_tail(&base_val, chain, env)
    }

    pub(super) fn eval_delete_oc_tail(
        &mut self,
        base_val: &JsValue,
        chain: &Expression,
        env: &EnvRef,
    ) -> Completion {
        match chain {
            Expression::Identifier(name) if !name.is_empty() => {
                // delete obj?.prop → delete obj.prop
                self.eval_delete_on_object(base_val, name, env)
            }
            Expression::Member(inner, mp, _) => {
                // Evaluate inner to get the object, then delete the last property
                let (inner_val, _) = match self.eval_oc_tail_with_this_ctx(
                    base_val,
                    &JsValue::Undefined,
                    inner,
                    env,
                ) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                if matches!(&inner_val, JsValue::Null | JsValue::Undefined) {
                    let key_str = match mp {
                        MemberProperty::Dot(name) => name.clone(),
                        MemberProperty::Computed(_) => "property".to_string(),
                        MemberProperty::Private(name) => format!("#{name}"),
                    };
                    return Completion::Throw(self.create_type_error(&format!(
                        "Cannot read properties of {} (reading '{key_str}')",
                        if matches!(&inner_val, JsValue::Null) {
                            "null"
                        } else {
                            "undefined"
                        }
                    )));
                }
                let key = match mp {
                    MemberProperty::Dot(name) => name.clone(),
                    MemberProperty::Computed(expr) => {
                        let v = match self.eval_expr(expr, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        match self.to_property_key(&v) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    MemberProperty::Private(_) => {
                        return Completion::Throw(
                            self.create_type_error("Private fields cannot be deleted"),
                        );
                    }
                };
                self.eval_delete_on_object(&inner_val, &key, env)
            }
            Expression::Call(callee, args, _) => {
                // delete obj?.method() — evaluate the call for side effects, return true
                let (func_val, this_val) = match self.eval_oc_tail_with_this_ctx(
                    base_val,
                    &JsValue::Undefined,
                    callee,
                    env,
                ) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let evaluated_args = match self.eval_spread_args(args, env) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                match self.call_function(&func_val, &this_val, &evaluated_args) {
                    Completion::Normal(_) => Completion::Normal(JsValue::Boolean(true)),
                    other => other,
                }
            }
            _ => {
                // Fallback: evaluate the chain for side effects, return true
                match self.eval_optional_chain_tail_with_base_this(
                    base_val,
                    &JsValue::Undefined,
                    chain,
                    env,
                ) {
                    Completion::Normal(_) => Completion::Normal(JsValue::Boolean(true)),
                    other => other,
                }
            }
        }
    }

    pub(super) fn eval_delete_on_object(
        &mut self,
        obj_val: &JsValue,
        key: &str,
        env: &EnvRef,
    ) -> Completion {
        let obj_val = if !matches!(obj_val, JsValue::Object(_)) {
            match self.to_object(obj_val) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Completion::Throw(e),
                _ => return Completion::Normal(JsValue::Boolean(true)),
            }
        } else {
            obj_val.clone()
        };
        if let JsValue::Object(ref o) = obj_val
            && let Some(obj) = self.get_object_cell(o.id)
        {
            if obj.borrow().is_proxy() || obj.borrow().is_proxy_revoked() {
                match self.proxy_delete_property(o.id, key) {
                    Ok(false) => {
                        if env.borrow().strict {
                            return Completion::Throw(self.create_type_error(&format!(
                                "Cannot delete property '{key}' of object"
                            )));
                        }
                        return Completion::Normal(JsValue::Boolean(false));
                    }
                    Ok(result) => return Completion::Normal(JsValue::Boolean(result)),
                    Err(e) => return Completion::Throw(e),
                }
            }
            let is_strict = env.borrow().strict;
            let mut obj_mut = obj.borrow_mut();
            if let Some(desc) = obj_mut.properties.get(key)
                && desc.configurable == Some(false)
            {
                if is_strict {
                    drop(obj_mut);
                    return Completion::Throw(
                        self.create_type_error(&format!(
                            "Cannot delete property '{key}' of object"
                        )),
                    );
                }
                return Completion::Normal(JsValue::Boolean(false));
            }
            obj_mut.remove_property(key);
            if let Some(map) = obj_mut.parameter_map_mut() {
                map.remove(key);
            }
            if let Ok(idx) = key.parse::<usize>()
                && let Some(elems) = obj_mut.array_elements_mut()
                && idx < elems.len()
            {
                elems[idx] = JsValue::Undefined;
            }
        }
        Completion::Normal(JsValue::Boolean(true))
    }

    /// Serve a property-access IC hit for a cached `kind`, assuming the
    /// receiver's `(obj_id, shape_id)` has already matched the cached entry.
    /// Returns `Some(value)` on a confirmed hit, or `None` to fall through to
    /// the slow path. Shared by the `Mono` and `Poly` probe paths.
    ///
    /// The receiver-shape match alone is not sufficient for the prototype-based
    /// kinds — reassigning `[[Prototype]]` (`__proto__` / `setPrototypeOf`)
    /// sets `prototype_id` directly WITHOUT bumping the receiver's shape_id — so
    /// `ProtoData`/`Missing` re-verify the prototype identity (and, for
    /// `ProtoData`, the prototype's shape) here. A stale entry self-heals: the
    /// caller's slow path re-records it.
    fn ic_probe_prop_kind(
        &self,
        obj_rc: &Rc<RefCell<JsObjectData>>,
        key: &str,
        kind: crate::interpreter::ic::PropIcKind,
    ) -> Option<JsValue> {
        use crate::interpreter::ic::PropIcKind;
        match kind {
            PropIcKind::OwnData => obj_rc
                .borrow()
                .properties
                .get(key)
                .and_then(|d| d.value.clone()),
            PropIcKind::Missing { proto_id, .. } => {
                if obj_rc.borrow().prototype_id == proto_id {
                    Some(JsValue::Undefined)
                } else {
                    None
                }
            }
            PropIcKind::ProtoData {
                proto_id,
                proto_shape_id,
            } => {
                if obj_rc.borrow().prototype_id == Some(proto_id)
                    && let Some(proto_rc) = self.get_object(proto_id)
                {
                    let pb = proto_rc.borrow();
                    if pb.shape_id == proto_shape_id {
                        return pb.properties.get(key).and_then(|d| d.value.clone());
                    }
                }
                None
            }
            // Reserved kinds are never recorded in this scope, so a probe
            // should never see them; treat as a miss defensively.
            PropIcKind::OwnAccessor | PropIcKind::TypedArrayElement => None,
        }
    }

    /// Classifies the post-lookup state of `(obj_id, key)` into a `PropIcEntry`
    /// ready for caching, or `None` if the site is not IC-able under the narrow
    /// scope. Caller feeds the result into `PropIcSlot::advance` after the slow
    /// path returns successfully. Issue #71, plan Step 11.
    ///
    /// Scope:
    /// - Object must NOT be a proxy / module-namespace / typed-array.
    /// - Property must resolve as own data (→ `OwnData`), as a depth-1
    ///   prototype data descriptor (→ `ProtoData`), or be absent when the
    ///   immediate prototype is null (→ `Missing`).
    /// - Own accessors, depth-1 prototype *accessors*, and depth>1 hits/misses
    ///   return `None` (caller leaves the slot Empty rather than caching).
    fn classify_for_prop_ic(
        &self,
        obj_id: u64,
        key: &str,
    ) -> Option<crate::interpreter::ic::PropIcEntry> {
        use crate::interpreter::ic::{PropIcEntry, PropIcKind};
        let obj_rc = self.get_object(obj_id)?;
        let obj = obj_rc.borrow();
        // Non-cacheable categories (plan "Excluded from IC in v1").
        if obj.proxy().is_some()
            || obj.module_namespace().is_some()
            || obj.typed_array_info().is_some()
        {
            return None;
        }
        // Own property?
        if let Some(d) = obj.properties.get(key) {
            if d.is_data_descriptor() {
                return Some(PropIcEntry {
                    obj_id,
                    obj_shape_id: obj.shape_id,
                    kind: PropIcKind::OwnData,
                });
            }
            // Own accessor — not recorded in v1 narrow scope.
            return None;
        }
        // Absent on receiver — only cacheable if the immediate prototype is
        // null (depth-1 Missing) or the immediate prototype carries the
        // property as a plain data descriptor (depth-1 ProtoData). Anything
        // deeper stays uncached.
        let proto_id = match obj.prototype_id {
            None => {
                return Some(PropIcEntry {
                    obj_id,
                    obj_shape_id: obj.shape_id,
                    kind: PropIcKind::Missing {
                        proto_id: None,
                        proto_shape_id: 0,
                    },
                });
            }
            Some(pid) => pid,
        };
        // Depth-1 prototype lookup. Drop the receiver borrow first so the
        // prototype borrow below can never alias it (proto could be the same
        // slot in pathological setups, though that would be a self-cycle).
        drop(obj);
        let proto_rc = self.get_object(proto_id)?;
        let proto = proto_rc.borrow();
        // The prototype must be an ordinary object whose own data property can
        // be fetched directly. A proxy/module-namespace/typed-array prototype
        // would require trap/index logic the probe doesn't perform.
        if proto.proxy().is_some()
            || proto.module_namespace().is_some()
            || proto.typed_array_info().is_some()
        {
            return None;
        }
        match proto.properties.get(key) {
            // Depth-1 data descriptor on the immediate prototype.
            Some(d) if d.is_data_descriptor() => {
                // Re-read the receiver shape (the borrow was dropped above).
                let obj_shape_id = obj_rc.borrow().shape_id;
                Some(PropIcEntry {
                    obj_id,
                    obj_shape_id,
                    kind: PropIcKind::ProtoData {
                        proto_id,
                        proto_shape_id: proto.shape_id,
                    },
                })
            }
            // Depth-1 accessor — excluded (no getter invocation in the probe).
            Some(_) => None,
            // Absent on the immediate prototype → resolves deeper than depth-1
            // (or is missing). Not cacheable in this narrow scope.
            None => None,
        }
    }

    pub(super) fn eval_member(
        &mut self,
        obj: &Expression,
        prop: &MemberProperty,
        env: &EnvRef,
        site_id: PropSiteId,
    ) -> Completion {
        // §13.3.7.1: super[expr] — special evaluation order:
        // 1. GetThisBinding (throws if uninitialized) — before key expression
        // 2. Evaluate key expression
        // 3. GetSuperBase (HomeObject.__proto__) — before ToPropertyKey
        // 4. ToPropertyKey (in GetValue on the reference)
        // 5. Property lookup on captured super base
        if matches!(obj, Expression::Super) {
            if let MemberProperty::Private(name) = prop {
                if Self::this_is_in_tdz(env) {
                    return Completion::Throw(self.create_reference_error(
                        "Must call super constructor in derived class before accessing 'this' or returning from derived constructor",
                    ));
                }
                let obj_val = match self.eval_expr(obj, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let branded = self.resolve_private_name(name, env);
                return match &obj_val {
                    JsValue::Object(o) => {
                        if let Some(obj_rc) = self.get_object_cell(o.id) {
                            let elem = obj_rc.borrow().private_fields.get(&branded).cloned();
                            match elem {
                                Some(PrivateElement::Field(v))
                                | Some(PrivateElement::Method(v)) => Completion::Normal(v),
                                Some(PrivateElement::Accessor { get, .. }) => {
                                    if let Some(getter) = get {
                                        self.call_function(&getter, &obj_val, &[])
                                    } else {
                                        Completion::Throw(self.create_type_error(&format!(
                                            "Cannot read private member #{name} which has no getter"
                                        )))
                                    }
                                }
                                None => Completion::Throw(self.create_type_error(&format!(
                                    "Cannot read private member #{name} from an object whose class did not declare it"
                                ))),
                            }
                        } else {
                            Completion::Normal(JsValue::Undefined)
                        }
                    }
                    _ => Completion::Throw(self.create_type_error(&format!(
                        "Cannot read private member #{name} from a non-object"
                    ))),
                };
            }

            // Step 2: GetThisBinding — throws ReferenceError if this is in TDZ
            if Self::this_is_in_tdz(env) {
                return Completion::Throw(self.create_reference_error(
                    "Must call super constructor in derived class before accessing 'this' or returning from derived constructor",
                ));
            }
            let this_val = env.borrow().get("this").unwrap_or(JsValue::Undefined);

            // Steps 3-4: Evaluate key expression (without ToPropertyKey yet)
            let raw_key = match prop {
                MemberProperty::Dot(name) => {
                    JsValue::String(crate::types::JsString::from_str(name))
                }
                MemberProperty::Computed(expr) => match self.eval_expr(expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                },
                MemberProperty::Private(_) => unreachable!(),
            };

            // Step 7 → §13.3.7.3 step 3: GetSuperBase — capture BEFORE ToPropertyKey
            let super_base_id = self.get_super_base_id(env);

            // §6.2.5.5 GetValue step 3.c.i: ToPropertyKey (deferred until after GetSuperBase)
            let key = match self.to_property_key(&raw_key) {
                Ok(s) => s,
                Err(e) => return Completion::Throw(e),
            };

            // Property lookup on captured super base
            match super_base_id {
                Some(base_id) => {
                    return self.get_object_property(base_id, &key, &this_val);
                }
                None => {
                    return Completion::Throw(self.create_type_error(&format!(
                        "Cannot read properties of null (reading '{key}')"
                    )));
                }
            }
        }

        let obj_val = match self.eval_expr(obj, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        if let MemberProperty::Private(name) = prop {
            let branded = self.resolve_private_name(name, env);
            return match &obj_val {
                JsValue::Object(o) => {
                    if let Some(obj) = self.get_object_cell(o.id) {
                        let elem = obj.borrow().private_fields.get(&branded).cloned();
                        match elem {
                            Some(PrivateElement::Field(v)) | Some(PrivateElement::Method(v)) => {
                                Completion::Normal(v)
                            }
                            Some(PrivateElement::Accessor { get, .. }) => {
                                if let Some(getter) = get {
                                    self.call_function(&getter, &obj_val, &[])
                                } else {
                                    Completion::Throw(self.create_type_error(&format!(
                                        "Cannot read private member #{name} which has no getter"
                                    )))
                                }
                            }
                            None => Completion::Throw(self.create_type_error(&format!(
                                "Cannot read private member #{name} from an object whose class did not declare it"
                            ))),
                        }
                    } else {
                        Completion::Normal(JsValue::Undefined)
                    }
                }
                _ => Completion::Throw(self.create_type_error(&format!(
                    "Cannot read private member #{name} from a non-object"
                ))),
            };
        }
        // For computed properties, evaluate the expression but defer ToPropertyKey
        // until after we check that the base is not null/undefined (spec: ToObject
        // precedes ToPropertyKey per §6.2.5.5 GetValue step 3.a vs 3.c.i).
        let (key, computed_raw) = match prop {
            MemberProperty::Dot(name) => (name.clone(), None),
            MemberProperty::Computed(expr) => {
                let v = match self.eval_expr(expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if matches!(&obj_val, JsValue::Null | JsValue::Undefined) {
                    let err = self.create_type_error(&format!(
                        "Cannot read properties of {obj_val} (reading property)"
                    ));
                    return Completion::Throw(err);
                }
                // Fast path: numeric index on typed array or array object
                if let JsValue::Number(index) = &v
                    && let JsValue::Object(o) = &obj_val
                    && let Some(obj_rc) = self.get_object_cell(o.id)
                {
                    let obj_borrow = obj_rc.borrow();
                    // Typed array: direct element access
                    if let Some(ta) = obj_borrow.typed_array_info() {
                        use crate::interpreter::types::{
                            is_valid_integer_index, typed_array_get_index,
                        };
                        if is_valid_integer_index(ta, *index) {
                            let result = typed_array_get_index(ta, *index as usize);
                            return Completion::Normal(result);
                        }
                        // Any canonical numeric index on typed array that's
                        // out of range returns undefined (no prototype walk)
                        let trunc = index.trunc();
                        if *index == trunc && !index.is_nan() && !index.is_sign_negative() {
                            return Completion::Normal(JsValue::Undefined);
                        }
                    }
                    // Array: direct element access (skip if index overridden by defineProperty)
                    if let Some(elems) = obj_borrow.array_elements() {
                        let trunc = index.trunc();
                        if *index == trunc && *index >= 0.0 && (*index as usize) < elems.len() {
                            let idx = *index as usize;
                            let key_str = (idx as u32).to_string();
                            if !obj_borrow.properties.contains_key(&key_str)
                                && !matches!(elems[idx], JsValue::Undefined)
                            {
                                return Completion::Normal(elems[idx].clone());
                            }
                        }
                    }
                }
                let key = match self.to_property_key(&v) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                (key, Some(()))
            }
            MemberProperty::Private(_) => unreachable!(),
        };
        let _ = computed_raw;
        match &obj_val {
            JsValue::Object(o) => {
                // Phase 2 IC: probe + record for Dot access only (v1 scope).
                // Computed access goes straight to the slow path; caching it
                // requires extra logic for non-string keys and is deferred.
                if matches!(prop, MemberProperty::Dot(_))
                    && site_id != PropSiteId::UNASSIGNED
                    && self.with_scope_depth == 0
                {
                    use crate::interpreter::ic::{PropIcKind, PropIcSlot};
                    // Probe: find the cached entry whose object identity matches
                    // the current receiver, without copying the slot (the `Poly`
                    // variant owns a heap `Vec`). Extract just the Copy fields we
                    // need for the shape check + dispatch, then release the
                    // borrow before touching any object.
                    let cached: Option<(u64, PropIcKind)> = match self.prop_slot(site_id) {
                        PropIcSlot::Mono {
                            obj_id,
                            obj_shape_id,
                            kind,
                        } if *obj_id == o.id => Some((*obj_shape_id, *kind)),
                        PropIcSlot::Poly(entries) => entries
                            .iter()
                            .find(|e| e.obj_id == o.id)
                            .map(|e| (e.obj_shape_id, e.kind)),
                        _ => None,
                    };
                    if let Some((want_shape, kind)) = cached
                        && let Some(obj_rc) = self.get_object(o.id)
                        && obj_rc.borrow().shape_id == want_shape
                        && let Some(v) = self.ic_probe_prop_kind(&obj_rc, &key, kind)
                    {
                        // Shape match + confirmed dispatch — IC hit.
                        self.ic_hit_count.set(self.ic_hit_count.get() + 1);
                        return Completion::Normal(v);
                    }
                    // Probe miss — snapshot the pre-miss slot BEFORE the slow
                    // path, then record afterwards. `get_object_property` can
                    // run user code (an accessor getter or a proxy `get` trap)
                    // that re-enters this same body + `PropSiteId` and mutates
                    // the slot; capturing here keeps the state-machine
                    // transition operating on the slot as it stood before this
                    // access, honoring `PropIcSlot::advance`'s contract. The
                    // clone is on the cold miss path only and allocates nothing
                    // unless the slot was already `Poly`.
                    self.ic_slow_path_count
                        .set(self.ic_slow_path_count.get() + 1);
                    let prev = self.prop_slot(site_id).clone();
                    let result = self.get_object_property(o.id, &key, &obj_val.clone());
                    if let Completion::Normal(_) = &result {
                        // Classify for IC recording. Cheap one-borrow walk.
                        let observed = self.classify_for_prop_ic(o.id, &key);
                        // Drive the Empty → Mono → Poly → Megamorphic machine.
                        *self.prop_slot(site_id) = prev.advance(observed);
                    }
                    return result;
                }
                self.get_object_property(o.id, &key, &obj_val.clone())
            }
            JsValue::String(s) => {
                if key == "length" {
                    Completion::Normal(JsValue::Number(s.len() as f64))
                } else if let Ok(idx) = key.parse::<usize>() {
                    if idx < s.code_units.len() {
                        Completion::Normal(JsValue::String(JsString::from_vec(vec![
                            s.code_units[idx],
                        ])))
                    } else {
                        Completion::Normal(JsValue::Undefined)
                    }
                } else {
                    let wrapper = match self.to_object(&obj_val) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if let JsValue::Object(ref o) = wrapper {
                        self.get_object_property(o.id, &key, &obj_val)
                    } else {
                        Completion::Normal(JsValue::Undefined)
                    }
                }
            }
            JsValue::Symbol(_) | JsValue::Number(_) | JsValue::Boolean(_) | JsValue::BigInt(_) => {
                let wrapper = match self.to_object(&obj_val) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if let JsValue::Object(ref o) = wrapper {
                    self.get_object_property(o.id, &key, &obj_val)
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::Undefined | JsValue::Null => {
                let err = self.create_type_error(&format!(
                    "Cannot read properties of {obj_val} (reading '{key}')"
                ));
                Completion::Throw(err)
            }
        }
    }
}
