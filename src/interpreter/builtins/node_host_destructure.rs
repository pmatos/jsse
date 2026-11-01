//! V8's messages for destructuring a null or undefined value, under `--node`
//! only (#1512): `Cannot destructure property 'a' of 'o.p' as it is
//! undefined.` and `x is not iterable`. V8 prints the initializer from its
//! AST; so does `expr_text`, at the declarator or assignment that holds both
//! the pattern and the initializer.

use super::super::*;
use super::node_host_expr_text::expr_text;
use super::node_host_pattern_origin::PatternOrigin;
use crate::ast::{
    AssignOp, Expression, ObjectPatternProperty, Pattern, Property, PropertyKey, PropertyKind,
    VariableDeclarator,
};

fn key_name(key: &PropertyKey) -> Option<String> {
    match key {
        PropertyKey::Identifier(name) => Some(name.clone()),
        PropertyKey::String(units) => Some(String::from_utf16_lossy(units)),
        PropertyKey::Number(n) => Some(JsValue::number(*n).to_string()),
        _ => None,
    }
}

/// The first property's name and whether it has a default: V8 words a
/// failed destructure by the name only without one (`Cannot read properties
/// of null (reading 'a')` with it).
type Lead = Option<(String, bool)>;

fn first_pattern_name(props: &[ObjectPatternProperty]) -> Lead {
    match props.first()? {
        ObjectPatternProperty::Shorthand(name) => Some((name.clone(), false)),
        ObjectPatternProperty::KeyValue(key, target) => {
            Some((key_name(key)?, matches!(target, Pattern::Assign(..))))
        }
        ObjectPatternProperty::Rest(_) => None,
    }
}

fn first_target_name(props: &[Property]) -> Lead {
    let first = props.first()?;
    if let Expression::Spread(target) = &first.value {
        return (!matches!(**target, Expression::Identifier(_))).then(|| (String::new(), true));
    }
    if first.computed || first.kind != PropertyKind::Init {
        return None;
    }
    let (target, defaulted) = match &first.value {
        Expression::Assign(AssignOp::Assign, target, _) => (&**target, true),
        value => (value, false),
    };
    let read = defaulted || matches!(target, Expression::Member(..));
    Some((key_name(&first.key)?, read))
}

/// V8's text for the initializer of an array pattern that is not iterable:
/// it prints names, literals, `this` and operators, a member read or an
/// assignment reads as the value.
fn array_pattern_text(init: &Expression) -> Option<String> {
    match init {
        Expression::Identifier(_)
        | Expression::Literal(_)
        | Expression::Call(..)
        | Expression::New(..)
        | Expression::TaggedTemplate(..)
        | Expression::Object(..)
        | Expression::This
        | Expression::Unary(..)
        | Expression::Void(_) => Interpreter::iterable_message(init),
        _ => None,
    }
}

impl Interpreter {
    /// The TypeError for destructuring `value` (null or undefined). `source`
    /// is the initializer; without it V8 prints the value itself.
    fn destructure_error(
        &mut self,
        first: Lead,
        is_array: bool,
        source: Option<&Expression>,
        value: &JsValue,
    ) -> JsValue {
        let text = match source {
            _ if is_array => match source.and_then(array_pattern_text) {
                Some(text) => text,
                None => return self.not_iterable_error(value),
            },
            _ => match first {
                Some((name, true)) if name.is_empty() => {
                    let what = if value.is_null() { "null" } else { "undefined" };
                    format!("Cannot read properties of {what}")
                }
                Some((name, true)) => {
                    self.nullish_keyed_read_message(value, &JsPropertyKey::from(name))
                }
                first => {
                    let shown = source.map_or_else(|| self.v8_call_site_text(value), expr_text);
                    Self::destructure_message(first.map(|lead| lead.0), &shown, value)
                }
            },
        };
        self.create_type_error(&text)
    }

    /// `Cannot destructure property 'a' of 'shown' as it is null.`
    pub(super) fn destructure_message(
        first: Option<String>,
        shown: &str,
        value: &JsValue,
    ) -> String {
        let what = if value.is_null() { "null" } else { "undefined" };
        match first {
            Some(name) => {
                format!("Cannot destructure property '{name}' of '{shown}' as it is {what}.")
            }
            None => format!("Cannot destructure '{shown}' as it is {what}."),
        }
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
        let top = value.clone();
        let bound = match (&declarator.pattern, &declarator.init) {
            (
                Pattern::Object(_),
                Some(
                    init @ (Expression::Identifier(_)
                    | Expression::Member(..)
                    | Expression::Call(..)
                    | Expression::Object(..)),
                ),
            ) if self.node_host.stack.installed => {
                let origin = PatternOrigin::Init(expr_text(init).into());
                self.with_pattern_origin(origin, |i| {
                    i.bind_pattern(&declarator.pattern, value, kind, env)
                })
            }
            _ => self.bind_pattern(&declarator.pattern, value, kind, env),
        };
        match (&declarator.pattern, &declarator.init, bound) {
            (Pattern::Array(_), Some(init), Completion::Throw(e))
                if self.node_host.stack.installed
                    && array_pattern_text(init).is_some()
                    && self.iterable_error_was_for(&top) =>
            {
                Completion::Throw(self.reword_iterable(e, init))
            }
            (_, _, bound) => bound,
        }
    }

    /// Whether the not-iterable error made last was for `value` itself, not
    /// for a value nested inside it.
    pub(super) fn iterable_error_was_for(&self, value: &JsValue) -> bool {
        self.node_host
            .iterable_value
            .as_ref()
            .is_some_and(|operand| crate::interpreter::helpers::same_value(operand, value))
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
            let lead = first_pattern_name(props);
            let nested = matches!(
                self.node_host.pattern_frames.as_slice(),
                [.., outer, inner] if outer == inner
            );
            let error = match (lead, self.head_text_here()) {
                (Some((name, _)), _) if nested => {
                    self.nullish_keyed_read_message(value, &JsPropertyKey::from(name))
                }
                (_, _) if nested => "Cannot convert undefined or null to object".to_string(),
                (lead, Some(head)) => {
                    Self::destructure_message(lead.map(|lead| lead.0), head, value)
                }
                (lead, None) => {
                    return Completion::Throw(self.destructure_error(lead, false, None, value));
                }
            };
            return Completion::Throw(self.create_type_error(&error));
        }
        self.require_object_coercible(value)
    }

    /// RequireObjectCoercible for an object assignment pattern that is nested
    /// or a for-of head: V8 reads its first property.
    pub(crate) fn require_assignable(&mut self, props: &[Property], value: &JsValue) -> Completion {
        if self.node_host.stack.installed
            && value.is_nullish()
            && let Some((name, _)) = first_target_name(props)
            && !name.is_empty()
        {
            let text = self.nullish_keyed_read_message(value, &JsPropertyKey::from(name));
            return Completion::Throw(self.create_type_error(&text));
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
        let frame = self.call_depth;
        let site = self.array_pattern_site(pattern, &value);
        self.node_host.pattern_frames.push(frame);
        self.node_host
            .pattern_kinds
            .push(matches!(pattern, Pattern::Object(_)));
        let done = self.bind_pattern_at(pattern, value, kind, env);
        self.node_host.pattern_frames.pop();
        self.node_host.pattern_kinds.pop();
        match site {
            Some(site) => self.reword_array_pattern(done, site),
            None => done,
        }
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
            Expression::Object(props, _) => {
                if !self.node_host.stack.installed
                    || !matches!(
                        source,
                        Expression::Identifier(_)
                            | Expression::Member(..)
                            | Expression::Call(..)
                            | Expression::Object(..)
                    )
                {
                    return self.destructure_object_assignment(props, value, env);
                }
                let arrays: Vec<usize> = props
                    .iter()
                    .filter_map(|prop| match &prop.value {
                        Expression::Array(elements, _) => Some(elements.as_ptr() as usize),
                        _ => None,
                    })
                    .collect();
                if arrays.is_empty() {
                    return self.destructure_object_assignment(props, value, env);
                }
                let origin = PatternOrigin::Target(expr_text(source).into(), arrays.into());
                self.with_pattern_origin(origin, |i| {
                    i.destructure_object_assignment(props, value, env)
                })
            }
            _ => Completion::Normal(JsValue::UNDEFINED),
        }
    }

    /// An array assignment pattern met while assigning through an object
    /// pattern. When it is a direct property of the outermost one, V8 words
    /// its failure by the source of the assignment, like a property read.
    pub(crate) fn nested_array_assign(
        &mut self,
        elements: &[Option<Expression>],
        value: &JsValue,
        env: &EnvRef,
    ) -> Completion {
        let done = self.destructure_array_assignment(elements, value, env);
        let Completion::Throw(error) = &done else {
            return done;
        };
        let Some(PatternOrigin::Target(shown, arrays)) = self.origin_here() else {
            return done;
        };
        let at = elements.as_ptr() as usize;
        if !self.node_host.stack.installed
            || !arrays.contains(&at)
            || !self.iterable_error_was_for(value)
            || !self.iterable_error_here(error)
        {
            return done;
        }
        let text = if value.is_nullish() {
            let what = if value.is_null() { "null" } else { "undefined" };
            format!(
                "Cannot destructure property 'Symbol(Symbol.iterator)' of '{shown}' as it is {what}."
            )
        } else {
            format!("{shown} is not iterable (cannot read property Symbol(Symbol.iterator))")
        };
        Completion::Throw(self.create_type_error(&text))
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

    /// V8's text for a for-of or spread operand that is not iterable, when it
    /// prints one (a call of a function expression or an optional chain
    /// reads as the value instead).
    pub(crate) fn iterable_message(source: &Expression) -> Option<String> {
        match source {
            Expression::Call(callee, ..)
            | Expression::New(callee, ..)
            | Expression::TaggedTemplate(callee, _) => (!matches!(
                **callee,
                Expression::Function(_) | Expression::ArrowFunction(_) | Expression::Class(_)
            ))
            .then(|| {
                format!(
                    "{} is not a function or its return value is not iterable",
                    expr_text(callee)
                )
            }),
            Expression::OptionalChain(..) => None,
            _ => Some(format!("{} is not iterable", expr_text(source))),
        }
    }

    /// `error` from `GetIterator` on the value of `source`, worded with
    /// `source`'s text when it is the builtin not-iterable error.
    pub(crate) fn reword_iterable(&mut self, error: JsValue, source: &Expression) -> JsValue {
        if self.iterable_error_here(&error)
            && let Some(text) = Self::iterable_message(source)
        {
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
            Some(text) if self.iterable_error_here(&error) => self.create_type_error(text),
            _ => error,
        }
    }

    /// Whether `error` is the not-iterable error the `GetIterator` of the
    /// current call frame made, not one raised inside user code it ran.
    pub(super) fn iterable_error_here(&mut self, error: &JsValue) -> bool {
        let here = self.call_depth;
        let made = self.node_host.iterable_error.take();
        made.is_some() && made == error.as_object_id().map(|id| (id, here))
    }
}
