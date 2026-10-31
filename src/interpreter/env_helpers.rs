//! Interpreter-side wrappers around the slim `Environment` methods. They
//! re-introduce the global-object mirroring that PR 1b.4 removed from
//! `Environment::set/get/has/declare_global_*` so those methods could be
//! free of `Rc<RefCell<JsObjectData>>` field access.
//!
//! Borrow-ordering rule: every wrapper drops all `RefCell` borrows on `env`
//! before invoking `proxy_set` / `set_property_value` on the global object.
//! A re-entrant `set` trap that calls back into `env_set` would otherwise
//! double-borrow the same env.

use super::*;
use crate::types::JsValue;

impl Interpreter {
    /// Read a name from the current realm's global env, falling through to
    /// the global object's own properties when env.bindings doesn't carry the
    /// name (which is the case for built-ins after `setup_globals` strips them
    /// from env.bindings — see builtins/mod.rs:3995-4004).
    pub(crate) fn get_global_var(&mut self, name: &str) -> Option<JsValue> {
        let env = self.realm().global_env.clone();
        self.env_get(&env, name)
    }

    /// Test-only `&self` variant of `get_global_var`.
    /// Skips Proxy traps on the global object, which is sufficient for test
    /// assertions that inspect the global object's own properties.
    #[cfg(test)]
    pub(crate) fn get_global_var_ref(&self, name: &str) -> Option<JsValue> {
        let env = self.realm().global_env.borrow();
        if let Some(v) = env.get(name) {
            return Some(v);
        }
        let gid = env.global_object_id?;
        drop(env);
        self.get_object_cell(gid)
            .and_then(|go| go.borrow().own_property_lookup(name))
    }

    /// SetMutableBinding along the scope chain; a name no declarative scope
    /// binds is written to the global object with [[Set]] (setters and a
    /// Proxy global see it).
    pub(crate) fn env_set<N: BindingName + ?Sized>(
        &mut self,
        env: &EnvRef,
        key: &N,
        value: JsValue,
    ) -> Result<(), JsValue> {
        let name = key.as_name();
        let mut current = env.clone();
        loop {
            // A declarative local write needs no re-entrant object operation,
            // so update it while this single map probe is live. Global writes
            // still defer mutation until after the mirrored object write.
            let action = {
                let mut e = current.borrow_mut();
                if e.is_indirect_binding(name) {
                    return Err(JsValue::from_str("Assignment to constant variable."));
                }
                let strict = e.strict;
                let global_object_id = e.global_object_id;
                if let Some(binding) = e.bindings.get_mut(key) {
                    if !binding.initialized
                        && matches!(binding.kind, BindingKind::Let | BindingKind::Const)
                    {
                        return Err(JsValue::from_str(&format!(
                            "Cannot access '{name}' before initialization"
                        )));
                    }
                    if binding.kind == BindingKind::Const && binding.initialized {
                        return Err(JsValue::from_str("Assignment to constant variable."));
                    }
                    if (binding.kind == BindingKind::FunctionName
                        || binding.kind == BindingKind::ImmutableValue)
                        && binding.initialized
                    {
                        if strict {
                            return Err(JsValue::from_str("Assignment to constant variable."));
                        }
                        return Ok(());
                    }
                    binding.value = value;
                    binding.initialized = true;
                    return Ok(());
                } else if e.parent.is_some() {
                    EnvSetAction::Recurse(e.parent.clone().unwrap())
                } else if let Some(gid) = global_object_id {
                    EnvSetAction::ImplicitGlobal { gid }
                } else {
                    EnvSetAction::CreateBinding
                }
            };

            match action {
                EnvSetAction::Recurse(parent) => {
                    current = parent;
                }
                EnvSetAction::ImplicitGlobal { gid } => {
                    self.set_global_object_property(gid, name, value)?;
                    return Ok(());
                }
                EnvSetAction::CreateBinding => {
                    key.insert_into(
                        &mut current.borrow_mut().bindings,
                        Binding::new(value, BindingKind::Var, true),
                    );
                    return Ok(());
                }
            }
        }
    }

    /// Lookup a name, falling through to the global object's own properties
    /// when the chain bottoms out at the global env.
    pub(crate) fn env_get(&mut self, env: &EnvRef, name: &str) -> Option<JsValue> {
        // First try slim Environment::get.
        if let Some(v) = env.borrow().get(name) {
            return Some(v);
        }
        // Walk to find an env with global_object_id set; fall through.
        let mut current = Some(env.clone());
        while let Some(e_ref) = current {
            let e = e_ref.borrow();
            if e.bindings.contains_key(name) || e.is_indirect_binding(name) {
                return None;
            }
            if let Some(gid) = e.global_object_id {
                drop(e);
                if self.get_proxy_info(gid).is_some() {
                    if !self.proxy_has_property(gid, name).ok()? {
                        return None;
                    }
                    return match self.get_object_property(gid, name, &JsValue::object(gid)) {
                        Completion::Normal(v) => Some(v),
                        _ => None,
                    };
                }
                if let Some(go) = self.get_object_cell(gid)
                    && let Some(v) = go.borrow().own_property_lookup(name)
                {
                    return Some(v);
                }
                return None;
            }
            current = e.parent.clone();
        }
        None
    }

    /// `Environment::has` with global-object fall-through.
    pub(crate) fn env_has(&mut self, env: &EnvRef, name: &str) -> bool {
        if env.borrow().has(name) {
            return true;
        }
        let mut current = Some(env.clone());
        while let Some(e_ref) = current {
            let e = e_ref.borrow();
            if e.bindings.contains_key(name) || e.is_indirect_binding(name) {
                return true;
            }
            if let Some(gid) = e.global_object_id {
                drop(e);
                if self.get_proxy_info(gid).is_some() {
                    return self.proxy_has_property(gid, name).unwrap_or(false);
                }
                if let Some(go) = self.get_object_cell(gid) {
                    return go.borrow().own_has_property(name).unwrap_or(false);
                }
                return false;
            }
            current = e.parent.clone();
        }
        false
    }

    /// Walk the scope chain and check what error (if any) setting a binding
    /// would produce. Mirrors the static `Environment::check_set_binding` but
    /// reads the global object via the slab.
    pub(crate) fn env_check_set_binding<N: BindingName + ?Sized>(
        &self,
        env: &EnvRef,
        key: &N,
    ) -> SetBindingCheck {
        let name = key.as_name();
        let mut current = env.clone();
        loop {
            let next = {
                let e = current.borrow();
                if e.is_indirect_binding(name) {
                    return SetBindingCheck::ConstAssign;
                }
                if let Some(binding) = e.bindings.get(key) {
                    if !binding.initialized && binding.kind != BindingKind::Var {
                        return SetBindingCheck::TdzError;
                    }
                    if binding.kind == BindingKind::Const && binding.initialized {
                        return SetBindingCheck::ConstAssign;
                    }
                    if (binding.kind == BindingKind::FunctionName
                        || binding.kind == BindingKind::ImmutableValue)
                        && binding.initialized
                    {
                        return SetBindingCheck::FunctionNameAssign;
                    }
                    return SetBindingCheck::Ok;
                }
                if let Some(gid) = e.global_object_id {
                    drop(e);
                    if let Some(go) = self.get_object_cell(gid)
                        && matches!(go.borrow().own_has_property(name), Some(true))
                    {
                        return SetBindingCheck::Ok;
                    }
                    return SetBindingCheck::Unresolvable;
                }
                e.parent.clone()
            };
            match next {
                Some(parent) => current = parent,
                None => return SetBindingCheck::Unresolvable,
            }
        }
    }
}

enum EnvSetAction {
    Recurse(EnvRef),
    ImplicitGlobal { gid: u64 },
    CreateBinding,
}
