//! V8's messages for destructuring a null or undefined value, under `--node`
//! only (#1512): `Cannot destructure property 'a' of 'o.p' as it is
//! undefined.` and `x is not iterable`. V8 prints the initializer from its
//! AST; so does `expr_text`, at the declarator or assignment that holds both
//! the pattern and the initializer.

use super::super::*;
use super::node_host_expr_text::expr_text;
use crate::ast::{
    Expression, ObjectPatternProperty, Pattern, Property, PropertyKey, PropertyKind,
    VariableDeclarator,
};

fn key_name(key: &PropertyKey) -> Option<String> {
    match key {
        PropertyKey::Identifier(name) => Some(name.clone()),
        PropertyKey::String(units) => Some(String::from_utf16_lossy(units)),
        _ => None,
    }
}

fn first_pattern_name(props: &[ObjectPatternProperty]) -> Option<String> {
    match props.first()? {
        ObjectPatternProperty::Shorthand(name) => Some(name.clone()),
        ObjectPatternProperty::KeyValue(key, _) => key_name(key),
        ObjectPatternProperty::Rest(_) => None,
    }
}

fn first_target_name(props: &[Property]) -> Option<String> {
    let first = props.first()?;
    (!first.computed && first.kind == PropertyKind::Init)
        .then(|| key_name(&first.key))
        .flatten()
}

impl Interpreter {
    /// The TypeError for destructuring `value` (null or undefined). `source`
    /// is the initializer; without it V8 prints the value itself.
    fn destructure_error(
        &mut self,
        first: Option<String>,
        is_array: bool,
        source: Option<&Expression>,
        value: &JsValue,
    ) -> JsValue {
        let text = match source {
            Some(Expression::Call(callee, _, _)) if is_array => {
                format!(
                    "{} is not a function or its return value is not iterable",
                    expr_text(callee)
                )
            }
            Some(source @ (Expression::Identifier(_) | Expression::Literal(_))) if is_array => {
                format!("{} is not iterable", expr_text(source))
            }
            _ if is_array => return self.not_iterable_error(value),
            _ => {
                let shown = source.map_or_else(|| self.v8_call_site_text(value), expr_text);
                let what = if value.is_null() { "null" } else { "undefined" };
                match first {
                    Some(name) => format!(
                        "Cannot destructure property '{name}' of '{shown}' as it is {what}."
                    ),
                    None => format!("Cannot destructure '{shown}' as it is {what}."),
                }
            }
        };
        self.create_type_error(&text)
    }

    /// `bind_pattern` for a declarator: under `--node` a null or undefined
    /// initializer of an object or array pattern is reported with the
    /// initializer's text.
    pub(crate) fn bind_declarator(
        &mut self,
        declarator: &VariableDeclarator,
        value: JsValue,
        kind: BindingKind,
        env: &EnvRef,
    ) -> Completion {
        if self.node_host.stack.installed
            && value.is_nullish()
            && let Some(init) = &declarator.init
        {
            let error = match &declarator.pattern {
                Pattern::Object(props) => Some(self.destructure_error(
                    first_pattern_name(props),
                    false,
                    Some(init),
                    &value,
                )),
                Pattern::Array(_) => Some(self.destructure_error(None, true, Some(init), &value)),
                _ => None,
            };
            if let Some(error) = error {
                return Completion::Throw(error);
            }
        }
        self.bind_pattern(&declarator.pattern, value, kind, env)
    }

    /// The error for `{...} = source` / `[...] = source` when the value is
    /// null or undefined, under `--node`.
    pub(crate) fn node_assign_precheck(
        &mut self,
        target: &Expression,
        source: &Expression,
        value: &JsValue,
    ) -> Option<JsValue> {
        if !self.node_host.stack.installed || !value.is_nullish() {
            return None;
        }
        match target {
            Expression::Object(props, _) => {
                Some(self.destructure_error(first_target_name(props), false, Some(source), value))
            }
            Expression::Array(..) => Some(self.destructure_error(None, true, None, value)),
            _ => None,
        }
    }

    /// RequireObjectCoercible for an object binding pattern whose initializer
    /// is not at hand: a parameter or loop head reads like a declaration, a
    /// nested pattern like a property read.
    pub(crate) fn require_destructurable(
        &mut self,
        props: &[ObjectPatternProperty],
        value: &JsValue,
    ) -> Completion {
        if self.node_host.stack.installed && value.is_nullish() {
            let first = first_pattern_name(props);
            let error = match first {
                Some(name) if self.node_host.pattern_depth > 1 => {
                    self.nullish_keyed_read_message(value, &JsPropertyKey::from(name))
                }
                _ if self.node_host.pattern_depth > 1 => {
                    "Cannot convert undefined or null to object".to_string()
                }
                first => {
                    return Completion::Throw(self.destructure_error(first, false, None, value));
                }
            };
            return Completion::Throw(self.create_type_error(&error));
        }
        self.require_object_coercible(value)
    }

    /// `bind_pattern_at`, counting the open calls under `--node`.
    pub(crate) fn bind_pattern(
        &mut self,
        pattern: &Pattern,
        value: JsValue,
        kind: BindingKind,
        env: &EnvRef,
    ) -> Completion {
        if !self.node_host.stack.installed {
            return self.bind_pattern_at(pattern, value, kind, env);
        }
        self.node_host.pattern_depth += 1;
        let done = self.bind_pattern_at(pattern, value, kind, env);
        self.node_host.pattern_depth -= 1;
        done
    }

    /// `{...} = source` / `[...] = source` once `value` is evaluated.
    pub(crate) fn assign_pattern_value(
        &mut self,
        target: &Expression,
        source: &Expression,
        value: &JsValue,
        env: &EnvRef,
    ) -> Completion {
        if let Some(error) = self.node_assign_precheck(target, source, value) {
            return Completion::Throw(error);
        }
        match target {
            Expression::Array(elements, _) => {
                self.destructure_array_assignment(elements, value, env)
            }
            Expression::Object(props, _) => self.destructure_object_assignment(props, value, env),
            _ => Completion::Normal(JsValue::UNDEFINED),
        }
    }

    /// The bytecode's named read of a null or undefined base: the error of a
    /// `let`/`const` object pattern when the compiler noted its initializer.
    pub(crate) fn nullish_named_error(&mut self, base: &JsValue, name: &str) -> JsValue {
        let text = match self.node_host.pattern_src.take() {
            Some(source) => {
                let what = if base.is_null() { "null" } else { "undefined" };
                format!("Cannot destructure property '{name}' of '{source}' as it is {what}.")
            }
            None => format!("Cannot read properties of {base} (reading '{name}')"),
        };
        self.create_type_error(&text)
    }

    /// V8's text for a for-of or spread operand that is not iterable.
    pub(crate) fn iterable_message(source: &Expression) -> String {
        match source {
            Expression::Call(callee, _, _) => format!(
                "{} is not a function or its return value is not iterable",
                expr_text(callee)
            ),
            _ => format!("{} is not iterable", expr_text(source)),
        }
    }

    /// `error` from `GetIterator` on the value of `source`, worded with
    /// `source`'s text when it is the builtin not-iterable error.
    pub(crate) fn reword_iterable(&mut self, error: JsValue, source: &Expression) -> JsValue {
        if self.node_host.iterable_error.take() == error.as_object_id() && error.is_object() {
            let text = Self::iterable_message(source);
            return self.create_type_error(&text);
        }
        error
    }

    /// The same for the bytecode, which noted the message at compile time.
    pub(crate) fn reword_iterable_noted(
        &mut self,
        error: JsValue,
        chunk: &crate::interpreter::bytecode::chunk::Chunk,
        pc: usize,
    ) -> JsValue {
        match chunk.pattern_src_at(pc) {
            Some(text) if self.node_host.iterable_error.take() == error.as_object_id() => {
                self.create_type_error(text)
            }
            _ => error,
        }
    }
}
