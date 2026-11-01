//! Letting go of what a finished activation's environment holds that no
//! closure can name (#1420).
//!
//! A closure keeps its whole defining environment, so a callback that outlives
//! the call that made it also keeps every local of that call. The names a
//! closure can reach in the activation's scopes are all mentioned inside the
//! functions and classes nested in the literal
//! (`CaptureWalk::nested_mentions`), so once the activation has returned, a
//! binding holding a heap value and named by none of them is unreachable by
//! any code and is overwritten with `undefined`. The entry stays: slot hints and the frame layout are unchanged,
//! which keeps the tree-walker and the bytecode VM in step.

use std::rc::Rc;

use crate::ast::{Body, CaptureWalk, Pattern};
use crate::types::{JsValue, ValueKind};

use super::atom::Atom;
use super::binding_map::BindingMap;
use super::literal_frame::LiteralFrame;
use super::types::EnvRef;

/// Past this many names a probe is a binary search on the hash.
const SCAN_MAX: usize = 12;

/// The names closures nested in one function literal can mention.
#[derive(Debug)]
pub(crate) struct NestedNames {
    /// One bit per name (`BindingMap`'s scheme): a clear bit is a definite no.
    filter: u64,
    /// Distinct, ordered by precomputed hash.
    names: Box<[Atom]>,
}

impl NestedNames {
    /// `None` when a direct `eval` or `with` makes the names unknowable.
    pub(crate) fn of(params: &[Pattern], body: &Body) -> Option<Self> {
        let mentions = CaptureWalk::nested_mentions(params, body.as_slice());
        if mentions.opaque {
            return None;
        }
        let mut names: Vec<Atom> = mentions.names.into_iter().map(Atom::intern).collect();
        names.sort_unstable_by_key(Atom::precomputed_hash);
        names.dedup_by(|a, b| a.ptr_eq(b));
        Some(Self {
            filter: BindingMap::filter_of(&names),
            names: names.into(),
        })
    }

    #[inline]
    fn mentions(&self, name: &Atom) -> bool {
        let hash = name.precomputed_hash();
        if self.filter & (1u64 << (hash >> 58)) == 0 {
            return false;
        }
        if self.names.len() <= SCAN_MAX {
            return self.names.iter().any(|n| n.ptr_eq(name));
        }
        let first = self.names.partition_point(|n| n.precomputed_hash() < hash);
        self.names[first..]
            .iter()
            .take_while(|n| n.precomputed_hash() == hash)
            .any(|n| n.ptr_eq(name))
    }
}

/// Whether releasing the value could free memory: numbers, booleans,
/// `undefined` and `null` hold none.
fn holds_heap(value: &JsValue) -> bool {
    !matches!(
        value.kind(),
        ValueKind::Undefined | ValueKind::Null | ValueKind::Boolean | ValueKind::Number
    )
}

/// [`trim_exited_environment`] for the parameter environment `func_env` and
/// the body environment `exec_env` of an activation that has just returned
/// (the same environment when the body is not `separate`). A scope no
/// closure holds is left alone: it is about to be recycled.
#[inline]
pub(crate) fn trim_activation_scopes(
    func_env: &EnvRef,
    exec_env: &EnvRef,
    separate: bool,
    frame: &Rc<LiteralFrame>,
    params: &[Pattern],
    body: &Body,
) {
    let captured_body = separate && Rc::strong_count(exec_env) > 1;
    if captured_body {
        trim_exited_environment(exec_env, frame, params, body);
    }
    // `func_env` itself, plus `exec_env` (the same environment cloned, or its
    // child).
    if captured_body || Rc::strong_count(func_env) > 2 {
        trim_exited_environment(func_env, frame, params, body);
    }
}

/// [`trim_activation_scopes`] for the general call path, which also serves a
/// sloppy function with simple parameters: its `arguments` object is mapped to
/// the parameter bindings and may outlive the call, so `unmapped` is false and
/// nothing is trimmed.
#[inline]
pub(crate) fn trim_general_call(
    unmapped: bool,
    (func_env, exec_env, separate): (&EnvRef, &EnvRef, bool),
    frame: &Rc<LiteralFrame>,
    params: &[Pattern],
    body: &Body,
) {
    if unmapped {
        trim_activation_scopes(func_env, exec_env, separate, frame, params, body);
    }
}

/// Overwrites the bindings of `env` that no closure nested in the literal
/// mentions. Call it after the activation that created `env` has returned and
/// only while something besides the call still holds `env` (a closure). Not
/// for an activation whose `arguments` object may be mapped to its parameters.
#[inline(never)]
pub(crate) fn trim_exited_environment(
    env: &EnvRef,
    frame: &Rc<LiteralFrame>,
    params: &[Pattern],
    body: &Body,
) {
    let Some(names) = frame.nested_names(params, body) else {
        return;
    };
    let mut env = env.borrow_mut();
    let mut i = 0;
    while let Some((name, binding)) = env.bindings.entry_at(i) {
        let dead = holds_heap(&binding.value) && !names.mentions(name);
        if dead && let Some((_, binding)) = env.bindings.entry_at_mut(i) {
            binding.value = JsValue::UNDEFINED;
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Statement;

    fn names_of(source: &str) -> Option<NestedNames> {
        let program = crate::parser::Parser::new(source)
            .expect("parser init")
            .parse_program()
            .expect("parse program");
        let Some(Statement::FunctionDeclaration(f)) = program.body.as_slice().first() else {
            panic!("expected a function declaration");
        };
        NestedNames::of(&f.params, &f.body)
    }

    fn mentions(names: &NestedNames, name: &str) -> bool {
        names.mentions(&Atom::intern(name))
    }

    #[test]
    fn names_are_those_of_nested_functions_classes_and_defaults() {
        let names = names_of(
            "function f(a, b = () => dflt) {
                var own = 1;
                function g() { return viaDecl; }
                const h = (x) => viaArrow + x;
                class C extends Base { [key]() {} m() { return viaMethod; } static s = viaField; }
                return own + a;
            }",
        )
        .expect("no eval or with");
        for name in [
            "dflt",
            "viaDecl",
            "viaArrow",
            "Base",
            "key",
            "viaMethod",
            "viaField",
        ] {
            assert!(
                mentions(&names, name),
                "{name} is named by a nested function"
            );
        }
        for name in ["own", "a", "b", "f", "h"] {
            assert!(
                !mentions(&names, name),
                "{name} is named only by the activation"
            );
        }
    }

    #[test]
    fn names_reach_every_nesting_depth() {
        let names = names_of("function f() { return () => () => () => deep; }").expect("names");
        assert!(mentions(&names, "deep"));
    }

    #[test]
    fn many_names_are_found_by_hash_search() {
        let uses: String = (0..40)
            .map(|i| format!("n{i}"))
            .collect::<Vec<_>>()
            .join("+");
        let names = names_of(&format!("function f() {{ return () => {uses}; }}")).expect("names");
        for i in 0..40 {
            assert!(mentions(&names, &format!("n{i}")));
        }
        assert!(!mentions(&names, "n40"));
    }

    #[test]
    fn eval_or_with_anywhere_makes_the_names_unknowable() {
        assert!(names_of("function f() { return () => eval('x'); }").is_none());
        assert!(names_of("function f() { eval('x'); }").is_none());
        assert!(names_of("function f(o) { with (o) { return () => p; } }").is_none());
        assert!(names_of("function f() { return () => ({ eval: 1 }).eval; }").is_some());
    }
}
