//! Interpreter-side wrappers around the slim `Environment` methods: the
//! global-object fall-through of lookups and the `[[Set]]` of a name no
//! declarative scope binds (the Object Environment Record half of the global
//! environment; see `global_access/record.rs`).
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
                    EnvSetAction::ConstAssign
                } else {
                    let strict = e.strict;
                    let global_object_id = e.global_object_id;
                    if let Some(binding) = e.bindings.get_mut(key) {
                        if !binding.initialized
                            && matches!(binding.kind, BindingKind::Let | BindingKind::Const)
                        {
                            EnvSetAction::Tdz
                        } else if binding.kind == BindingKind::Const && binding.initialized {
                            EnvSetAction::ConstAssign
                        } else if (binding.kind == BindingKind::FunctionName
                            || binding.kind == BindingKind::ImmutableValue)
                            && binding.initialized
                        {
                            if strict {
                                EnvSetAction::ConstAssign
                            } else {
                                EnvSetAction::Ignored
                            }
                        } else {
                            binding.value = value;
                            binding.initialized = true;
                            return Ok(());
                        }
                    } else if e.parent.is_some() {
                        EnvSetAction::Recurse(e.parent.clone().unwrap())
                    } else if let Some(gid) = global_object_id {
                        EnvSetAction::ImplicitGlobal { gid }
                    } else {
                        EnvSetAction::CreateBinding
                    }
                }
            };

            match action {
                EnvSetAction::Ignored => return Ok(()),
                EnvSetAction::Tdz => {
                    return Err(self.create_reference_error(&format!(
                        "Cannot access '{name}' before initialization"
                    )));
                }
                EnvSetAction::ConstAssign => return Err(self.const_assign_error(name, &current)),
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
    Ignored,
    Tdz,
    ConstAssign,
    Recurse(EnvRef),
    ImplicitGlobal { gid: u64 },
    CreateBinding,
}

impl Binding {
    /// An initialized `var`/`let` binding: a plain write needs no TDZ or
    /// immutability handling, so assignment fast paths may skip the full
    /// reference resolution.
    pub(crate) fn is_plain_mutable(&self) -> bool {
        self.initialized && matches!(self.kind, BindingKind::Var | BindingKind::Let)
    }
}
