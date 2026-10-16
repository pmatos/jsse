/// Bit 2 of the `UpdateName`/`UpdateLocal` mode byte: effect context, the
/// updated value is not pushed.
pub(crate) const UPDATE_DISCARD: u8 = 0b100;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    LoadConst = 0,
    LoadUndefined = 1,
    Return = 2,
    ReturnUndefined = 3,
    Add = 4,
    Sub = 5,
    Mul = 6,
    Div = 7,
    Mod = 8,
    Pow = 9,
    Eq = 10,
    NotEq = 11,
    StrictEq = 12,
    StrictNotEq = 13,
    Lt = 14,
    Gt = 15,
    LtEq = 16,
    GtEq = 17,
    BitAnd = 18,
    BitOr = 19,
    BitXor = 20,
    Shl = 21,
    Shr = 22,
    UShr = 23,
    Neg = 24,
    Plus = 25,
    Not = 26,
    BitNot = 27,
    Pop = 28,
    LoadTrue = 29,
    LoadFalse = 30,
    LoadNull = 31,
    Jump = 32,
    JumpIfFalse = 33,
    JumpIfTrue = 34,
    JumpIfTruthyKeep = 35,
    JumpIfFalsyKeep = 36,
    JumpIfNotNullishKeep = 37,
    LoadName = 38,
    ResolveName = 39,
    LoadResolvedName = 40,
    StoreResolvedName = 41,
    /// `++`/`--` on a name. Operands: name (u16), mode (u8): bits 0-1 select
    /// post-inc, pre-inc, post-dec, pre-dec; bit 2 means the result is
    /// discarded (not pushed).
    UpdateName = 42,
    GetProp = 43,
    SetProp = 44,
    GetElement = 45,
    SetElement = 46,
    LoadCalleeName = 47,
    Call = 48,
    ReturnCall = 49,
    SetCompletion = 50,
    ReturnCompletion = 51,
    LoadThis = 52,
    DupN = 53,
    ToPrimitiveKey = 54,
    Construct = 55,
    /// Marks a statement-list position so the VM can reach a GC safepoint
    /// there, matching the tree-walker's per-statement safepoint (issue
    /// #808). No operand bytes.
    Safepoint = 56,
    /// `[..., base]` → `[..., method, base]` for `base.name(...)`; the u16
    /// operand is the property name. `base` stays as the call's `this`.
    LoadMethod = 57,
    /// `[..., base, key]` → `[..., method, base]` for `base[key](...)`.
    LoadMethodElement = 58,
    /// `[...]` → `[..., value]` for a function-local name. Operands: name
    /// (u16), slot hint (u16). The slot is only a hint into the frame's
    /// binding vector, verified by atom identity at run time; a miss falls
    /// back to the by-name `LoadName` path (#898).
    LoadLocal = 59,
    /// `[..., value]` → `[..., value]`, storing into a function-local name
    /// (same operands and fallback as `LoadLocal`).
    StoreLocal = 60,
    /// `++`/`--` on a function-local name. Operands: name (u16), slot hint
    /// (u16), mode (u8, as `UpdateName`).
    UpdateLocal = 61,
    /// `[..., value]` → abrupt `Throw(value)`.
    Throw = 62,
    /// `[..., value]` → `[..., typeof value]`.
    Typeof = 63,
    /// `[...]` → `[..., typeof name]`; operand: name (u16). An unresolvable
    /// name yields `"undefined"` rather than a ReferenceError.
    TypeofName = 64,
    /// `[..., value, ctor]` → `[..., value instanceof ctor]`.
    Instanceof = 65,
    /// `[..., key, object]` → `[..., key in object]`.
    In = 66,
    /// `++`/`--` on `base.name`: `[..., base]` → `[..., result]`. Operands:
    /// name (u16), mode (u8, as `UpdateName`).
    UpdateProp = 67,
    /// `++`/`--` on `base[key]`: `[..., base, key]` → `[..., result]`. The op
    /// runs the base-nullish check and ToPropertyKey once. Operand: mode (u8).
    UpdateElement = 68,
    /// `[...]` → `[..., value]`: evaluate the expression node `ast[idx]` on
    /// the tree-walker in the current environment. Operand: ast index (u16).
    EvalAst = 69,
    /// Run the statement node `ast[idx]` on the tree-walker (operand stack is
    /// empty at a statement boundary). Operands: ast index (u16), escape
    /// table (u16, `NO_ESCAPE_TABLE` when no break/continue can leave it).
    ExecAst = 70,
    /// `[..., value]` → `[...]`: initialise a `let`/`const` binding of the
    /// current environment (InitializeBinding, 9.1.1.1.4). Operands: name
    /// (u16), kind (u8: 0 = let, 1 = const), slot hint (u16, the binding's
    /// position in a block environment, or `NO_INIT_SLOT`).
    InitLexical = 71,
    /// `StoreLocal` in effect context: the stored value is popped afterwards
    /// (it stays rooted on the operand stack through the store, which can
    /// reach a setter and a safepoint). Same operands as `StoreLocal`.
    StoreLocalPop = 72,
    /// `StoreResolvedName` in effect context: pops the reference, stores, then
    /// pops the value. `ResolveName` is still emitted before the right-hand
    /// side, so the reference is created first (spec order).
    StoreResolvedNamePop = 73,
    /// `[..., l]` → `[..., l op K]` for a number literal `K`. Operands: binary
    /// op (u8, the `Op` byte of one of the 20 binary ops), constant (u16).
    BinConst = 74,
    /// `[..., l, r]` → `[...]`, jumping when `l cmp r` is false. Operands:
    /// comparison (u8, the `Op` byte of `Eq`..`GtEq`), offset (i16, last so
    /// `patch_jump` applies unchanged).
    CmpJumpIfFalse = 75,
    /// Exactly `LoadLocal` followed by `BinConst`. Operands: name (u16), slot
    /// hint (u16), binary op (u8), constant (u16).
    LocalBinConst = 76,
    /// Open a block environment (child of the current one) and instantiate
    /// its declarations (BlockDeclarationInstantiation): closures for block
    /// function declarations, then `let`/`const`/class bindings in TDZ.
    /// Operand: block scope index (u16).
    PushEnv = 77,
    /// Leave `n` block environments. Operand: count (u8).
    PopEnv = 78,
    /// CreatePerIterationEnvironment (sec-createperiterationenvironment):
    /// replace the innermost block environment with a fresh sibling holding
    /// copies of its bindings. No operands.
    CopyIterEnv = 79,
    /// `[...]` → `[..., array]`: a fresh array from the constant template
    /// `array_templates[idx]` (primitive elements and holes only). Operand:
    /// template index (u16).
    NewArrayConst = 80,
    /// `[..., v0, ..., vn-1]` → `[..., array]`: a fresh array of the `n`
    /// operand values. Operand: n (u16).
    NewArray = 81,
    /// `[..., array, v0, ..., vn-1]` → `[..., array]`: appends `n` values to
    /// an array literal under construction (literals longer than one operand
    /// chunk). Operand: n (u16).
    ArrayAppend = 82,
    /// `[..., v0, ..., vn-1]` → `[..., object]`: a fresh object literal whose
    /// `n` statically keyed properties take the operand values in order.
    /// Operand: object shape index (u16).
    NewObject = 83,
    /// `[...]` → `[..., closure]`: a closure of the function or arrow literal
    /// `functions[idx]` over the current environment. Operands: template index
    /// (u16), flags (u8: bit 0 = object-literal method).
    MakeClosure = 84,
    /// `[...]` → `[..., regexp]`: a fresh RegExp object for the literal
    /// `regexps[idx]`. Operand: index (u16).
    NewRegExp = 85,
    /// `[..., object, v0, ..., vn-1]` → `[..., object]`: defines the next `n`
    /// statically keyed properties of an object literal under construction
    /// (literals longer than one operand chunk). Operand: object shape index
    /// (u16).
    ObjectAppend = 86,
    /// `[..., iterable]` → `[..., iterator]`: GetIterator(iterable, sync)
    /// (sec-getiterator). The iterator stays on the operand stack for the
    /// whole loop, where it is rooted.
    GetIterator = 87,
    /// `[..., value]` → `[..., enumerator]`: the for-in head
    /// (sec-runtime-semantics-forinofheadevaluation, enumerate): a nullish
    /// value yields an already exhausted enumerator.
    ForInHead = 88,
    /// `[..., iterator]` → `[..., iterator, value]`, or when the iterator is
    /// done `[...]` (the iterator dropped) and a jump. Operand: offset (i16).
    /// An error from `next`, `done` or `value` propagates without closing the
    /// iterator (IteratorStepValue marks it done).
    IterStep = 89,
    /// `IterStep` over a for-in enumerator.
    ForInStep = 90,
    /// IteratorClose with a normal completion (sec-iteratorclose): the
    /// iterator is `depth` values below the top (0 or 1: a pending return
    /// value sits above it) and is removed. A throw from `return()` or a
    /// non-object result propagates. Operand: depth (u8).
    IterClose = 91,
    /// Handler entry for an abrupt exit of a for-of body or head binding:
    /// `[..., iterator, thrown]`, IteratorClose with the throw completion
    /// (errors from `return()` are discarded), then rethrows `thrown`.
    IterCloseThrow = 92,
    /// Removes `count` values that sit `depth` values below the top.
    /// Operands: depth (u8), count (u8).
    DropBelow = 93,
    /// Leaves a `finally` block: `[..., value, kind]`. Kind 0 falls through,
    /// 1 throws `value`, and `n >= 2` resumes at the `n - 2`th entry of the
    /// finalizer's exit table (pushing `value` back when it carries one).
    /// Operand: exit table (u16).
    EndFinally = 94,
    /// `[..., value]` → `[...]`: stores a for-in/for-of iteration value
    /// through the head binding `head_bindings[idx]` (a destructuring or
    /// assignment pattern). Operand: index (u16).
    BindHead = 95,
    /// Honors a `__host_exit` latched while the `try` block ran (it must stay
    /// uncatchable and skip `finally`).
    CheckExit = 96,
    /// `base.#name`: `[..., base]` → `[..., value]` (PrivateGet). Operand:
    /// index into `Chunk::private_names` (u16).
    GetPrivate = 97,
    /// `base.#name = value`: `[..., base, value]` → `[..., value]`
    /// (PrivateSet). Operand: private-name index (u16).
    SetPrivate = 98,
    /// `++`/`--` on `base.#name`: `[..., base]` → `[..., result]`. Operands:
    /// private-name index (u16), mode (u8, as `UpdateProp`).
    UpdatePrivate = 99,
    /// `[..., base]` → `[..., method, base]` for `base.#name(...)`. Operand:
    /// private-name index (u16).
    LoadMethodPrivate = 100,
    /// `#name in object`: `[..., object]` → `[..., boolean]`. Operand:
    /// private-name index (u16).
    PrivateIn = 101,
    /// `switch` case test: `[..., discriminant, selector]` → `[..., discriminant]`,
    /// jumping when the selector `===` the discriminant. Operand: i16 offset.
    CaseStrictEq = 102,
}

impl Op {
    pub(crate) fn from_u8(b: u8) -> Option<Op> {
        match b {
            0 => Some(Op::LoadConst),
            1 => Some(Op::LoadUndefined),
            2 => Some(Op::Return),
            3 => Some(Op::ReturnUndefined),
            4 => Some(Op::Add),
            5 => Some(Op::Sub),
            6 => Some(Op::Mul),
            7 => Some(Op::Div),
            8 => Some(Op::Mod),
            9 => Some(Op::Pow),
            10 => Some(Op::Eq),
            11 => Some(Op::NotEq),
            12 => Some(Op::StrictEq),
            13 => Some(Op::StrictNotEq),
            14 => Some(Op::Lt),
            15 => Some(Op::Gt),
            16 => Some(Op::LtEq),
            17 => Some(Op::GtEq),
            18 => Some(Op::BitAnd),
            19 => Some(Op::BitOr),
            20 => Some(Op::BitXor),
            21 => Some(Op::Shl),
            22 => Some(Op::Shr),
            23 => Some(Op::UShr),
            24 => Some(Op::Neg),
            25 => Some(Op::Plus),
            26 => Some(Op::Not),
            27 => Some(Op::BitNot),
            28 => Some(Op::Pop),
            29 => Some(Op::LoadTrue),
            30 => Some(Op::LoadFalse),
            31 => Some(Op::LoadNull),
            32 => Some(Op::Jump),
            33 => Some(Op::JumpIfFalse),
            34 => Some(Op::JumpIfTrue),
            35 => Some(Op::JumpIfTruthyKeep),
            36 => Some(Op::JumpIfFalsyKeep),
            37 => Some(Op::JumpIfNotNullishKeep),
            38 => Some(Op::LoadName),
            39 => Some(Op::ResolveName),
            40 => Some(Op::LoadResolvedName),
            41 => Some(Op::StoreResolvedName),
            42 => Some(Op::UpdateName),
            43 => Some(Op::GetProp),
            44 => Some(Op::SetProp),
            45 => Some(Op::GetElement),
            46 => Some(Op::SetElement),
            47 => Some(Op::LoadCalleeName),
            48 => Some(Op::Call),
            49 => Some(Op::ReturnCall),
            50 => Some(Op::SetCompletion),
            51 => Some(Op::ReturnCompletion),
            52 => Some(Op::LoadThis),
            53 => Some(Op::DupN),
            54 => Some(Op::ToPrimitiveKey),
            55 => Some(Op::Construct),
            56 => Some(Op::Safepoint),
            57 => Some(Op::LoadMethod),
            58 => Some(Op::LoadMethodElement),
            59 => Some(Op::LoadLocal),
            60 => Some(Op::StoreLocal),
            61 => Some(Op::UpdateLocal),
            62 => Some(Op::Throw),
            63 => Some(Op::Typeof),
            64 => Some(Op::TypeofName),
            65 => Some(Op::Instanceof),
            66 => Some(Op::In),
            67 => Some(Op::UpdateProp),
            68 => Some(Op::UpdateElement),
            69 => Some(Op::EvalAst),
            70 => Some(Op::ExecAst),
            71 => Some(Op::InitLexical),
            72 => Some(Op::StoreLocalPop),
            73 => Some(Op::StoreResolvedNamePop),
            74 => Some(Op::BinConst),
            75 => Some(Op::CmpJumpIfFalse),
            76 => Some(Op::LocalBinConst),
            77 => Some(Op::PushEnv),
            78 => Some(Op::PopEnv),
            79 => Some(Op::CopyIterEnv),
            80 => Some(Op::NewArrayConst),
            81 => Some(Op::NewArray),
            82 => Some(Op::ArrayAppend),
            83 => Some(Op::NewObject),
            84 => Some(Op::MakeClosure),
            85 => Some(Op::NewRegExp),
            86 => Some(Op::ObjectAppend),
            87 => Some(Op::GetIterator),
            88 => Some(Op::ForInHead),
            89 => Some(Op::IterStep),
            90 => Some(Op::ForInStep),
            91 => Some(Op::IterClose),
            92 => Some(Op::IterCloseThrow),
            93 => Some(Op::DropBelow),
            94 => Some(Op::EndFinally),
            95 => Some(Op::BindHead),
            96 => Some(Op::CheckExit),
            97 => Some(Op::GetPrivate),
            98 => Some(Op::SetPrivate),
            99 => Some(Op::UpdatePrivate),
            100 => Some(Op::LoadMethodPrivate),
            101 => Some(Op::PrivateIn),
            102 => Some(Op::CaseStrictEq),
            _ => None,
        }
    }
}
