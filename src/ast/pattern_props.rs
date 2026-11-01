//! The properties of an object binding pattern with V8's error position
//! (#1511, #1566). It derefs to the `Vec`, so code that only reads the
//! properties is unchanged.

use super::ObjectPatternProperty;

/// `at` is the offset plus one under `--node` (else 0) of the first
/// property's value target: where V8 reports a null or undefined value. A
/// pattern without a property, or whose first key is computed, uses its `{`.
#[derive(Clone, Debug, Default)]
pub(crate) struct PatternProps {
    props: Vec<ObjectPatternProperty>,
    pub at: u32,
}

impl PatternProps {
    pub(crate) fn at(props: Vec<ObjectPatternProperty>, at: u32) -> Self {
        Self { props, at }
    }
}

impl From<Vec<ObjectPatternProperty>> for PatternProps {
    fn from(props: Vec<ObjectPatternProperty>) -> Self {
        Self { props, at: 0 }
    }
}

impl std::ops::Deref for PatternProps {
    type Target = Vec<ObjectPatternProperty>;

    fn deref(&self) -> &Self::Target {
        &self.props
    }
}

impl std::ops::DerefMut for PatternProps {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.props
    }
}

impl IntoIterator for PatternProps {
    type Item = ObjectPatternProperty;
    type IntoIter = std::vec::IntoIter<ObjectPatternProperty>;

    fn into_iter(self) -> Self::IntoIter {
        self.props.into_iter()
    }
}

impl<'a> IntoIterator for &'a PatternProps {
    type Item = &'a ObjectPatternProperty;
    type IntoIter = std::slice::Iter<'a, ObjectPatternProperty>;

    fn into_iter(self) -> Self::IntoIter {
        self.props.iter()
    }
}
