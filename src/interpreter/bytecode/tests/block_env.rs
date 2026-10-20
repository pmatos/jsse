//! Block environments of loops: which loops share one environment between
//! the head and the body block, and how `break` leaves the ones that do not.

use super::*;

/// Whether the loop's last `PopEnv` comes after its back edge, that is,
/// whether the environment is entered and left once per loop rather than
/// once per iteration.
fn env_spans_the_loop(source: &str, params: &[&str]) -> bool {
    let ops = work_ops(&compile_function_source(source, params));
    let pop = ops
        .iter()
        .rposition(|op| *op == Op::PopEnv)
        .expect("PopEnv");
    let back_edge = ops.iter().rposition(|op| *op == Op::Jump).expect("Jump");
    pop > back_edge
}

#[test]
fn non_capturing_for_let_keeps_one_environment_for_every_iteration() {
    let chunk = compile_function_source(
        "for (let i = 0; i < n; i++) { const v = i * 2; s += v; }",
        &["n", "s"],
    );
    assert_eq!(count_ops(&chunk, Op::ExecAst), 0, "{:?}", work_ops(&chunk));
    assert_eq!(count_ops(&chunk, Op::PushEnv), 1, "head and body share one");
    assert_eq!(count_ops(&chunk, Op::CopyIterEnv), 0);
}

#[test]
fn loop_bodies_share_the_loop_environment_unless_it_is_observable() {
    let params = ["n", "s", "xs", "o", "fs"];
    for shared in [
        "while (s < n) { let t = s + 1; s = t; }",
        "do { const t = s + 1; s = t; } while (s < n);",
        "for (var i = 0; i < n; i++) { let t = i; s += t; }",
        "for (const x of xs) { let t = x; s += t; }",
        "for (var k in o) { const { a } = o; s += a; }",
    ] {
        assert!(env_spans_the_loop(shared, &params), "{shared}");
    }
    for fresh in [
        "for (const x of xs) { let t = x; s += t; fs.push(() => t); }",
        "while (s < n) { try { s += t; } catch (e) {} let t = s; }",
        "var t = 1; while (t < n) { let t = s; s = t; }",
        "for (var i = 0; i < n; i++) { const [a] = xs; s += a; }",
        "for (var i = 0; i < n; i++) { let t = i; s += t; fs.push(() => t); }",
    ] {
        assert!(!env_spans_the_loop(fresh, &params), "{fresh}");
    }
}

#[test]
fn a_shared_loop_body_has_no_pop_env_inside_the_loop() {
    let chunk = compile_function_source(
        "for (let i = 0; i < n; i++) { let a = i; if (a > 2) break; s += a; }",
        &["n", "s"],
    );
    let pops: Vec<u16> = decode_ops(&chunk)
        .into_iter()
        .filter(|(op, _)| *op == Op::PopEnv)
        .map(|(_, operands)| operands[0])
        .collect();
    assert_eq!(pops, vec![1], "only the loop exit leaves the environment");
}

#[test]
fn local_ops_address_block_bindings_and_still_hit_the_frame_for_outer_names() {
    use super::super::chunk::BLOCK_SLOT_FLAG;
    let chunk = compile_function_source(
        "for (let i = 0; i < n; i++) { const v = i + 1; s = s + v; }",
        &["n", "s"],
    );
    let slots: Vec<(String, u16)> = decode_ops(&chunk)
        .into_iter()
        .filter(|(op, _)| {
            matches!(
                op,
                Op::LoadLocal
                    | Op::StoreLocal
                    | Op::StoreLocalPop
                    | Op::LocalLocalBin
                    | Op::MoveLocal
            )
        })
        .flat_map(|(op, operands)| {
            let refs = if matches!(op, Op::LocalLocalBin | Op::MoveLocal) {
                2
            } else {
                1
            };
            operands
                .chunks(2)
                .take(refs)
                .map(|pair| (chunk.names[usize::from(pair[0])].to_string(), pair[1]))
                .collect::<Vec<_>>()
        })
        .collect();
    let slot_of = |name: &str| {
        slots
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("no local op names {name}: {slots:?}"))
            .1
    };
    assert_eq!(slot_of("s") & BLOCK_SLOT_FLAG, 0, "frame local stays frame");
    assert_eq!(slot_of("v") & BLOCK_SLOT_FLAG, BLOCK_SLOT_FLAG);
    assert_eq!(slot_of("v") >> 8, 0x80, "the body shares the head scope");
    assert_eq!(slot_of("v") & 0xff, 1, "after the head's i");
}

#[test]
fn break_out_of_nested_blocks_leaves_their_environments_first() {
    let chunk = compile_function_source(
        "for (let i = 0; i < n; i++) { let a = i; g(() => a); { let b = a; if (b > 2) break; } }",
        &["n", "g"],
    );
    let pops: Vec<u16> = decode_ops(&chunk)
        .into_iter()
        .filter(|(op, _)| *op == Op::PopEnv)
        .map(|(_, operands)| operands[0])
        .collect();
    assert!(
        pops.contains(&2),
        "the break leaves the two blocks it is nested in: {pops:?}"
    );
}
