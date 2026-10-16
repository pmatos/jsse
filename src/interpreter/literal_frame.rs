//! Call-frame facts that are pure functions of one function literal (#1062),
//! shared by `Rc` with every closure of the literal so a call reads them off
//! the function it already holds instead of probing a Body-keyed table.

use std::cell::{Cell, OnceCell};

use crate::ast::Pattern;

use super::atom::Atom;
use super::ic_store::BodyStoreHandle;

/// The interned parameter names of a simple parameter list.
#[derive(Debug)]
pub(crate) struct ParamAtoms {
    atoms: Box<[Atom]>,
    /// True when no two names are equal and none is `arguments`, so each can
    /// be appended to an environment that holds at most the `arguments`
    /// binding without first looking the name up.
    fresh: bool,
}

impl ParamAtoms {
    fn new(params: &[Pattern]) -> Self {
        let atoms: Box<[Atom]> = params
            .iter()
            .map(|param| {
                let Pattern::Identifier(name) = param else {
                    unreachable!("simple parameter metadata must match the parameter list");
                };
                Atom::intern(name)
            })
            .collect();
        let fresh = atoms.iter().enumerate().all(|(i, atom)| {
            atom.as_str() != "arguments" && !atoms[..i].iter().any(|prior| prior.ptr_eq(atom))
        });
        Self { atoms, fresh }
    }

    pub(crate) fn atoms(&self) -> &[Atom] {
        &self.atoms
    }

    pub(crate) fn names_are_fresh(&self) -> bool {
        self.fresh
    }
}

#[derive(Debug, Default)]
pub(crate) struct LiteralFrame {
    param_atoms: OnceCell<ParamAtoms>,
    ic_handle: Cell<Option<BodyStoreHandle>>,
}

impl LiteralFrame {
    /// The parameter atoms, interned on first use. `params` must be the
    /// literal's simple parameter list: the frame is only ever paired with the
    /// one list it was created beside.
    #[inline]
    pub(crate) fn param_atoms(&self, params: &[Pattern]) -> &ParamAtoms {
        let atoms = self.param_atoms.get_or_init(|| ParamAtoms::new(params));
        debug_assert!(
            atoms.atoms.len() == params.len()
                && atoms.atoms.iter().zip(params).all(
                    |(atom, param)| matches!(param, Pattern::Identifier(name) if name == atom.as_str())
                ),
            "a LiteralFrame was used with a different parameter list"
        );
        atoms
    }

    pub(crate) fn ic_handle(&self) -> &Cell<Option<BodyStoreHandle>> {
        &self.ic_handle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident(name: &str) -> Pattern {
        Pattern::Identifier(name.to_string())
    }

    #[test]
    fn distinct_names_are_fresh() {
        let params = [ident("a"), ident("b")];
        let atoms = ParamAtoms::new(&params);
        assert!(atoms.names_are_fresh());
        assert_eq!(atoms.atoms().len(), 2);
    }

    #[test]
    fn duplicate_or_arguments_names_are_not_fresh() {
        assert!(!ParamAtoms::new(&[ident("a"), ident("a")]).names_are_fresh());
        assert!(!ParamAtoms::new(&[ident("arguments")]).names_are_fresh());
    }

    #[test]
    fn atoms_are_interned_once_per_frame() {
        let params = [ident("x")];
        let frame = LiteralFrame::default();
        let first = frame.param_atoms(&params) as *const ParamAtoms;
        let second = frame.param_atoms(&params) as *const ParamAtoms;
        assert_eq!(first, second);
        assert!(frame.param_atoms(&params).atoms()[0].ptr_eq(&Atom::intern("x")));
    }
}
