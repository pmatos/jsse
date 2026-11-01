use super::*;

impl Compiler {
    pub(super) fn emit_drop_below(&mut self, has_value: bool, count: u8) {
        self.emit(Op::DropBelow);
        self.code.push(u8::from(has_value));
        self.code.push(count);
    }
}
