#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    LoadConst = 0,
    LoadUndefined = 1,
    Return = 2,
    ReturnUndefined = 3,
    Add = 4,
}

impl Op {
    pub(crate) fn from_u8(b: u8) -> Option<Op> {
        match b {
            0 => Some(Op::LoadConst),
            1 => Some(Op::LoadUndefined),
            2 => Some(Op::Return),
            3 => Some(Op::ReturnUndefined),
            4 => Some(Op::Add),
            _ => None,
        }
    }
}
