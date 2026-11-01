//! V8's wording for parameter patterns that fail to destructure, under
//! `--node` only (#1557). V8 builds a parameter's pattern from its argument
//! with a synthetic initializer, so its messages name that initializer:
//!
//! - an object pattern with a default, `function ({ a } = d) {}`, reads
//!   `Cannot destructure property 'a' of '(intermediate value)(intermediate
//!   value)(intermediate value)' as it is null.` (the initializer is a
//!   conditional, whether or not the default ran);
//! - an array pattern after an object pattern parameter reads like a property
//!   read of the object pattern's initializer: `Cannot destructure property
//!   'Symbol(Symbol.iterator)' of 'undefined' as it is undefined.`

use super::super::*;
use super::node_host_expr_text::expr_text;
use crate::ast::{ObjectPatternProperty, PropertyKey};

/// V8 prints a conditional as three unnamed values.
pub(super) const CONDITIONAL: &str = "(intermediate value)(intermediate value)(intermediate value)";

/// What an open `bind_pattern` binds.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PatternKind {
    Object,
    /// A pattern with a default: the wrapper around the pattern proper.
    Default,
    Other,
}

impl PatternKind {
    pub(crate) fn of(pattern: &Pattern) -> Self {
        match pattern {
            Pattern::Object(_) => PatternKind::Object,
            Pattern::Assign(..) => PatternKind::Default,
            _ => PatternKind::Other,
        }
    }
}

/// What an earlier parameter leaves in V8's printer for the array pattern
/// parameter after it.
#[derive(Clone)]
pub(crate) enum Leak {
    /// The value's own text.
    Plain,
    /// The desugared conditional of a parameter with a default.
    Defaulted,
    /// The default of a nested pattern (`{ a: { c } = {} }` leaves `{}`).
    Text(String),
    /// A call default: `f is not iterable (...)`.
    Callee(String),
}

/// What V8 reads as the position of a later parameter's failure in this object
/// pattern parameter. A computed key or a default on a plain name ends the
/// effect, except that a call default names its callee.
fn leaks_position(props: &[ObjectPatternProperty]) -> Option<Leak> {
    let mut leak = Leak::Plain;
    for prop in props {
        let ObjectPatternProperty::KeyValue(key, target) = prop else {
            continue;
        };
        if matches!(key, PropertyKey::Computed(_)) {
            return None;
        }
        if let Pattern::Assign(inner, default) = target {
            if !matches!(**inner, Pattern::Identifier(_)) {
                leak = Leak::Text(expr_text(default));
            } else if let Expression::Call(callee, ..) = &**default {
                return Some(Leak::Callee(expr_text(callee)));
            } else {
                return None;
            }
        }
    }
    Some(leak)
}

/// The pattern a parameter binds, and whether it has a default.
fn unwrap_default(param: &Pattern) -> (&Pattern, bool) {
    match param {
        Pattern::Assign(inner, _) => (inner, true),
        param => (param, false),
    }
}

impl Interpreter {
    /// Whether the object pattern being bound is the one of a parameter with
    /// a default.
    pub(crate) fn under_param_default(&self) -> bool {
        let node = &self.node_host;
        let depth = self.call_depth;
        node.pattern_frames
            .iter()
            .rev()
            .take_while(|d| **d == depth)
            .count()
            == 2
            && node.pattern_kinds.len() >= 2
            && node.pattern_kinds[node.pattern_kinds.len() - 2] == PatternKind::Default
    }

    /// `bind_pattern_or_throw` for the parameter at `index`; under `--node` it
    /// notes beforehand whether an earlier object pattern parameter changes
    /// the wording of an array pattern one.
    pub(crate) fn bind_param(
        &mut self,
        params: &[Pattern],
        index: usize,
        value: JsValue,
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        let param = &params[index];
        let noted = self.node_host.stack.installed && self.note_param_leak(params, index);
        let done = self.bind_pattern(param, value, BindingKind::Var, env);
        if noted {
            self.node_host.param_leak = None;
        }
        match done {
            Completion::Throw(error) => Err(error),
            _ => Ok(()),
        }
    }

    /// Notes the array pattern parameter at `index` when the nearest earlier
    /// pattern parameter is an object pattern V8 reads its position from.
    fn note_param_leak(&mut self, params: &[Pattern], index: usize) -> bool {
        let (pattern, _) = unwrap_default(&params[index]);
        if !matches!(pattern, Pattern::Array(_)) {
            return false;
        }
        if let Pattern::Assign(_, default) = &params[index] {
            if let Expression::Call(callee, ..) = &**default {
                let at = std::ptr::from_ref(pattern) as usize;
                self.node_host.param_leak = Some((at, Leak::Callee(expr_text(callee))));
                return true;
            }
            if !matches!(
                **default,
                Expression::Literal(_) | Expression::Void(_) | Expression::Object(..)
            ) {
                return false;
            }
        }
        let earlier = params[..index].iter().rev().find_map(|param| {
            let (pattern, defaulted) = unwrap_default(param);
            matches!(pattern, Pattern::Object(_) | Pattern::Array(_))
                .then_some((pattern, defaulted))
        });
        let Some((Pattern::Object(props), defaulted)) = earlier else {
            return false;
        };
        let at = std::ptr::from_ref(pattern) as usize;
        let leak = if defaulted {
            Some(Leak::Defaulted)
        } else {
            leaks_position(props)
        };
        self.node_host.param_leak = leak.map(|leak| (at, leak));
        self.node_host.param_leak.is_some()
    }

    /// The message for the array pattern parameter at `addr` that cannot
    /// iterate `value`, when an earlier object pattern parameter changes it.
    pub(crate) fn param_after_object_message(
        &self,
        addr: usize,
        value: &JsValue,
    ) -> Option<String> {
        let (at, leak) = self.node_host.param_leak.as_ref()?;
        if *at != addr {
            return None;
        }
        let defaulted = matches!(leak, Leak::Defaulted);
        let shown = match leak {
            Leak::Defaulted => CONDITIONAL.to_string(),
            Leak::Text(text) => text.clone(),
            Leak::Callee(callee) => {
                return Some(format!(
                    "{callee} is not iterable (cannot read property Symbol(Symbol.iterator))"
                ));
            }
            Leak::Plain => self.v8_call_site_text(value),
        };
        if value.is_nullish() {
            Some(Self::iterator_read_message(&shown, value))
        } else if defaulted {
            Some(format!(
                "{shown} is not iterable (cannot read property Symbol(Symbol.iterator))"
            ))
        } else {
            None
        }
    }
}
