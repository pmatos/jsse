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
                let (Some(bindings), Some(init)) =
                    (simple_object_pattern(&declarator.pattern), &declarator.init)
                else {
                    return Err(unsupported);
                };
                self.compile_expr(init)?;
                self.compile_simple_object_pattern(&bindings, is_const)?;
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
            self.emit_init_lexical(name, is_const)?;
        }
        Ok(())
    }

    /// `[..., value]` → `[...]`: binds `bindings` out of the value on top of
    /// the stack, initialising each name with `InitLexical`.
    pub(super) fn compile_simple_object_pattern(
        &mut self,
        bindings: &[SimpleBinding],
        is_const: bool,
    ) -> Result<(), CompileError> {
        let last = bindings.len() - 1;
        for (i, binding) in bindings.iter().enumerate() {
            if i < last {
                self.emit_dup_n(1);
            }
            let key = self.add_name(&binding.key)?;
            self.emit(Op::GetProp);
            self.emit_u16(key);
            self.emit_init_lexical(&binding.name, is_const)?;
        }
        Ok(())
    }
}
