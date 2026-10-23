use super::*;
use crate::{RunSignature, SigOutcome, execution_divergence, fuzz_execute_signatures};

fn scenario(
    op: Op,
    host: Host,
    hook: usize,
    action: Action,
    step: usize,
    shape: Shape,
) -> Scenario {
    Scenario {
        op,
        host,
        hook,
        action,
        step,
        shape,
    }
}

/// Renders `plan`, requires the program to be comparable (parses, finishes
/// within the step budget, avoids every skip/nondeterminism marker) and free
/// of bytecode/tree-walker divergence, and returns the bytecode signature.
fn run_checked(plan: &Plan) -> RunSignature {
    let rendered = plan.render();
    let src = rendered.as_str();
    crate::run_on_engine_stack(move || {
        let (bytecode, tree_walker) =
            fuzz_execute_signatures(src).unwrap_or_else(|| panic!("not comparable:\n{src}"));
        assert!(
            matches!(bytecode.outcome, SigOutcome::Completed(_)),
            "{:?}\n{src}",
            bytecode.outcome
        );
        assert_eq!(execution_divergence(&bytecode, &tree_walker), None, "{src}");
        bytecode
    })
}

fn single(s: Scenario) -> Plan {
    Plan { scenarios: vec![s] }
}

#[test]
fn decode_is_total_and_bounded() {
    let all_ff = [0xFF; 1024];
    for data in [&[][..], &[1][..], &[7, 3, 9][..], &all_ff[..]] {
        let plan = Plan::decode(data);
        assert!((1..=MAX_SCENARIOS).contains(&plan.scenarios.len()));
        assert!(!plan.render().is_empty());
    }
}

#[test]
fn decode_is_deterministic() {
    let data: Vec<u8> = (0..60u8).map(|b| b.wrapping_mul(37)).collect();
    assert_eq!(Plan::decode(&data).render(), Plan::decode(&data).render());
}

#[test]
fn rendered_numeric_literals_stay_small() {
    let big = Plan::decode(&[0xFF; 64]).render();
    assert!(!big.contains("4294967295") && !big.contains("65535"));
}

#[test]
fn comparator_callback_reenters_sort() {
    let sig = run_checked(&single(scenario(
        Op::Sort,
        Host::Callback,
        0,
        Action::Reenter,
        0,
        Shape::Array,
    )));
    assert!(sig.output.contains("cb,cb,cb"), "{}", sig.output);
}

#[test]
fn proxy_trap_fires_and_action_runs() {
    let sig = run_checked(&single(scenario(
        Op::Keys,
        Host::ProxyTrap,
        4,
        Action::Shrink,
        0,
        Shape::Array,
    )));
    assert!(sig.output.contains("ownKeys"), "{}", sig.output);
}

#[test]
fn generator_running_rejects_reentrant_next() {
    let sig = run_checked(&single(scenario(
        Op::Spread,
        Host::Generator,
        0,
        Action::GenNext,
        0,
        Shape::Array,
    )));
    assert!(sig.output.contains("ATypeError"), "{}", sig.output);
}

#[test]
fn every_action_and_shape_renders_runnable_program() {
    for action in Action::ALL {
        for shape in Shape::ALL {
            run_checked(&single(scenario(
                Op::Get,
                Host::Accessor,
                0,
                action,
                0,
                shape,
            )));
        }
    }
}

#[test]
fn every_op_and_host_hook_renders_runnable_program() {
    for (i, op) in Op::ALL.into_iter().enumerate() {
        for host in Host::ALL {
            for hook in 0..host.hook_count() {
                let action = Action::ALL[(i + hook) % Action::ALL.len()];
                let shape = op.shapes()[hook % op.shapes().len()];
                run_checked(&single(scenario(op, host, hook, action, hook % 4, shape)));
            }
        }
    }
}

#[test]
fn every_shape_is_reachable_from_some_op() {
    for shape in Shape::ALL {
        assert!(Op::ALL.iter().any(|op| op.shapes().contains(&shape)));
    }
}

#[test]
fn seeds_cover_every_op_and_host_and_run_clean() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fuzz/corpus/reentrancy");
    let mut ops = std::collections::HashSet::new();
    let mut hosts = std::collections::HashSet::new();
    for entry in std::fs::read_dir(&dir).expect("seed directory") {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        assert!(
            bytes.contains(&0),
            "seed must be binary so hooks leave it alone"
        );
        let plan = Plan::decode(&bytes);
        for s in &plan.scenarios {
            assert!(
                s.hook < s.host.hook_count(),
                "seed hook out of range for {:?}; regenerate with scripts/gen-fuzz-reentrancy-corpus.py",
                s.host
            );
            ops.insert(format!("{:?}", s.op));
            hosts.insert(format!("{:?}", s.host));
        }
        run_checked(&plan);
    }
    for op in Op::ALL {
        assert!(ops.contains(&format!("{op:?}")), "no seed for {op:?}");
    }
    for host in Host::ALL {
        assert!(hosts.contains(&format!("{host:?}")), "no seed for {host:?}");
    }
}
