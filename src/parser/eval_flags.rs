use super::*;

impl Parser<'_> {
    pub(crate) fn set_eval_allow_super_property(&mut self) {
        self.allow_super_property = true;
    }
}
