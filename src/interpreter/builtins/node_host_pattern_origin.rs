//! V8's text for an array pattern that fails to iterate its value, under
//! `--node` only (#1545). V8 prints what its CallPrinter has at that point:
//! `.for` for the head of a for-of loop (`.for is not iterable`), and the
//! initializer of the declaration for an array pattern nested in an object
//! pattern, which V8 reads like a property (`{(intermediate value)} is not
//! iterable (cannot read property Symbol(Symbol.iterator))`, `Cannot
//! destructure property 'Symbol(Symbol.iterator)' of 'o' as it is null.`).
//! Only the pattern right under the outermost object pattern sees the text;
//! deeper patterns print their value.

use super::super::*;
use crate::interpreter::bytecode::chunk::HeadBinding;
use std::rc::Rc;

/// What a pattern binding's initializer reads as in V8's messages.
#[derive(Clone)]
pub(crate) enum PatternOrigin {
    ForHead,
    Init(Rc<str>),
}

impl PatternOrigin {
    pub(crate) fn text(&self) -> &str {
        match self {
            PatternOrigin::ForHead => ".for",
            PatternOrigin::Init(text) => text,
        }
    }
}

/// Where an array pattern sits, taken before it is bound.
pub(crate) struct ArrayPatternSite {
    top_of_for_head: bool,
    shown: Option<Rc<str>>,
    in_object: bool,
    value: JsValue,
}

impl Interpreter {
    /// The origin set for the pattern binding that is open at this call depth.
    fn origin_here(&self) -> Option<PatternOrigin> {
        let (depth, origin) = self.node_host.pattern_origin.as_ref()?;
        (*depth == self.call_depth).then(|| origin.clone())
    }

    /// Whether the object pattern being bound is the head of a for-of.
    pub(crate) fn is_for_head_here(&self) -> bool {
        matches!(self.origin_here(), Some(PatternOrigin::ForHead))
    }

    /// Run `bind` with `origin` as the text of the initializer.
    pub(crate) fn with_pattern_origin(
        &mut self,
        origin: PatternOrigin,
        bind: impl FnOnce(&mut Self) -> Completion,
    ) -> Completion {
        let entry = (self.call_depth, origin);
        let saved = self.node_host.pattern_origin.replace(entry);
        let done = bind(self);
        self.node_host.pattern_origin = saved;
        done
    }

    /// `bind_pattern` for the declaration head of a for-of loop.
    pub(crate) fn bind_head(
        &mut self,
        pattern: &Pattern,
        value: JsValue,
        kind: BindingKind,
        env: &EnvRef,
    ) -> Completion {
        if !self.node_host.stack.installed || matches!(pattern, Pattern::Identifier(_)) {
            return self.bind_pattern(pattern, value, kind, env);
        }
        self.with_pattern_origin(PatternOrigin::ForHead, |i| {
            i.bind_pattern(pattern, value, kind, env)
        })
    }

    /// Store one iteration value (or a `catch` argument) through a head
    /// binding the bytecode compiler left to the tree-walker.
    pub(crate) fn bind_head_binding(
        &mut self,
        head: &HeadBinding,
        value: JsValue,
        env: &EnvRef,
    ) -> Completion {
        match head {
            HeadBinding::Declare { pattern, kind } => self.bind_head(pattern, value, *kind, env),
            HeadBinding::Catch { pattern } => {
                self.bind_pattern(pattern, value, BindingKind::Let, env)
            }
            HeadBinding::Assign { pattern } => self.assign_to_for_pattern(pattern, value, env),
        }
    }

    /// Where `pattern` sits among the patterns open at this call depth, when
    /// it is an array pattern whose failure V8 words with an origin.
    pub(crate) fn array_pattern_site(
        &self,
        pattern: &Pattern,
        value: &JsValue,
    ) -> Option<ArrayPatternSite> {
        if !matches!(pattern, Pattern::Array(_)) {
            return None;
        }
        let depth = self.call_depth;
        let open = self
            .node_host
            .pattern_frames
            .iter()
            .rev()
            .take_while(|d| **d == depth)
            .count();
        let origin = self.origin_here();
        let in_object = open == 1 && self.node_host.pattern_kinds.last() == Some(&true);
        let top_of_for_head = open == 0 && matches!(origin, Some(PatternOrigin::ForHead));
        if !in_object && !top_of_for_head {
            return None;
        }
        let shown = match origin {
            Some(origin) if in_object => Some(Rc::from(origin.text())),
            _ => None,
        };
        Some(ArrayPatternSite {
            top_of_for_head,
            shown,
            in_object,
            value: value.clone(),
        })
    }

    /// `done` after binding the array pattern at `site`: a not-iterable
    /// error of its own value is reworded as V8 does.
    pub(crate) fn reword_array_pattern(
        &mut self,
        done: Completion,
        site: ArrayPatternSite,
    ) -> Completion {
        let Completion::Throw(error) = done else {
            return done;
        };
        let made = self.node_host.iterable_error;
        let ours = made.is_some() && made == error.as_object_id().map(|id| (id, self.call_depth));
        if !ours || !self.iterable_error_was_for(&site.value) {
            return Completion::Throw(error);
        }
        let value = &site.value;
        let text = if site.top_of_for_head {
            format!("{} is not iterable", PatternOrigin::ForHead.text())
        } else if !site.in_object {
            return Completion::Throw(error);
        } else {
            let shown = site
                .shown
                .as_deref()
                .map_or_else(|| self.v8_call_site_text(value), str::to_string);
            if value.is_nullish() {
                let what = if value.is_null() { "null" } else { "undefined" };
                format!(
                    "Cannot destructure property 'Symbol(Symbol.iterator)' of '{shown}' as it is {what}."
                )
            } else if site.shown.is_some() {
                format!("{shown} is not iterable (cannot read property Symbol(Symbol.iterator))")
            } else {
                return Completion::Throw(error);
            }
        };
        self.node_host.iterable_error = None;
        Completion::Throw(self.create_type_error(&text))
    }
}
