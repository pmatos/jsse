use super::*;

impl Compiler {
    pub(super) fn local_pair(
        &mut self,
        lhs: &Expression,
        rhs: &Expression,
    ) -> Option<((u16, u16), (u16, u16))> {
        Some((self.local_ref(lhs)?, self.local_ref(rhs)?))
    }
}
