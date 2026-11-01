//! Execution of a variable declaration list (§14.3.2.1).

use super::*;

impl Interpreter {
    pub(super) fn exec_variable_declaration(
        &mut self,
        decl: &VariableDeclaration,
        env: &EnvRef,
    ) -> Completion {
        let is_using = matches!(decl.kind, VarKind::Using | VarKind::AwaitUsing);
        let kind = match decl.kind {
            VarKind::Var => BindingKind::Var,
            VarKind::Let => BindingKind::Let,
            VarKind::Const | VarKind::Using | VarKind::AwaitUsing => BindingKind::Const,
        };
        for d in &decl.declarations {
            if d.init.is_none()
                && decl.kind == VarKind::Var
                && matches!(d.pattern, Pattern::Identifier(_))
            {
                continue;
            }
            // §13.3.2.4 step 2: ResolveBinding BEFORE evaluating Initializer
            // Pre-resolve with-scope binding so the reference is captured before
            // side effects in the initializer can change it.
            let pre_resolved_with = if (self.with_scope_depth > 0 || self.has_ever_entered_with)
                && decl.kind == VarKind::Var
                && d.init.is_some()
                && let Pattern::Identifier(ref name) = d.pattern
            {
                match self.resolve_with_has_binding(name, env) {
                    Ok(obj_id) => obj_id,
                    Err(e) => return Completion::Throw(e),
                }
            } else {
                None
            };
            let val = if let Some(init) = &d.init {
                // For anonymous class expressions, pass the binding name into
                // eval_class so SetFunctionName happens before static fields.
                if let Pattern::Identifier(ref name) = d.pattern
                    && let Expression::Class(ce) = init
                    && ce.name.is_none()
                {
                    match self.eval_class(
                        name,
                        "",
                        &ce.super_class,
                        &ce.body,
                        env,
                        ce.source_text.clone(),
                    ) {
                        Completion::Normal(v) => v,
                        other => return other,
                    }
                } else {
                    match self.eval_expr(init, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    }
                }
            } else {
                JsValue::UNDEFINED
            };
            self.node_note_statement_pos(d.init_at);
            if let Pattern::Identifier(ref name) = d.pattern
                && d.init
                    .as_ref()
                    .is_some_and(|e| e.is_anonymous_function_definition())
            {
                // Skip for class expressions — name already set in eval_class
                if !d
                    .init
                    .as_ref()
                    .is_some_and(|e| matches!(e, Expression::Class(ce) if ce.name.is_none()))
                {
                    self.set_function_name(&val, name);
                }
            }
            if is_using {
                let hint = DisposeHint::for_var_kind(decl.kind);
                if let Err(e) = self.add_disposable_resource(env, &val, hint) {
                    return Completion::Throw(e);
                }
            }
            // Use pre-resolved with-scope binding if available
            if let Some(with_obj_id) = pre_resolved_with {
                if let Pattern::Identifier(ref name) = d.pattern {
                    let strict = env.borrow().strict;
                    if let Err(e) = self.with_set_mutable_binding(with_obj_id, name, val, strict) {
                        return Completion::Throw(e);
                    }
                } else {
                    propagate!(self.bind_declarator(d, val, kind, env));
                }
            } else {
                propagate!(self.bind_declarator(d, val, kind, env));
            }
        }
        Completion::Normal(JsValue::UNDEFINED)
    }
}
