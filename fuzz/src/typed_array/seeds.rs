//! Seed corpus for the `typed_array` target, defined as named op sequences
//! and committed as `fuzz/corpus/typed_array/NN-<name>.bin`. Regenerate with
//! `cargo test --manifest-path fuzz/Cargo.toml --lib -- --ignored regenerate_seed_corpus`.

use super::ops::{ATOMICS, Action, Arg, Kind, METHODS, Op, TransferMode, decode, encode};
use jsse::SigOutcome;
use std::collections::HashSet;
use std::path::PathBuf;

const BUDGET: u64 = 20_000;

fn buf(slot: u8, len: u32, max: Option<u32>, shared: bool) -> Op {
    Op::NewBuffer {
        slot,
        len,
        max,
        shared,
    }
}

fn view(slot: u8, kind: Kind, buf: u8, offset: u32, len: Option<u32>) -> Op {
    Op::NewView {
        slot,
        kind,
        buf,
        offset,
        len,
    }
}

fn access(view: u8, idx: i32, reps: u32) -> Op {
    Op::Access { view, idx, reps }
}

fn method(view: u8, name: &'static str, args: [Arg; 3]) -> Op {
    Op::Method { view, name, args }
}

fn atomic(view: u8, op: &'static str, idx: i32, a: Arg, b: Arg) -> Op {
    Op::Atomic {
        view,
        op,
        idx,
        args: [a, b],
    }
}

fn p(v: &'static str) -> Arg {
    Arg::plain(v)
}

fn plain3() -> [Arg; 3] {
    [p("1"), p("0"), p("3")]
}

fn every_method() -> Vec<Op> {
    let mut ops = vec![
        buf(0, 24, Some(64), false),
        view(0, Kind::Int16, 0, 0, None),
        view(1, Kind::DataView, 0, 0, None),
        view(2, Kind::BigInt64, 0, 8, Some(2)),
    ];
    for (name, _) in METHODS {
        let view = if name.starts_with("dv_") { 1 } else { 0 };
        ops.push(method(view, name, plain3()));
    }
    ops.push(method(2, "fill", plain3()));
    ops.push(method(2, "includes", plain3()));
    ops
}

fn every_atomic(kind: Kind, shared: bool) -> Vec<Op> {
    let mut ops = vec![buf(0, 24, None, shared), view(0, kind, 0, 0, None)];
    for (name, _) in ATOMICS {
        ops.push(atomic(0, name, 1, p("3"), p("2")));
        ops.push(atomic(0, name, -1, p("1"), p("1")));
    }
    ops
}

fn every_kind() -> Vec<Op> {
    let mut ops = vec![buf(0, 24, Some(64), false)];
    for (n, kind) in Kind::ALL.into_iter().enumerate() {
        let slot = (n % 4) as u8;
        ops.push(view(slot, kind, 0, 0, None));
        ops.push(access(slot, 1, 2));
        ops.push(Op::Store {
            view: slot,
            idx: 2,
            val: p("7"),
            reps: 2,
        });
    }
    ops.push(Op::Resize { buf: 0, len: 3 });
    ops.push(access(0, 1, 2));
    ops
}

pub fn seeds() -> Vec<(&'static str, Vec<Op>)> {
    let shrink = Op::Resize { buf: 0, len: 3 };
    let hostile = |action, len| Arg::hostile("1", action, 0, len);
    vec![
        (
            "resizable-shrink-length-tracking",
            vec![
                buf(0, 16, Some(64), false),
                view(0, Kind::Int32, 0, 0, None),
                view(1, Kind::Uint8, 0, 4, Some(4)),
                access(0, 3, 2),
                shrink.clone(),
                access(0, 3, 1),
                access(1, 1, 1),
                method(0, "state", plain3()),
                method(1, "state", plain3()),
                Op::Resize { buf: 0, len: 24 },
                access(1, 1, 1),
            ],
        ),
        (
            "shared-grow",
            vec![
                buf(0, 8, Some(64), true),
                view(0, Kind::Int32, 0, 0, None),
                view(1, Kind::Uint8, 0, 0, Some(4)),
                Op::Resize { buf: 0, len: 24 },
                method(0, "state", plain3()),
                Op::Resize { buf: 0, len: 3 },
                method(0, "fill", plain3()),
            ],
        ),
        (
            "transfer-then-use-old-view",
            vec![
                buf(0, 16, Some(64), false),
                view(0, Kind::Uint16, 0, 0, None),
                Op::Transfer {
                    buf: 0,
                    mode: TransferMode::Same,
                },
                access(0, 1, 1),
                method(0, "at", plain3()),
                view(1, Kind::Uint8, 0, 0, None),
                Op::Transfer {
                    buf: 0,
                    mode: TransferMode::ToFixed,
                },
                Op::Transfer {
                    buf: 0,
                    mode: TransferMode::Fixed(64),
                },
                method(1, "state", plain3()),
                buf(2, 8, None, false),
                view(2, Kind::Float32, 2, 0, None),
                Op::Detach { buf: 2 },
                access(2, 0, 1),
                method(2, "state", plain3()),
            ],
        ),
        (
            "detach-inside-callbacks",
            vec![
                buf(0, 16, Some(64), false),
                view(0, Kind::Int8, 0, 0, None),
                method(0, "fill", [p("1"), p("0"), hostile(Action::Detach, 0)]),
                buf(0, 16, None, false),
                view(1, Kind::Int8, 0, 0, None),
                method(1, "sort", [hostile(Action::Detach, 0), p("0"), p("0")]),
                buf(0, 16, Some(64), false),
                view(2, Kind::Int8, 0, 0, None),
                method(2, "map", [hostile(Action::Resize, 3), p("0"), p("0")]),
                view(2, Kind::Int8, 0, 0, None),
                method(2, "set_array", [p("1"), p("2"), hostile(Action::Detach, 0)]),
            ],
        ),
        (
            "subarray-of-out-of-bounds-view",
            vec![
                buf(0, 16, Some(64), false),
                view(0, Kind::Int16, 0, 4, Some(4)),
                shrink.clone(),
                method(0, "subarray", plain3()),
                method(0, "slice", plain3()),
                method(0, "state", plain3()),
                view(1, Kind::Int16, 0, 0, None),
                method(1, "subarray", [p("1"), hostile(Action::Resize, 0), p("0")]),
            ],
        ),
        (
            "hostile-slice-and-copywithin",
            vec![
                buf(0, 24, Some(64), false),
                view(0, Kind::Uint8, 0, 0, None),
                method(0, "slice", [p("0"), hostile(Action::Resize, 3), p("0")]),
                Op::Resize { buf: 0, len: 24 },
                method(
                    0,
                    "copyWithin",
                    [p("0"), hostile(Action::Resize, 1), p("8")],
                ),
                Op::Resize { buf: 0, len: 24 },
                method(0, "indexOf", [p("1"), hostile(Action::Detach, 0), p("0")]),
                buf(0, 24, Some(64), false),
                view(0, Kind::Uint8, 0, 0, None),
                method(0, "slice", [p("0"), hostile(Action::Transfer, 8), p("0")]),
            ],
        ),
        ("atomics-int32", every_atomic(Kind::Int32, false)),
        ("atomics-int32-shared", every_atomic(Kind::Int32, true)),
        (
            "atomics-bigint64-shared",
            every_atomic(Kind::BigInt64, true),
        ),
        (
            "atomics-after-resize",
            vec![
                buf(0, 16, Some(64), true),
                view(0, Kind::Int32, 0, 0, None),
                atomic(0, "add", 3, p("1"), p("0")),
                Op::Resize { buf: 0, len: 3 },
                atomic(0, "load", 3, p("0"), p("0")),
                atomic(0, "wait", 0, p("0"), p("0")),
                atomic(0, "add", 0, hostile(Action::Resize, 0), p("0")),
            ],
        ),
        (
            "dataview-on-resizable",
            vec![
                buf(0, 16, Some(64), false),
                view(1, Kind::DataView, 0, 0, None),
                view(2, Kind::DataView, 0, 4, Some(4)),
                method(1, "dv_setInt32", [p("8"), p("7"), p("1")]),
                shrink,
                method(1, "dv_getInt16", plain3()),
                method(2, "dv_getInt16", plain3()),
                method(1, "state", plain3()),
                method(2, "state", plain3()),
            ],
        ),
        ("every-typed-array-kind", every_kind()),
        ("every-method", every_method()),
        (
            "warmed-access-then-resize",
            vec![
                buf(0, 16, Some(64), false),
                view(0, Kind::Uint8, 0, 0, None),
                access(0, 8, 40),
                Op::Store {
                    view: 0,
                    idx: 8,
                    val: p("7"),
                    reps: 40,
                },
                Op::Resize { buf: 0, len: 3 },
                access(0, 8, 40),
                Op::Store {
                    view: 0,
                    idx: 8,
                    val: hostile(Action::Detach, 0),
                    reps: 8,
                },
                access(0, 1, 8),
            ],
        ),
    ]
}

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus/typed_array")
}

fn file_name(n: usize, name: &str) -> String {
    format!("{:02}-{name}.bin", n + 1)
}

#[test]
#[ignore = "rewrites fuzz/corpus/typed_array"]
fn regenerate_seed_corpus() {
    let dir = corpus_dir();
    std::fs::create_dir_all(&dir).unwrap();
    for (n, (name, ops)) in seeds().iter().enumerate() {
        std::fs::write(dir.join(file_name(n, name)), encode(ops)).unwrap();
    }
}

#[test]
fn committed_seeds_match_definitions_and_run_clean() {
    let dir = corpus_dir();
    let defs = seeds();
    let mut on_disk: Vec<_> = std::fs::read_dir(&dir)
        .expect("seed corpus directory")
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    on_disk.sort();
    let expected: Vec<_> = defs
        .iter()
        .enumerate()
        .map(|(n, (name, _))| file_name(n, name))
        .collect();
    assert_eq!(on_disk, expected, "run regenerate_seed_corpus");

    let mut op_kinds = HashSet::new();
    let mut methods = HashSet::new();
    let mut atomics = HashSet::new();
    let mut kinds = HashSet::new();
    let mut hostile_actions = HashSet::new();
    for (n, (name, ops)) in defs.iter().enumerate() {
        let bytes = std::fs::read(dir.join(file_name(n, name))).unwrap();
        assert_eq!(
            bytes,
            encode(ops),
            "{name} is stale: run regenerate_seed_corpus"
        );
        assert_eq!(&decode(&bytes), ops, "{name} does not round-trip");
        assert!(bytes.len() <= 640, "{name} exceeds the target max_len");

        let src = super::script::render(ops);
        let (bytecode, tree_walker) = jsse::run_on_engine_stack(|| {
            (
                jsse::run_script_observed(&src, BUDGET, true),
                jsse::run_script_observed(&src, BUDGET, false),
            )
        });
        assert_eq!(bytecode, tree_walker, "{name}: engines disagree");
        assert!(
            matches!(bytecode.outcome, SigOutcome::Completed(_)),
            "{name}: {:?}",
            bytecode.outcome
        );

        for op in ops {
            op_kinds.insert(std::mem::discriminant(op));
            let args: &[Arg] = match op {
                Op::NewView { kind, .. } => {
                    kinds.insert(*kind);
                    &[]
                }
                Op::Method { name, args, .. } => {
                    methods.insert(*name);
                    args
                }
                Op::Atomic { op, args, .. } => {
                    atomics.insert(*op);
                    args
                }
                Op::Store { val, .. } => std::slice::from_ref(val),
                _ => &[],
            };
            hostile_actions.extend(
                args.iter()
                    .filter_map(|a| a.hostile.map(|h| h.action))
                    .map(|a| a as u8),
            );
        }
    }
    assert_eq!(op_kinds.len(), super::ops::OP_KINDS as usize);
    assert_eq!(methods.len(), METHODS.len());
    assert_eq!(atomics.len(), ATOMICS.len());
    assert_eq!(kinds.len(), Kind::ALL.len());
    assert_eq!(hostile_actions.len(), 3);
}
