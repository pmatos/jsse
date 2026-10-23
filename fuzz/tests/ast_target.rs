use arbitrary::{Arbitrary, Unstructured};
use jsse_fuzz::js_ast::*;
use jsse_fuzz::js_pools::*;
use jsse_fuzz::js_print::print;

const MARKERS: [&str; 13] = [
    "agent",
    "waitAsync",
    "sleep",
    "Date",
    "DateTimeFormat",
    "Temporal",
    "WeakRef",
    "FinalizationRegistry",
    "$262",
    "Atomics",
    "SharedArrayBuffer",
    "performance",
    "gc",
];

fn xorshift(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn random_buffer(state: &mut u64) -> Vec<u8> {
    let len = 16 + (xorshift(state) % 2033) as usize;
    (0..len).map(|_| xorshift(state) as u8).collect()
}

fn generate(bytes: &[u8]) -> String {
    let program = Program::arbitrary(&mut Unstructured::new(bytes)).unwrap();
    print(&program)
}

fn parses(src: &str) -> bool {
    jsse::run_on_engine_stack(move || jsse::bench_parse(src))
}

fn contains_ident(src: &str, ident: &str) -> bool {
    let is_ident_char = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    src.match_indices(ident).any(|(at, _)| {
        !src[..at].chars().next_back().is_some_and(is_ident_char)
            && !src[at + ident.len()..]
                .chars()
                .next()
                .is_some_and(is_ident_char)
    })
}

fn stmt(e: Expr) -> Stmt {
    Stmt::Expr(e)
}

fn program(body: Block) -> String {
    print(&Program {
        strict: false,
        body,
    })
}

#[test]
fn empty_input_yields_a_terminal_program() {
    let p = Program::arbitrary(&mut Unstructured::new(&[])).unwrap();
    assert!(p.body.is_empty());
    assert!(parses(&print(&p)));
}

#[test]
fn printed_programs_parse_in_the_engine() {
    let samples: usize = std::env::var("AST_PARSE_SAMPLES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2000);
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let (mut ok, mut failures, mut bytes) = (0usize, vec![], 0usize);
    for _ in 0..samples {
        let src = generate(&random_buffer(&mut state));
        bytes += src.len();
        if parses(&src) {
            ok += 1;
        } else if failures.len() < 3 {
            failures.push(src);
        }
    }
    let rate = ok as f64 / samples as f64;
    eprintln!(
        "parse rate: {ok}/{samples} = {:.2}% (mean source {} bytes)",
        rate * 100.0,
        bytes / samples
    );
    assert!(
        rate >= 0.90,
        "parse rate {rate:.3}; first failures:\n{}",
        failures.join("\n=====\n")
    );
}

#[test]
fn pools_contain_no_runner_marker_identifiers() {
    let mut texts: Vec<String> = vec![];
    for pool in [
        &VARS[..],
        &PARAMS,
        &CATCH,
        &LABELS,
        GLOBALS,
        PROPS,
        KEYS,
        METHODS,
        SYMBOLS,
        STRS,
        NUMS,
        REGEX,
        FLAGS,
        TPL,
        EVALS,
    ] {
        texts.extend(pool.iter().map(|s| s.to_string()));
    }
    for (global, methods) in STATICS {
        texts.push(global.to_string());
        texts.extend(methods.iter().map(|s| s.to_string()));
    }
    texts.extend((0..LEX_COUNT).map(|i| format!("n{i}")));
    for text in &texts {
        for marker in MARKERS {
            assert!(
                !contains_ident(text, marker),
                "pool entry {text:?} contains {marker}"
            );
        }
        assert!(
            !text.contains("use strict"),
            "pool entry {text:?} would form a directive"
        );
    }
}

#[test]
fn generated_sources_never_contain_runner_markers() {
    let mut state = 0xDEAD_BEEF_CAFE_F00Du64;
    for _ in 0..300 {
        let src = generate(&random_buffer(&mut state));
        for marker in MARKERS {
            assert!(!contains_ident(&src, marker), "{marker} in:\n{src}");
        }
    }
}

#[test]
fn worst_case_input_does_not_overflow_the_stack() {
    let patterns: [Vec<u8>; 3] = [
        vec![0xFF; 4096],
        vec![0x00; 4096],
        (0..4096).map(|i| (i % 7) as u8 * 40).collect(),
    ];
    for bytes in patterns {
        let result = std::thread::Builder::new()
            .stack_size(8 << 20)
            .spawn(move || generate(&bytes).len())
            .unwrap()
            .join();
        let len = result.expect("generator or printer overflowed the stack");
        assert!(len < 40_000, "unbounded output: {len} bytes");
    }
}

#[test]
fn context_dependent_statements_degrade_outside_their_context() {
    let src = program(vec![
        Stmt::Break(None),
        Stmt::Continue(Some(0)),
        Stmt::Return(Some(Expr::Int(1))),
        stmt(Expr::Yield(false, None)),
        stmt(Expr::Await(Box::new(Expr::Int(1)))),
        stmt(Expr::SuperCall(vec![])),
        stmt(Expr::SuperMember(0)),
        stmt(Expr::NewTarget),
        stmt(Expr::Arguments),
    ]);
    assert!(parses(&src), "{src}");
    for bad in ["break", "continue", "return", "yield", "await", "super"] {
        assert!(!contains_ident(&src, bad), "{bad} survived:\n{src}");
    }
}

#[test]
fn context_dependent_statements_print_inside_their_context() {
    let loop_body = vec![
        Stmt::Break(Some(0)),
        Stmt::Continue(Some(0)),
        Stmt::Continue(None),
    ];
    let func = FuncDef {
        is_async: true,
        is_gen: true,
        strict: true,
        params: Params {
            count: 2,
            style: PStyle::Simple,
        },
        body: vec![
            Stmt::Return(Some(Expr::Await(Box::new(Expr::Int(1))))),
            stmt(Expr::Yield(true, None)),
            Stmt::Labeled(0, Box::new(Stmt::While(Expr::Bool(false), loop_body))),
        ],
    };
    let src = program(vec![Stmt::Func(0, func)]);
    assert!(parses(&src), "{src}");
    for needed in [
        "return",
        "await",
        "yield*",
        "break L0",
        "continue L0",
        "continue;",
    ] {
        assert!(src.contains(needed), "missing {needed}:\n{src}");
    }
}

#[test]
fn lexical_names_are_unique_per_scope() {
    let lets = (0..8)
        .map(|i| Stmt::Lex(i % 2 == 0, 0, Expr::Int(i as i16)))
        .collect();
    let funcs: Block = (0..8)
        .map(|_| {
            Stmt::Func(
                3,
                FuncDef {
                    is_async: false,
                    is_gen: false,
                    strict: false,
                    params: Params {
                        count: 0,
                        style: PStyle::Simple,
                    },
                    body: vec![],
                },
            )
        })
        .collect();
    for body in [lets, funcs] {
        let src = program(body);
        assert!(parses(&src), "{src}");
    }
}

#[test]
fn classes_with_privates_heritage_and_static_blocks_parse() {
    let method = |k| {
        MemberKind::Method(
            k,
            FuncDef {
                is_async: true,
                is_gen: true,
                strict: false,
                params: Params {
                    count: 1,
                    style: PStyle::Rest,
                },
                body: vec![stmt(Expr::PrivateGet(Box::new(Expr::This), 0))],
            },
        )
    };
    let class = ClassDef {
        extends: Some(Box::new(Expr::Global(1))),
        ctor: Some(Ctor {
            params: Params {
                count: 2,
                style: PStyle::Defaults(Expr::Int(1)),
            },
            super_first: true,
            body: vec![stmt(Expr::SuperMember(2))],
        }),
        members: vec![
            Member {
                is_static: false,
                kind: MemberKind::Field(Key::Private, Some(Expr::This)),
            },
            Member {
                is_static: true,
                kind: method(Key::Private),
            },
            Member {
                is_static: false,
                kind: MemberKind::Get(
                    Key::Name(1),
                    vec![stmt(Expr::PrivateIn(0, Box::new(Expr::This)))],
                ),
            },
            Member {
                is_static: false,
                kind: MemberKind::Set(Key::Computed(Expr::WellKnown(0)), vec![]),
            },
            Member {
                is_static: true,
                kind: MemberKind::StaticBlock(vec![stmt(Expr::NewTarget)]),
            },
        ],
    };
    let src = program(vec![
        Stmt::Class(0, class),
        stmt(Expr::Class(Box::new(ClassDef {
            extends: None,
            ctor: None,
            members: vec![],
        }))),
    ]);
    assert!(parses(&src), "{src}");
}
