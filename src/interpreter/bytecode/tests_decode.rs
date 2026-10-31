//! Decoding bytecode into `(op, operands)` for the compiler tests.

use super::chunk::{CALL_POS_FLAG, Chunk};
use super::op::Op;

/// Byte widths of an op's operands, in order (1 = u8, 2 = u16/i16, 4 = u32).
fn operand_layout(op: Op) -> &'static [usize] {
    match op {
        Op::LoadConst
        | Op::LoadName
        | Op::ResolveName
        | Op::LoadResolvedName
        | Op::StoreResolvedName
        | Op::StoreResolvedNamePop
        | Op::GetProp
        | Op::SetProp
        | Op::LoadCalleeName
        | Op::LoadMethod
        | Op::Jump
        | Op::JumpIfFalse
        | Op::JumpIfTrue
        | Op::CaseStrictEq
        | Op::JumpIfTruthyKeep
        | Op::JumpIfFalsyKeep
        | Op::JumpIfNotNullishKeep
        | Op::ArgSpreadFirst => &[2],
        Op::UpdateName => &[2, 1],
        Op::LoadLocal | Op::StoreLocal | Op::StoreLocalPop => &[2, 2],
        Op::UpdateLocal => &[2, 2, 1],
        Op::Call | Op::ReturnCall | Op::Construct => &[2, 4],
        Op::SwitchTable => &[2],
        Op::SetCallPos => &[4],
        Op::DupN => &[1],
        Op::BinConst | Op::CmpJumpIfFalse => &[1, 2],
        Op::LocalBinConst => &[2, 2, 1, 2],
        Op::LocalLocalBin => &[2, 2, 2, 2, 1],
        Op::MoveLocal => &[2, 2, 2, 2],
        Op::LocalLocalCmpJump => &[2, 2, 2, 2, 1, 2],
        Op::LocalConstCmpJump => &[2, 2, 1, 2, 2],
        Op::PushEnv | Op::EvalAst => &[2],
        Op::PopEnv => &[1],
        Op::ExecAst => &[2, 2, 1],
        Op::InitLexical => &[2, 1, 2],
        Op::IterStep | Op::ForInStep | Op::EndFinally | Op::BindHead => &[2],
        Op::IterClose => &[1],
        Op::DropBelow => &[1, 1],
        Op::NewArrayConst
        | Op::NewArray
        | Op::ArrayAppend
        | Op::NewObject
        | Op::ObjectAppend
        | Op::NewRegExp => &[2],
        Op::MakeClosure => &[2, 1],
        Op::Yield => &[2, 1, 2],
        _ => &[],
    }
}

/// Decode `(op, operands)`; widen to u16, keeping a u32's low half.
pub(super) fn decode_ops(chunk: &Chunk) -> Vec<(Op, Vec<u16>)> {
    let mut out = Vec::new();
    let mut pc = 0;
    while pc < chunk.code.len() {
        let op = Op::from_u8(chunk.code[pc]).expect("valid opcode");
        pc += 1;
        let mut operands = Vec::new();
        for &width in operand_layout(op) {
            operands.push(match width {
                1 => u16::from(chunk.code[pc]),
                _ => u16::from(chunk.code[pc]) | (u16::from(chunk.code[pc + 1]) << 8),
            });
            pc += width;
        }
        // A call op folding its `--node` position (#1492): flag in the count,
        // position after the site id.
        if matches!(op, Op::Call | Op::ReturnCall | Op::Construct)
            && operands[0] & CALL_POS_FLAG != 0
        {
            operands[0] &= !CALL_POS_FLAG;
            operands.push(u16::from(chunk.code[pc]) | (u16::from(chunk.code[pc + 1]) << 8));
            pc += 4;
        }
        out.push((op, operands));
    }
    out
}
