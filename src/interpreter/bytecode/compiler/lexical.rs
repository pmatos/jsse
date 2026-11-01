//! `let`/`const` declarations, including their object binding patterns
//! (sec-runtime-semantics-bindinginitialization, ObjectBindingPattern).
//!
//! A pattern with at least one property, all of them `name` or `key: name`
//! (identifier, string or number key; no default, rest, computed key or
//! nested pattern), lowers to `init; (DupN 1; GetProp key; InitLexical name)*;
//! GetProp key; InitLexical name`: the last property consumes the value the
//! others duplicate. A `null` or `undefined` value fails at the first read
//! with a TypeError, as RequireObjectCoercible would, before anything
//! observable has happened, so the guard needs no op of its own; the empty
//! pattern `{}` has no read to fail and stays on the tree-walker. ResolveBinding of a
//! `let`/`const` name has no observable effect, so the spec's resolve-then-GetV
//! order needs no op of its own either.

use super::{CompileError, Compiler, is_anonymous_class};
use crate::ast::{ObjectPatternProperty, Pattern, PropertyKey, VarKind, VariableDeclaration};
use crate::interpreter::bytecode::op::Op;
use crate::types::JsValue;

/// One `key: name` entry of a simple object pattern.
pub(super) struct SimpleBinding {
    key: String,
    name: String,
}

/// The `(property key, bound name)` list of `pattern`, or `None` when any
/// entry needs more than a plain read and binding.
pub(super) fn simple_object_pattern(pattern: &Pattern) -> Option<Vec<SimpleBinding>> {
    let Pattern::Object(props) = pattern else {
        return None;
    };
    if props.is_empty() {
        return None;
    }
    props
        .iter()
        .map(|prop| match prop {
            ObjectPatternProperty::Shorthand(name) => Some(SimpleBinding {
                key: name.clone(),
                name: name.clone(),
            }),
            ObjectPatternProperty::KeyValue(key, Pattern::Identifier(name)) => {
                let key = match key {
                    PropertyKey::Identifier(s) => s.clone(),
                    PropertyKey::String(units) => String::from_utf16(units).ok()?,
                    PropertyKey::Number(n) => {
                        crate::interpreter::to_js_string(&JsValue::number(*n))
                    }
                    PropertyKey::Computed(_) | PropertyKey::Private(_) => return None,
                };
                Some(SimpleBinding {
                    key,
                    name: name.clone(),
                })
            }
            _ => None,
        })
        .collect()
}

impl Compiler {
    pub(super) fn compile_lexical_declaration(
        &mut self,
        decl: &VariableDeclaration,
    ) -> Result<(), CompileError> {
        let is_const = decl.kind == VarKind::Const;
        for declarator in &decl.declarations {
            let Pattern::Identifier(name) = &declarator.pattern else {
                let unsupported = CompileError::Unsupported("lexical binding pattern");
                let (Some(bindings), Some(init), Pattern::Object(props)) = (
                    simple_object_pattern(&declarator.pattern),
                    &declarator.init,
                    &declarator.pattern,
                ) else {
                    return Err(unsupported);
                };
                self.compile_expr(init)?;
                self.compile_simple_object_pattern(&bindings, is_const, init, props.at)?;
                continue;
            };
            if declarator.init.as_ref().is_some_and(is_anonymous_class) {
                return Err(CompileError::Unsupported("anonymous class initializer"));
            }
            match &declarator.init {
                Some(init) => self.compile_named_value(init, name)?,
                None => {
                    self.emit(Op::LoadUndefined);
                    self.push_n(1);
                }
            }
            self.emit_stmt_pos(declarator.init_at);
            self.emit_init_lexical(name, is_const)?;
        }
        Ok(())
    }

    /// `owns_lexicals` lets a `let`/`const` initialise the binding its
    /// enclosing environment already holds in its TDZ: declaration
    /// instantiation hoisted it into the function environment, or `PushEnv`
    /// into the block's. Anywhere else (a Script's top level, whose lexicals
    /// are global declarative bindings) the tree-walker runs the whole
    /// statement.
    pub(super) fn compile_var_declaration(
        &mut self,
        decl: &VariableDeclaration,
        owns_lexicals: bool,
    ) -> Result<(), CompileError> {
        if decl.kind != VarKind::Var {
            if !owns_lexicals || !matches!(decl.kind, VarKind::Let | VarKind::Const) {
                return Err(CompileError::Unsupported("lexical declaration"));
            }
            return self.compile_lexical_declaration(decl);
        }
        for declarator in &decl.declarations {
            let Pattern::Identifier(name) = &declarator.pattern else {
                return Err(CompileError::Unsupported("var binding pattern"));
            };
            let idx = self.add_var_name(name)?;
            if let Some(init) = &declarator.init {
                if is_anonymous_class(init) {
                    return Err(CompileError::Unsupported("anonymous class initializer"));
                }
                if let Some(slot) = self.local_slot(name) {
                    self.compile_named_value(init, name)?;
                    self.emit_store_local(idx, slot, true);
                } else {
                    self.emit_resolve_name(idx);
                    self.compile_named_value(init, name)?;
                    self.emit_store_resolved_name(idx, true);
                }
                self.emit_stmt_pos(declarator.init_at);
            }
        }
        Ok(())
    }

    /// `[..., value]` → `[...]`: binds `bindings` out of the value on top of
    /// the stack, initialising each name with `InitLexical`.
    pub(super) fn compile_simple_object_pattern(
        &mut self,
        bindings: &[SimpleBinding],
        is_const: bool,
        init: &crate::ast::Expression,
        at: u32,
    ) -> Result<(), CompileError> {
        let last = bindings.len() - 1;
        for (i, binding) in bindings.iter().enumerate() {
            if i < last {
                self.emit_dup_n(1);
            }
            let key = self.add_name(&binding.key)?;
            if i == 0 {
                self.note_pattern_source(init);
            }
            self.emit_member_at(Op::GetProp, if i == 0 { at } else { 0 });
            self.emit_u16(key);
            self.emit_init_lexical(&binding.name, is_const)?;
        }
        Ok(())
    }
}
