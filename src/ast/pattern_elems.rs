//! The elements of an array binding pattern with V8's error position (#1511).
//! It derefs to the `Vec`, so code that only reads the elements is unchanged.

use super::ArrayPatternElement;

/// `at` is the offset plus one under `--node` (else 0) where V8 reports a
/// value that is not iterable: the pattern's `[`, or for a formal parameter
/// the `(` of the parameter list.
#[derive(Clone, Debug, Default)]
pub(crate) struct PatternElems {
    elems: Vec<Option<ArrayPatternElement>>,
    pub at: u32,
}

impl PatternElems {
    pub(crate) fn at(elems: Vec<Option<ArrayPatternElement>>, at: u32) -> Self {
        Self { elems, at }
    }
}

impl From<Vec<Option<ArrayPatternElement>>> for PatternElems {
    fn from(elems: Vec<Option<ArrayPatternElement>>) -> Self {
        Self { elems, at: 0 }
    }
}

impl std::ops::Deref for PatternElems {
    type Target = Vec<Option<ArrayPatternElement>>;

    fn deref(&self) -> &Self::Target {
        &self.elems
    }
}

impl std::ops::DerefMut for PatternElems {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.elems
    }
}

impl IntoIterator for PatternElems {
    type Item = Option<ArrayPatternElement>;
    type IntoIter = std::vec::IntoIter<Option<ArrayPatternElement>>;

    fn into_iter(self) -> Self::IntoIter {
        self.elems.into_iter()
    }
}

impl<'a> IntoIterator for &'a PatternElems {
    type Item = &'a Option<ArrayPatternElement>;
    type IntoIter = std::slice::Iter<'a, Option<ArrayPatternElement>>;

    fn into_iter(self) -> Self::IntoIter {
        self.elems.iter()
    }
}
