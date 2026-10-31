//! The Object Environment Record half of the Global Environment Record
//! (§9.1.1.4): declaration instantiation, `delete` and implicit-global writes
//! go through the global object's internal methods ([[GetOwnProperty]],
//! [[IsExtensible]], [[DefineOwnProperty]], [[HasProperty]], [[Set]],
//! [[Delete]]), so accessors and a Proxy global object see them (jsse#1346).
//!
//! The global environment keeps only the declarative record (`let`, `const`,
//! `class`, and the immutable `undefined`/`NaN`/`Infinity`); `var`, function
//! and implicit-global names live on the global object alone.

use super::super::*;
use crate::types::JsPropertyKey;

impl Interpreter {
    /// §9.1.1.4.14 HasRestrictedGlobalProperty
    pub(crate) fn has_restricted_global_property(
        &mut self,
        gid: u64,
        name: &str,
    ) -> Result<bool, JsValue> {
        let existing = self.proxy_get_own_property_native(gid, name)?;
        Ok(existing.is_some_and(|d| d.configurable != Some(true)))
    }

    /// §9.1.1.4.15 CanDeclareGlobalVar
    pub(crate) fn can_declare_global_var(&mut self, gid: u64, name: &str) -> Result<bool, JsValue> {
        if self.proxy_get_own_property_native(gid, name)?.is_some() {
            return Ok(true);
        }
        self.proxy_is_extensible(gid)
    }

    /// §9.1.1.4.16 CanDeclareGlobalFunction
    pub(crate) fn can_declare_global_function(
        &mut self,
        gid: u64,
        name: &str,
    ) -> Result<bool, JsValue> {
        let Some(existing) = self.proxy_get_own_property_native(gid, name)? else {
            return self.proxy_is_extensible(gid);
        };
        if existing.configurable == Some(true) {
            return Ok(true);
        }
        let is_data = existing.get.is_none() && existing.set.is_none();
        Ok(is_data && existing.writable == Some(true) && existing.enumerable == Some(true))
    }

    /// `Set(globalObject, name, value, false)`: the sloppy write behind an
    /// unresolvable reference and behind InitializeBinding. The result says
    /// whether [[Set]] succeeded.
    pub(crate) fn set_global_object_property(
        &mut self,
        gid: u64,
        name: &str,
        value: JsValue,
    ) -> Result<bool, JsValue> {
        let receiver = JsValue::object(gid);
        self.with_gc_root_scope(|interp| {
            interp.gc_root_value(&value);
            interp.proxy_set(gid, name, value, &receiver)
        })
    }

    fn define_global_property_or_throw(
        &mut self,
        gid: u64,
        name: &str,
        desc: PropertyDescriptor,
    ) -> Result<(), JsValue> {
        let key = JsPropertyKey::from(name);
        if self.proxy_define_own_property_native(gid, key, desc)? {
            Ok(())
        } else {
            Err(self.create_type_error(&format!("Cannot redefine property: {name}")))
        }
    }

    /// §9.1.1.4.17 CreateGlobalVarBinding
    pub(crate) fn create_global_var_binding(
        &mut self,
        gid: u64,
        name: &str,
        deletable: bool,
    ) -> Result<(), JsValue> {
        let has_property = self.proxy_get_own_property_native(gid, name)?.is_some();
        let extensible = self.proxy_is_extensible(gid)?;
        if has_property || !extensible {
            return Ok(());
        }
        let desc = PropertyDescriptor::data(JsValue::UNDEFINED, true, true, deletable);
        self.define_global_property_or_throw(gid, name, desc)?;
        if self.get_proxy_info(gid).is_some() && self.proxy_has_property(gid, name)? {
            // InitializeBinding is SetMutableBinding(name, undefined, false)
            self.set_global_object_property(gid, name, JsValue::UNDEFINED)?;
        }
        Ok(())
    }

    /// §9.1.1.4.18 CreateGlobalFunctionBinding
    pub(crate) fn create_global_function_binding(
        &mut self,
        gid: u64,
        name: &str,
        value: JsValue,
        deletable: bool,
    ) -> Result<(), JsValue> {
        self.with_gc_root_scope(|interp| {
            interp.gc_root_value(&value);
            let existing = interp.proxy_get_own_property_native(gid, name)?;
            let desc = if existing.is_none_or(|d| d.configurable == Some(true)) {
                PropertyDescriptor::data(value.clone(), true, true, deletable)
            } else {
                PropertyDescriptor {
                    value: Some(value.clone()),
                    writable: None,
                    get: None,
                    set: None,
                    enumerable: None,
                    configurable: None,
                }
            };
            interp.define_global_property_or_throw(gid, name, desc)?;
            interp.set_global_object_property(gid, name, value)?;
            Ok(())
        })
    }

    /// `delete name` that resolved to the global environment (§9.1.1.4.7
    /// DeleteBinding): a declarative binding is not deletable, otherwise the
    /// global object's [[Delete]] answers if it has the name.
    pub(crate) fn delete_global_binding(&mut self, name: &str) -> Result<bool, JsValue> {
        let env = self.realm().global_env.clone();
        let gid = {
            let e = env.borrow();
            if e.bindings.contains_key(name) {
                return Ok(false);
            }
            e.global_object_id
        };
        let Some(gid) = gid else {
            return Ok(true);
        };
        if !self.proxy_has_property(gid, name)? {
            return Ok(true);
        }
        self.proxy_delete_property(gid, name)
    }

    /// §16.1.7 GlobalDeclarationInstantiation steps 3 to 12 for the checks
    /// that run before any binding is created. `Some` is the thrown error.
    pub(crate) fn check_global_declarations(
        &mut self,
        stmts: &[Statement],
        env: &EnvRef,
    ) -> Option<Completion> {
        let gid = env.borrow().global_object_id?;
        self.check_global_declarations_at(stmts, env, gid)
            .err()
            .map(Completion::Throw)
    }

    fn lexical_kind_in(env: &EnvRef, name: &str) -> bool {
        env.borrow()
            .bindings
            .get(name)
            .is_some_and(|b| matches!(b.kind, BindingKind::Let | BindingKind::Const))
    }

    fn redeclared(&mut self, name: &str) -> JsValue {
        self.create_error(
            "SyntaxError",
            &format!("Identifier '{name}' has already been declared"),
        )
    }

    fn check_global_declarations_at(
        &mut self,
        stmts: &[Statement],
        env: &EnvRef,
        gid: u64,
    ) -> Result<(), JsValue> {
        for name in Self::collect_lex_names(stmts) {
            if Self::lexical_kind_in(env, &name)
                || self.has_restricted_global_property(gid, &name)?
            {
                return Err(self.redeclared(&name));
            }
        }

        let mut var_names = HashSet::new();
        Self::collect_var_names_from_stmts(stmts, &mut var_names);
        let mut var_names: Vec<String> = var_names.into_iter().collect();
        var_names.sort();
        for name in &var_names {
            if Self::lexical_kind_in(env, name) {
                return Err(self.redeclared(name));
            }
        }

        let mut function_names: Vec<String> = Vec::new();
        for stmt in stmts.iter().rev() {
            let Some(f) = super::super::hoisting::unwrap_labeled_function(stmt) else {
                continue;
            };
            if function_names.contains(&f.name) {
                continue;
            }
            if Self::lexical_kind_in(env, &f.name) {
                return Err(self.redeclared(&f.name));
            }
            if !self.can_declare_global_function(gid, &f.name)? {
                return Err(
                    self.create_type_error(&format!("Cannot declare global function '{}'", f.name))
                );
            }
            function_names.push(f.name.clone());
        }

        for name in &var_names {
            if !function_names.contains(name) && !self.can_declare_global_var(gid, name)? {
                return Err(
                    self.create_type_error(&format!("Cannot declare global variable '{name}'"))
                );
            }
        }
        Ok(())
    }

    /// GlobalDeclarationInstantiation steps 16 to 18: a binding for each
    /// top-level function declaration, then for each declared `var` name.
    pub(crate) fn instantiate_global_functions_and_vars(
        &mut self,
        stmts: &[Statement],
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        let Some(gid) = env.borrow().global_object_id else {
            return Ok(());
        };
        for stmt in stmts {
            if let Some(f) = super::super::hoisting::unwrap_labeled_function(stmt)
                && f.phase != DeclPhase::CopyOnly
            {
                let value = self.hoisted_function_value(f, env);
                self.create_global_function_binding(gid, &f.name, value, false)?;
            }
        }
        self.create_global_var_bindings(stmts, env)
    }

    /// The var half of GlobalDeclarationInstantiation (steps 17 to 18):
    /// `CreateGlobalVarBinding` for each declared name, in source order.
    pub(crate) fn create_global_var_bindings(
        &mut self,
        stmts: &[Statement],
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        let Some(gid) = env.borrow().global_object_id else {
            return Ok(());
        };
        let mut seen = HashSet::new();
        for name in super::super::hoisting::collect_var_names(stmts) {
            if !seen.insert(name.clone()) || env.borrow().bindings.contains_key(name.as_str()) {
                continue;
            }
            self.create_global_var_binding(gid, &name, false)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
