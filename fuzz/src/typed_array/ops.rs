//! Operation model for the `typed_array` fuzz target, and its byte codec.
//!
//! A fuzz input is a stream of bytes read through a cursor that yields `0`
//! past the end, so decoding never fails. Every field is reduced into a small
//! table (sizes, indices, JS literals, method names), which keeps the
//! generated programs inside the shapes that reach the buffer/view code:
//! tiny buffers, in-range and out-of-range indices, and arguments whose
//! coercion resizes, detaches or transfers a buffer mid-call.

pub const MAX_OPS: usize = 64;
pub const SLOTS: u8 = 4;

pub const SIZES: [u32; 7] = [0, 1, 3, 8, 16, 24, 64];
pub const OFFSETS: [u32; 5] = [0, 1, 2, 4, 8];
pub const LENGTHS: [u32; 4] = [0, 1, 2, 4];
pub const INDICES: [i32; 8] = [0, 1, 2, 3, 7, 8, -1, 100];
pub const REPS: [u32; 4] = [1, 2, 8, 40];
pub const VALUES: [&str; 16] = [
    "0",
    "1",
    "2",
    "3",
    "-1",
    "7",
    "8",
    "16",
    "24",
    "64",
    "NaN",
    "Infinity",
    "-Infinity",
    "1.5",
    "undefined",
    "'3'",
];

/// Method templates. `{v}` is the receiver view, `{w}` the next view slot,
/// `{aN}` argument N verbatim, `{bN}` argument N converted to a BigInt when
/// the receiver is a BigInt typed array, `{cN}`/`{mN}` argument N as a
/// comparator/mapper callback (a hostile argument mutates a buffer when it
/// is called).
pub const METHODS: &[(&str, &str)] = &[
    ("fill", "{v}.fill({b0},{a1},{a2})"),
    ("set_array", "{v}.set([{b0},{b1}],{a2})"),
    ("set_view", "{v}.set({w},{a0})"),
    ("copyWithin", "{v}.copyWithin({a0},{a1},{a2})"),
    ("slice", "{v}.slice({a0},{a1})"),
    ("subarray", "{v}.subarray({a0},{a1})"),
    ("sort", "{v}.sort({c0})"),
    ("toSorted", "{v}.toSorted({c0})"),
    ("reverse", "{v}.reverse()"),
    ("toReversed", "{v}.toReversed()"),
    ("indexOf", "{v}.indexOf({b0},{a1})"),
    ("includes", "{v}.includes({b0},{a1})"),
    ("join", "{v}.join({a0})"),
    ("at", "{v}.at({a0})"),
    ("with", "{v}.with({a0},{b1})"),
    ("values", "Array.from({v}.values(),{m0})"),
    ("keys", "Array.from({v}.keys(),{m0})"),
    ("entries", "Array.from({v}.entries(),{m0})"),
    ("map", "{v}.map({m0})"),
    ("forEach", "{v}.forEach({m0})"),
    ("every", "{v}.every({m0})"),
    ("filter", "{v}.filter({m0})"),
    ("find", "{v}.findLast({m0})"),
    ("state", "[{v}.length,{v}.byteLength,{v}.byteOffset]"),
    ("dv_getInt16", "{v}.getInt16({a0},{a1})"),
    ("dv_getFloat64", "{v}.getFloat64({a0},{a1})"),
    ("dv_getBigUint64", "{v}.getBigUint64({a0},{a1})"),
    ("dv_setUint8", "{v}.setUint8({a0},{a1})"),
    ("dv_setInt32", "{v}.setInt32({a0},{a1},{a2})"),
    ("dv_setBigInt64", "{v}.setBigInt64({a0},BigInt({a1}))"),
];

pub const ATOMICS: &[(&str, &str)] = &[
    ("add", "Atomics.add({v},{i},{b0})"),
    ("sub", "Atomics.sub({v},{i},{b0})"),
    ("and", "Atomics.and({v},{i},{b0})"),
    ("or", "Atomics.or({v},{i},{b0})"),
    ("xor", "Atomics.xor({v},{i},{b0})"),
    ("exchange", "Atomics.exchange({v},{i},{b0})"),
    (
        "compareExchange",
        "Atomics.compareExchange({v},{i},{b0},{b1})",
    ),
    ("load", "Atomics.load({v},{i})"),
    ("store", "Atomics.store({v},{i},{b0})"),
    ("isLockFree", "Atomics.isLockFree({a0})"),
    ("notify", "Atomics.notify({v},{i},{a0})"),
    ("wait", "Atomics.wait({v},{i},{b0},0)"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Int8,
    Uint8,
    Uint8Clamped,
    Int16,
    Uint16,
    Int32,
    Uint32,
    Float16,
    Float32,
    Float64,
    BigInt64,
    BigUint64,
    DataView,
}

impl Kind {
    pub const ALL: [Kind; 13] = [
        Kind::Int8,
        Kind::Uint8,
        Kind::Uint8Clamped,
        Kind::Int16,
        Kind::Uint16,
        Kind::Int32,
        Kind::Uint32,
        Kind::Float16,
        Kind::Float32,
        Kind::Float64,
        Kind::BigInt64,
        Kind::BigUint64,
        Kind::DataView,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Int8 => "Int8Array",
            Kind::Uint8 => "Uint8Array",
            Kind::Uint8Clamped => "Uint8ClampedArray",
            Kind::Int16 => "Int16Array",
            Kind::Uint16 => "Uint16Array",
            Kind::Int32 => "Int32Array",
            Kind::Uint32 => "Uint32Array",
            Kind::Float16 => "Float16Array",
            Kind::Float32 => "Float32Array",
            Kind::Float64 => "Float64Array",
            Kind::BigInt64 => "BigInt64Array",
            Kind::BigUint64 => "BigUint64Array",
            Kind::DataView => "DataView",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Resize,
    Detach,
    Transfer,
}

/// What a hostile argument does to buffer slot `buf` when it is coerced or
/// called. `len` is the new length for `Resize` and `Transfer`, `0` for `Detach`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hostile {
    pub action: Action,
    pub buf: u8,
    pub len: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arg {
    pub value: &'static str,
    pub hostile: Option<Hostile>,
}

impl Arg {
    pub fn plain(value: &'static str) -> Arg {
        Arg {
            value,
            hostile: None,
        }
    }

    pub fn hostile(value: &'static str, action: Action, buf: u8, len: u32) -> Arg {
        let len = if action == Action::Detach { 0 } else { len };
        Arg {
            value,
            hostile: Some(Hostile { action, buf, len }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferMode {
    Same,
    Fixed(u32),
    ToFixed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    NewBuffer {
        slot: u8,
        len: u32,
        max: Option<u32>,
        shared: bool,
    },
    NewView {
        slot: u8,
        kind: Kind,
        buf: u8,
        offset: u32,
        len: Option<u32>,
    },
    Resize {
        buf: u8,
        len: u32,
    },
    Transfer {
        buf: u8,
        mode: TransferMode,
    },
    Detach {
        buf: u8,
    },
    Access {
        view: u8,
        idx: i32,
        reps: u32,
    },
    Store {
        view: u8,
        idx: i32,
        val: Arg,
        reps: u32,
    },
    Method {
        view: u8,
        name: &'static str,
        args: [Arg; 3],
    },
    Atomic {
        view: u8,
        op: &'static str,
        idx: i32,
        args: [Arg; 2],
    },
}

pub const OP_KINDS: u8 = 9;

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn byte(&mut self) -> u8 {
        let b = self.data.get(self.pos).copied().unwrap_or(0);
        self.pos = self.pos.saturating_add(1);
        b
    }

    fn slot(&mut self) -> u8 {
        self.byte() % SLOTS
    }

    fn pick<T: Copy>(&mut self, table: &[T]) -> T {
        table[self.byte() as usize % table.len()]
    }

    fn optional<T: Copy>(&mut self, table: &[T]) -> Option<T> {
        match self.byte() as usize % (table.len() + 1) {
            0 => None,
            n => Some(table[n - 1]),
        }
    }

    fn name(&mut self, table: &[(&'static str, &'static str)]) -> &'static str {
        table[self.byte() as usize % table.len()].0
    }

    fn arg(&mut self) -> Arg {
        let value = self.pick(&VALUES);
        let mode = self.byte() % 6;
        let packed = self.byte();
        let buf = packed & (SLOTS - 1);
        let len = SIZES[(packed >> 2) as usize % SIZES.len()];
        match mode {
            3 => Arg::hostile(value, Action::Resize, buf, len),
            4 => Arg::hostile(value, Action::Detach, buf, len),
            5 => Arg::hostile(value, Action::Transfer, buf, len),
            _ => Arg::plain(value),
        }
    }
}

/// Decodes `data` into at most [`MAX_OPS`] operations. Never fails.
pub fn decode(data: &[u8]) -> Vec<Op> {
    let mut c = Cursor { data, pos: 0 };
    let mut ops = Vec::new();
    while c.pos < data.len() && ops.len() < MAX_OPS {
        ops.push(decode_op(&mut c));
    }
    ops
}

fn decode_op(c: &mut Cursor) -> Op {
    match c.byte() % OP_KINDS {
        0 => Op::NewBuffer {
            slot: c.slot(),
            len: c.pick(&SIZES),
            max: c.optional(&SIZES),
            shared: c.byte() % 2 == 1,
        },
        1 => Op::NewView {
            slot: c.slot(),
            kind: c.pick(&Kind::ALL),
            buf: c.slot(),
            offset: c.pick(&OFFSETS),
            len: c.optional(&LENGTHS),
        },
        2 => Op::Resize {
            buf: c.slot(),
            len: c.pick(&SIZES),
        },
        3 => {
            let buf = c.slot();
            let mode = c.byte() % 3;
            let len = c.pick(&SIZES);
            let mode = match mode {
                0 => TransferMode::Same,
                1 => TransferMode::Fixed(len),
                _ => TransferMode::ToFixed,
            };
            Op::Transfer { buf, mode }
        }
        4 => Op::Detach { buf: c.slot() },
        5 => Op::Access {
            view: c.slot(),
            idx: c.pick(&INDICES),
            reps: c.pick(&REPS),
        },
        6 => Op::Store {
            view: c.slot(),
            idx: c.pick(&INDICES),
            val: c.arg(),
            reps: c.pick(&REPS),
        },
        7 => Op::Method {
            view: c.slot(),
            name: c.name(METHODS),
            args: [c.arg(), c.arg(), c.arg()],
        },
        _ => Op::Atomic {
            view: c.slot(),
            op: c.name(ATOMICS),
            idx: c.pick(&INDICES),
            args: [c.arg(), c.arg()],
        },
    }
}

fn index_of<T: PartialEq>(table: &[T], value: &T) -> u8 {
    table
        .iter()
        .position(|t| t == value)
        .expect("value not in codec table") as u8
}

fn name_index(table: &[(&str, &str)], name: &str) -> u8 {
    table
        .iter()
        .position(|(n, _)| *n == name)
        .expect("name not in codec table") as u8
}

fn optional_index<T: PartialEq>(table: &[T], value: &Option<T>) -> u8 {
    value.as_ref().map_or(0, |v| index_of(table, v) + 1)
}

fn encode_arg(out: &mut Vec<u8>, arg: &Arg) {
    out.push(index_of(&VALUES, &arg.value));
    match arg.hostile {
        None => out.extend([0, 0]),
        Some(h) => {
            let mode = match h.action {
                Action::Resize => 3,
                Action::Detach => 4,
                Action::Transfer => 5,
            };
            out.extend([mode, h.buf | (index_of(&SIZES, &h.len) << 2)]);
        }
    }
}

/// Inverse of [`decode`] for ops built from the codec tables.
pub fn encode(ops: &[Op]) -> Vec<u8> {
    let mut out = Vec::new();
    for op in ops {
        match op {
            Op::NewBuffer {
                slot,
                len,
                max,
                shared,
            } => out.extend([
                0,
                *slot,
                index_of(&SIZES, len),
                optional_index(&SIZES, max),
                u8::from(*shared),
            ]),
            Op::NewView {
                slot,
                kind,
                buf,
                offset,
                len,
            } => out.extend([
                1,
                *slot,
                index_of(&Kind::ALL, kind),
                *buf,
                index_of(&OFFSETS, offset),
                optional_index(&LENGTHS, len),
            ]),
            Op::Resize { buf, len } => out.extend([2, *buf, index_of(&SIZES, len)]),
            Op::Transfer { buf, mode } => {
                let (mode, len) = match mode {
                    TransferMode::Same => (0, 0),
                    TransferMode::Fixed(n) => (1, index_of(&SIZES, n)),
                    TransferMode::ToFixed => (2, 0),
                };
                out.extend([3, *buf, mode, len]);
            }
            Op::Detach { buf } => out.extend([4, *buf]),
            Op::Access { view, idx, reps } => {
                out.extend([5, *view, index_of(&INDICES, idx), index_of(&REPS, reps)]);
            }
            Op::Store {
                view,
                idx,
                val,
                reps,
            } => {
                out.extend([6, *view, index_of(&INDICES, idx)]);
                encode_arg(&mut out, val);
                out.push(index_of(&REPS, reps));
            }
            Op::Method { view, name, args } => {
                out.extend([7, *view, name_index(METHODS, name)]);
                args.iter().for_each(|a| encode_arg(&mut out, a));
            }
            Op::Atomic {
                view,
                op,
                idx,
                args,
            } => {
                out.extend([8, *view, name_index(ATOMICS, op), index_of(&INDICES, idx)]);
                args.iter().for_each(|a| encode_arg(&mut out, a));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_is_empty_program() {
        assert!(decode(&[]).is_empty());
    }

    #[test]
    fn decode_caps_at_max_ops() {
        let ops = decode(&vec![0xA7; 100_000]);
        assert_eq!(ops.len(), MAX_OPS);
    }

    #[test]
    fn truncated_input_decodes_with_zero_padding() {
        assert_eq!(decode(&[7]).len(), 1);
        assert_eq!(decode(&[7]), decode(&[7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
    }

    #[test]
    fn decode_encode_is_a_fixpoint_on_pseudorandom_bytes() {
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        for _ in 0..200 {
            let data: Vec<u8> = (0..600)
                .map(|_| {
                    state = state
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    (state >> 56) as u8
                })
                .collect();
            let ops = decode(&data);
            assert_eq!(decode(&encode(&ops)), ops);
        }
    }

    #[test]
    fn every_op_kind_is_reachable_from_the_opcode_byte() {
        let kinds: std::collections::HashSet<_> = (0..OP_KINDS)
            .map(|b| std::mem::discriminant(&decode(&[b])[0]))
            .collect();
        assert_eq!(kinds.len(), OP_KINDS as usize);
    }
}
