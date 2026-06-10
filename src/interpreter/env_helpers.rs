//! Interpreter-side wrappers around the slim `Environment` methods. They
//! re-introduce the global-object mirroring that PR 1b.4 removed from
//! `Environment::set/get/has/declare_global_*` so those methods could be
//! free of `Rc<RefCell<JsObjectData>>` field access.
//!
//! Borrow-ordering rule: every wrapper drops all `RefCell` borrows on `env`
//! before invoking `proxy_set` / `set_property_value` on the global object.
//! A re-entrant `set` trap that calls back into `env_set` would otherwise
//! double-borrow the same env.

use super::types::*;
use super::*;
use crate::types::{JsString, JsValue};

impl Interpreter {
    /// Read a name from the current realm's global env, falling through to
    /// the global object's own properties when env.bindings doesn't carry the
    /// name (which is the case for built-ins after `setup_globals` strips them
    /// from env.bindings — see builtins/mod.rs:3995-4004).
    pub(crate) fn get_global_var(&mut self, name: &str) -> Option<JsValue> {
        let env = self.realm().global_env.clone();
        self.env_get(&env, name)
    }

    /// `&self` variant of `get_global_var` for read-only call sites that don't
    /// have a `&mut Interpreter` (e.g. helpers like `get_symbol_iterator_key`).
    /// Skips Proxy traps on the global object — sufficient for built-in name
    /// lookups that should resolve via own properties.
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

    /// Mirrors `Environment::set`'s old chain walk + global-object mirror,
    /// preserving sloppy-mode implicit globals and the var-binding mirror at
    /// the global env.
    pub(crate) fn env_set(
        &mut self,
        env: &EnvRef,
        name: &str,
        value: JsValue,
    ) -> Result<(), JsValue> {
        let mut current = env.clone();
        loop {
            // Inspect this frame without mutating; capture the next action.
            let action = {
                let e = current.borrow();
                if e.is_indirect_binding(name) {
                    return Err(JsValue::String(JsString::from_str(
                        "Assignment to constant variable.",
                    )));
                }
                if let Some(binding) = e.bindings.get(name) {
                    if !binding.initialized
                        && matches!(binding.kind, BindingKind::Let | BindingKind::Const)
                    {
                        return Err(JsValue::String(JsString::from_str(&format!(
                            "Cannot access '{name}' before initialization"
                        ))));
                    }
                    if binding.kind == BindingKind::Const && binding.initialized {
                        return Err(JsValue::String(JsString::from_str(
                            "Assignment to constant variable.",
                        )));
                    }
                    if (binding.kind == BindingKind::FunctionName
                        || binding.kind == BindingKind::ImmutableValue)
                        && binding.initialized
                    {
                        if e.strict {
                            return Err(JsValue::String(JsString::from_str(
                                "Assignment to constant variable.",
                            )));
                        }
                        return Ok(());
                    }
                    let mirror = if binding.kind == BindingKind::Var {
                        e.global_object_id
                    } else {
                        None
                    };
                    EnvSetAction::WriteBinding {
                        mirror_gid: mirror,
                        strict: e.strict,
                    }
                } else if e.parent.is_some() {
                    EnvSetAction::Recurse(e.parent.clone().unwrap())
                } else if let Some(gid) = e.global_object_id {
                    EnvSetAction::ImplicitGlobal { gid }
                } else {
                    EnvSetAction::CreateBinding
                }
            };

            match action {
                EnvSetAction::WriteBinding { mirror_gid, strict } => {
                    if let Some(gid) = mirror_gid {
                        // Borrow on `current` is dropped above; safe to call
                        // out to the slab (re-entrant set traps OK).
                        if let Some(go) = self.get_object_cell(gid) {
                            let ok = go.borrow_mut().set_property_value(name, value.clone());
                            if !ok {
                                if strict {
                                    return Err(JsValue::String(JsString::from_str(&format!(
                                        "Cannot assign to read only property '{name}' of object '#<Object>'"
                                    ))));
                                }
                                return Ok(());
                            }
                        }
                    }
                    let mut e = current.borrow_mut();
                    if let Some(b) = e.bindings.get_mut(name) {
                        b.value = value;
                        b.initialized = true;
                    }
                    return Ok(());
                }
                EnvSetAction::Recurse(parent) => {
                    current = parent;
                }
                EnvSetAction::ImplicitGlobal { gid } => {
                    let already_on_global = self
                        .get_object_cell(gid)
                        .is_some_and(|go| go.borrow().has_own_property(name));
                    let ok = self
                        .get_object_cell(gid)
                        .is_some_and(|go| go.borrow_mut().set_property_value(name, value.clone()));
                    if !ok {
                        return Ok(());
                    }
                    if already_on_global {
                        return Ok(());
                    }
                    current.borrow_mut().bindings.insert(
                        name.to_string(),
                        Binding::new(value, BindingKind::Var, true),
                    );
                    return Ok(());
                }
                EnvSetAction::CreateBinding => {
                    current.borrow_mut().bindings.insert(
                        name.to_string(),
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

    /// CreateGlobalVarBinding (§9.1.1.4.17): declare a non-configurable Var
    /// on the global object when env is the global env, otherwise just declare
    /// in env.bindings.
    pub(crate) fn env_declare_global_var(&mut self, env: &EnvRef, name: &str) {
        let gid = env.borrow().global_object_id;
        if let Some(gid) = gid {
            if let Some(go) = self.get_object_cell(gid) {
                let mut g = go.borrow_mut();
                if !g.properties.contains_key(name) {
                    let key = crate::interpreter::key_intern::intern_key(name);
                    g.property_order.push(Rc::clone(&key));
                    g.properties.insert(
                        key,
                        PropertyDescriptor::data(JsValue::Undefined, true, true, false),
                    );
                }
            }
        } else {
            env.borrow_mut().declare_global_var(name);
        }
    }

    /// CreateGlobalVarBinding with configurable=true (eval-declared vars).
    pub(crate) fn env_declare_global_var_configurable(&mut self, env: &EnvRef, name: &str) {
        let gid = env.borrow().global_object_id;
        if let Some(gid) = gid {
            if !env.borrow().bindings.contains_key(name) {
                let has_global_prop = self
                    .get_object_cell(gid)
                    .is_some_and(|g| g.borrow().properties.contains_key(name));
                if !has_global_prop {
                    env.borrow_mut().declare(name, BindingKind::Var);
                }
            }
            if let Some(go) = self.get_object_cell(gid) {
                let mut g = go.borrow_mut();
                if !g.properties.contains_key(name) {
                    let key = crate::interpreter::key_intern::intern_key(name);
                    g.property_order.push(Rc::clone(&key));
                    g.properties.insert(
                        key,
                        PropertyDescriptor::data(JsValue::Undefined, true, true, true),
                    );
                }
            }
        } else {
            env.borrow_mut().declare_global_var_configurable(name);
        }
    }

    /// Walk the scope chain and check what error (if any) setting a binding
    /// would produce. Mirrors the static `Environment::check_set_binding` but
    /// reads the global object via the slab.
    pub(crate) fn env_check_set_binding(&self, env: &EnvRef, name: &str) -> SetBindingCheck {
        let mut current = env.clone();
        loop {
            let next = {
                let e = current.borrow();
                if e.is_indirect_binding(name) {
                    return SetBindingCheck::ConstAssign;
                }
                if let Some(binding) = e.bindings.get(name) {
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

    /// CreateGlobalFunctionBinding (eval-declared functions).
    pub(crate) fn env_declare_global_function_binding(
        &mut self,
        env: &EnvRef,
        name: &str,
        value: JsValue,
        configurable: bool,
    ) {
        env.borrow_mut()
            .declare_global_function_binding(name, value.clone(), configurable);
        let gid = env.borrow().global_object_id;
        if let Some(gid) = gid
            && let Some(go) = self.get_object_cell(gid)
        {
            let mut g = go.borrow_mut();
            let existing = g.properties.get(name);
            let need_full_desc =
                existing.is_none() || existing.is_some_and(|d| d.configurable == Some(true));
            if need_full_desc {
                let desc = PropertyDescriptor::data(value, true, true, configurable);
                let key = crate::interpreter::key_intern::intern_key(name);
                if !g.properties.contains_key(name) {
                    g.property_order.push(Rc::clone(&key));
                }
                g.properties.insert(key, desc);
            } else {
                g.set_property_value(name, value);
            }
        }
    }
}

enum EnvSetAction {
    WriteBinding {
        mirror_gid: Option<u64>,
        strict: bool,
    },
    Recurse(EnvRef),
    ImplicitGlobal {
        gid: u64,
    },
    CreateBinding,
}
