//! V8's messages for a pattern with a default (`{ a: { c } = nul }`,
//! `[[c] = nul]`), under `--node` only (#1557): the failing value is worded by
//! the text of the default, whether the default or the value itself was null
//! (`Cannot destructure property 'c' of 'nul' as it is null.`, `nul is not
//! iterable`).

use super::super::*;
use super::node_host_expr_text::expr_text;
use crate::ast::{ArrayPatternElement, Expression, ObjectPatternProperty, Pattern, Property};
use std::rc::Rc;

/// An array pattern with no pattern among its elements.
fn flat(elements: &[Option<ArrayPatternElement>]) -> bool {
    elements.iter().all(|element| {
        !matches!(
            element,
            Some(ArrayPatternElement::Pattern(
                Pattern::Array(_) | Pattern::Object(_)
            ))
        )
    })
}

impl Interpreter {
    /// Notes the default of `pattern` when it is `{ ... } = default`, keyed by
    /// the address of the object pattern's properties. True when it did.
    pub(super) fn note_default_pattern(&mut self, pattern: &Pattern) -> bool {
        if let Pattern::Assign(inner, default) = pattern
            && let Pattern::Object(props) = &**inner
        {
            let text: Rc<str> = expr_text(default).into();
            self.node_host
                .default_texts
                .push((props.as_ptr() as usize, text));
            return true;
        }
        false
    }

    /// The text of the default of the object pattern with these properties.
    pub(super) fn default_text_for(&self, props: &[ObjectPatternProperty]) -> Option<Rc<str>> {
        self.default_text_at(props.as_ptr() as usize)
    }

    fn default_text_at(&self, at: usize) -> Option<Rc<str>> {
        self.node_host
            .default_texts
            .iter()
            .rev()
            .find(|entry| entry.0 == at)
            .map(|entry| entry.1.clone())
    }

    /// The text of the default of the object assignment pattern with these
    /// properties.
    pub(super) fn default_text_for_target(&self, props: &[Property]) -> Option<Rc<str>> {
        self.default_text_at(props.as_ptr() as usize)
    }

    /// Notes the default of the nested assignment target `{ ... } = default`
    /// before it is assigned. True when it did.
    pub(crate) fn note_default_target(
        &mut self,
        target: &Expression,
        default: Option<&Expression>,
    ) -> bool {
        let (Expression::Object(props, _), Some(default), true) =
            (target, default, self.node_host.stack.installed)
        else {
            return false;
        };
        let text: Rc<str> = expr_text(default).into();
        self.node_host
            .default_texts
            .push((props.as_ptr() as usize, text));
        true
    }

    /// `done` of assigning the target once its default is no longer noted:
    /// the not-iterable error of `[...] = default` is worded by the default.
    pub(crate) fn finish_default_target(
        &mut self,
        noted: bool,
        target: &Expression,
        default: Option<&Expression>,
        done: Completion,
    ) -> Completion {
        if noted {
            self.node_host.default_texts.pop();
        }
        match (target, default, done) {
            (Expression::Array(..), Some(default), Completion::Throw(error))
                if self.node_host.stack.installed
                    && matches!(
                        default,
                        Expression::Identifier(_) | Expression::Call(..) | Expression::Member(..)
                    )
                    && self.iterable_error_here(&error) =>
            {
                match Self::iterable_message(default) {
                    Some(text) => Completion::Throw(self.create_type_error(&text)),
                    None => Completion::Throw(error),
                }
            }
            (_, _, done) => done,
        }
    }

    /// `done` of binding `pattern` once its default is no longer noted: the
    /// not-iterable error of `[...] = default` is worded by the default.
    pub(super) fn finish_default_pattern(
        &mut self,
        pattern: &Pattern,
        noted: bool,
        nested: bool,
        value: &JsValue,
        done: Completion,
    ) -> Completion {
        if noted {
            self.node_host.default_texts.pop();
        }
        let own = value.is_undefined() || self.iterable_error_was_for(value);
        match (pattern, done) {
            (Pattern::Assign(inner, default), Completion::Throw(error))
                if nested
                    && matches!(&**inner, Pattern::Array(elements) if flat(elements) || own)
                    && matches!(
                        &**default,
                        Expression::Identifier(_) | Expression::Call(..) | Expression::Member(..)
                    )
                    && self.iterable_error_here(&error) =>
            {
                match Self::iterable_message(default) {
                    Some(text) => Completion::Throw(self.create_type_error(&text)),
                    None => Completion::Throw(error),
                }
            }
            (_, done) => done,
        }
    }
}
