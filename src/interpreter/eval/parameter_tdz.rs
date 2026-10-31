//! Parameter temporal dead zone (FunctionDeclarationInstantiation step 21).

use super::*;

impl Interpreter {
    /// Gives every plain parameter name in `atoms` an uninitialized binding
    /// in `func_env`, so a default that names a later parameter hits its TDZ
    /// instead of resolving to an outer binding of the same name.
    pub(super) fn declare_named_parameters_ahead(func_env: &EnvRef, atoms: &[Atom]) {
        let mut env = func_env.borrow_mut();
        for atom in atoms {
            if !env.bindings.contains_key(atom) {
                env.bindings.push_unique(
                    atom.clone(),
                    Binding::new(JsValue::UNDEFINED, BindingKind::Var, false),
                );
            }
        }
    }

    /// The same for an arbitrary non-simple parameter list: every name bound
    /// by `params` that `func_env` does not hold yet.
    pub(super) fn declare_parameter_names_ahead(func_env: &EnvRef, params: &[Pattern]) {
        let mut names = Vec::new();
        for param in params {
            param.bound_names(&mut names);
        }
        let mut env = func_env.borrow_mut();
        for name in &names {
            if !env.bindings.contains_key(name.as_str()) {
                env.bindings.insert(
                    name.as_str(),
                    Binding::new(JsValue::UNDEFINED, BindingKind::Var, false),
                );
            }
        }
    }
}
