//! Opt-in execution counters for performance investigations (issue #526).
//!
//! Compiled only under `--features perf-counters`. The default binary carries
//! no counter writes at all, so a counting build and a timing build are never
//! the same binary and a measured wall time is never inflated by the
//! instrumentation that explains it.
//!
//! The counters answer "where does the work go", not "where does the time go":
//! every one is a deterministic count, so two runs of the same deterministic
//! workload must produce identical output regardless of host load.

use crate::interpreter::bytecode::op::Op;
use rustc_hash::FxHashMap;
use std::fmt::Write as _;
use std::rc::Rc;

/// Widest opcode discriminant plus one. The assertion below fails to compile
/// if a new opcode outgrows the histogram.
const OP_SLOTS: usize = 64;

/// Identity of an attributed body: its display name plus the function object it
/// came from. Two distinct functions can share a name, and keying on the name
/// alone merged their work and let their bail reasons overwrite each other —
/// the per-function ranking could then identify neither (#537 review, third
/// pass). The id disambiguates internally; `report` only shows it when a name
/// is actually ambiguous, so unique names stay readable.
pub(crate) type BodyKey = (Rc<str>, u64);

/// Identity for synthetic bodies that have no function object at all
/// (`<script body>`, `<module body>`, `<eval>`, and unlabelled state-machine
/// bodies). Object ids are allocated upward from 0, so this can never collide.
pub(crate) const SYNTHETIC_BODY_ID: u64 = u64::MAX;
const _: () = assert!((Op::Construct as usize) < OP_SLOTS);

pub(crate) struct PerfCounters {
    /// Opcodes dispatched by `vm::run_chunk_inner`.
    pub(crate) vm_ops: u64,
    pub(crate) vm_op_hist: [u64; OP_SLOTS],
    /// Tree-walker work units: `exec_statement` and `eval_expr` entries.
    pub(crate) ast_stmts: u64,
    pub(crate) ast_exprs: u64,
    /// `dispatch_body` outcomes — the invocation split #524 measured. These
    /// two count *function invocations* and nothing else; the non-function
    /// body paths below are deliberately kept out of them so the compiled/AST
    /// share stays comparable with the figures already published for #524.
    pub(crate) body_compiled: u64,
    pub(crate) body_ast: u64,
    /// Executions of the body paths that bypass `dispatch_body` (generator and
    /// async state machines, the top-level script fallback, `eval`). This is a
    /// count of *executions, not invocations*: a generator resumption
    /// re-executes its body under replay, so one generator call with N yields
    /// registers roughly 4N here.
    pub(crate) body_non_function: u64,
    /// AST work units spent inside *function* bodies, exclusive of nested
    /// bodies. `ast_units()` covers script/generator/module/eval work too, so
    /// dividing it by `body_ast` — which counts function invocations only —
    /// mixed populations: one 2-unit function call beside a generator reported
    /// an average of 1,066 units per body (#537 review, fourth pass).
    pub(crate) ast_units_in_functions: u64,
    /// Compile attempts, and what each bail was blamed on.
    pub(crate) compile_ok: u64,
    pub(crate) compile_bail: FxHashMap<&'static str, u64>,
    /// Where a `[[Call]]` came from, and what it reached.
    pub(crate) calls_from_vm: u64,
    pub(crate) calls_to_native: u64,
    pub(crate) calls_to_user: u64,
    /// GC collections, and the wall time inside them. Collections are rare
    /// enough (thousands, not millions) that timing them adds no measurable
    /// distortion, unlike timing a per-opcode or per-call boundary.
    pub(crate) gc_minor: u64,
    pub(crate) gc_major: u64,
    pub(crate) gc_nanos: u128,
    /// Per-body AST attribution. `invocation share` cannot localize cost — a
    /// single fallback body can wrap a whole loop — so bodies are ranked by
    /// the tree-walker work units they consume *exclusive* of nested bodies.
    pub(crate) ast_body_units: FxHashMap<BodyKey, (u64, u64)>,
    /// Which construct each named body bailed on, so an eligibility expansion
    /// can be aimed at the bodies that actually hold the work.
    pub(crate) bail_by_name: FxHashMap<BodyKey, &'static str>,
    /// (key, is function invocation, ast units at entry, units consumed by
    /// nested bodies).
    ast_body_stack: Vec<(BodyKey, bool, u64, u64)>,
    /// Interned fallback label for state-machine bodies without a function
    /// object, plus labels for the other synthetic body paths. Framing them
    /// costs an `Rc` clone rather than an allocation per entry.
    pub(crate) name_non_function_body: Rc<str>,
    pub(crate) name_script_body: Rc<str>,
    pub(crate) name_module_body: Rc<str>,
    pub(crate) name_eval_body: Rc<str>,
}

impl Default for PerfCounters {
    fn default() -> Self {
        Self {
            vm_ops: 0,
            vm_op_hist: [0; OP_SLOTS],
            ast_stmts: 0,
            ast_exprs: 0,
            body_compiled: 0,
            body_ast: 0,
            body_non_function: 0,
            ast_units_in_functions: 0,
            compile_ok: 0,
            compile_bail: FxHashMap::default(),
            calls_from_vm: 0,
            calls_to_native: 0,
            calls_to_user: 0,
            gc_minor: 0,
            gc_major: 0,
            gc_nanos: 0,
            ast_body_units: FxHashMap::default(),
            bail_by_name: FxHashMap::default(),
            ast_body_stack: Vec::new(),
            name_non_function_body: Rc::from("<generator/async body>"),
            name_script_body: Rc::from("<script body>"),
            name_module_body: Rc::from("<module body>"),
            name_eval_body: Rc::from("<eval>"),
        }
    }
}

impl PerfCounters {
    pub(crate) fn record_op(&mut self, op: Op) {
        self.vm_ops += 1;
        self.vm_op_hist[op as usize] += 1;
    }

    pub(crate) fn record_bail(&mut self, reason: &'static str, name: Rc<str>, id: u64) {
        *self.compile_bail.entry(reason).or_insert(0) += 1;
        self.bail_by_name.insert((name, id), reason);
    }

    fn ast_units(&self) -> u64 {
        self.ast_stmts + self.ast_exprs
    }

    /// Starts attributing AST work to a Body. `is_function_invocation` is
    /// independent of whether the Body has a real function identity: named
    /// generator/async state-machine steps are executions, not invocations.
    pub(crate) fn enter_ast_body(&mut self, name: Rc<str>, id: u64, is_function_invocation: bool) {
        let at_entry = self.ast_units();
        self.ast_body_stack
            .push(((name, id), is_function_invocation, at_entry, 0));
    }

    /// Pops the innermost body, credits it the units it spent outside any
    /// nested body, and charges its inclusive cost to its caller's child total
    /// so no unit is counted twice.
    pub(crate) fn leave_ast_body(&mut self) {
        let now = self.ast_units();
        let Some((key, is_function_invocation, at_entry, children)) = self.ast_body_stack.pop()
        else {
            return;
        };
        let inclusive = now.saturating_sub(at_entry);
        let exclusive = inclusive.saturating_sub(children);
        if is_function_invocation {
            self.ast_units_in_functions += exclusive;
        }
        let entry = self.ast_body_units.entry(key).or_insert((0, 0));
        entry.0 += exclusive;
        entry.1 += 1;
        if let Some(parent) = self.ast_body_stack.last_mut() {
            parent.3 += inclusive;
        }
    }

    pub(crate) fn report(&self) -> String {
        let mut out = String::new();
        let ast_units = self.ast_stmts + self.ast_exprs;
        let _ = writeln!(out, "PERF\tvm_ops\t{}", self.vm_ops);
        let _ = writeln!(out, "PERF\tast_stmt_execs\t{}", self.ast_stmts);
        let _ = writeln!(out, "PERF\tast_expr_evals\t{}", self.ast_exprs);
        let _ = writeln!(out, "PERF\tast_work_units\t{ast_units}");
        let _ = writeln!(out, "PERF\tbody_dispatch_compiled\t{}", self.body_compiled);
        let _ = writeln!(out, "PERF\tbody_dispatch_ast\t{}", self.body_ast);
        let _ = writeln!(
            out,
            "PERF\tbody_non_function_execs\t{}",
            self.body_non_function
        );
        if self.body_compiled != 0 {
            let _ = writeln!(
                out,
                "PERF\tvm_ops_per_compiled_body\t{:.2}",
                self.vm_ops as f64 / self.body_compiled as f64
            );
        }
        let _ = writeln!(
            out,
            "PERF\tast_units_in_functions\t{}",
            self.ast_units_in_functions
        );
        if self.body_ast != 0 {
            // Function work over function invocations. Using the run-wide
            // `ast_units` here would divide script/generator/module/eval work
            // by a function-only denominator.
            let _ = writeln!(
                out,
                "PERF\tast_units_per_ast_body\t{:.2}",
                self.ast_units_in_functions as f64 / self.body_ast as f64
            );
        }
        let _ = writeln!(out, "PERF\tcompile_ok\t{}", self.compile_ok);
        let bail_total: u64 = self.compile_bail.values().sum();
        let _ = writeln!(out, "PERF\tcompile_bail\t{bail_total}");
        let _ = writeln!(out, "PERF\tcalls_from_vm\t{}", self.calls_from_vm);
        let _ = writeln!(out, "PERF\tcalls_to_native\t{}", self.calls_to_native);
        let _ = writeln!(out, "PERF\tcalls_to_user\t{}", self.calls_to_user);
        let _ = writeln!(out, "PERF\tgc_minor\t{}", self.gc_minor);
        let _ = writeln!(out, "PERF\tgc_major\t{}", self.gc_major);
        let _ = writeln!(out, "PERF\tgc_ms\t{}", self.gc_nanos / 1_000_000);

        let mut bails: Vec<_> = self.compile_bail.iter().collect();
        bails.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (reason, count) in bails {
            let _ = writeln!(out, "BAIL\t{reason}\t{count}");
        }

        // A name shared by several function objects is rendered `name#id`; a
        // unique name stays bare, so the common case reads cleanly and only
        // genuine ambiguity costs the reader anything.
        let mut ids_per_name: FxHashMap<&Rc<str>, usize> = FxHashMap::default();
        {
            let mut seen: std::collections::HashSet<(&Rc<str>, u64)> =
                std::collections::HashSet::new();
            for (name, id) in self.ast_body_units.keys() {
                if seen.insert((name, *id)) {
                    *ids_per_name.entry(name).or_insert(0) += 1;
                }
            }
        }
        let mut bodies: Vec<_> = self.ast_body_units.iter().collect();
        bodies.sort_by_key(|&(_, &(exclusive, _))| std::cmp::Reverse(exclusive));
        let ast_total: u64 = self.ast_body_units.values().map(|v| v.0).sum();
        for (key, (exclusive, invocations)) in bodies.into_iter().take(40) {
            let (name, id) = key;
            let ambiguous = ids_per_name.get(name).copied().unwrap_or(1) > 1;
            let label = if ambiguous {
                std::borrow::Cow::Owned(format!("{name}#{id}"))
            } else {
                std::borrow::Cow::Borrowed(name.as_ref())
            };
            let share = if ast_total == 0 {
                0.0
            } else {
                100.0 * *exclusive as f64 / ast_total as f64
            };
            let reason = self.bail_by_name.get(key).copied().unwrap_or("-");
            let _ = writeln!(
                out,
                "BODY\t{label}\t{exclusive}\t{share:.2}%\t{invocations}\t{reason}"
            );
        }

        let mut ops: Vec<(Op, u64)> = (0..OP_SLOTS)
            .filter_map(|i| Op::from_u8(i as u8).map(|op| (op, self.vm_op_hist[i])))
            .filter(|&(_, n)| n != 0)
            .collect();
        ops.sort_by_key(|&(_, count)| std::cmp::Reverse(count));
        for (op, count) in ops {
            let share = if self.vm_ops == 0 {
                0.0
            } else {
                100.0 * count as f64 / self.vm_ops as f64
            };
            let _ = writeln!(out, "OP\t{op:?}\t{count}\t{share:.2}%");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(s: &str) -> Rc<str> {
        Rc::from(s)
    }

    /// Test bodies get distinct synthetic identities unless a test is
    /// deliberately exercising the same-name-different-object case.
    fn key(s: &str, id: u64) -> BodyKey {
        (Rc::from(s), id)
    }

    /// A body that spends its own units is credited them.
    #[test]
    fn exclusive_attribution_credits_a_leaf_body() {
        let mut p = PerfCounters::default();
        p.enter_ast_body(name("leaf"), 1, true);
        p.ast_exprs += 7;
        p.leave_ast_body();
        assert_eq!(p.ast_body_units[&key("leaf", 1)], (7, 1));
    }

    /// A caller must not be credited with work its callee did: the whole point
    /// of ranking by exclusive units is that a body wrapping a loop nest does
    /// not absorb the cost of everything it calls.
    #[test]
    fn exclusive_attribution_does_not_credit_a_caller_with_callee_work() {
        let mut p = PerfCounters::default();
        p.enter_ast_body(name("caller"), 1, true);
        p.ast_stmts += 2;
        p.enter_ast_body(name("callee"), 1, true);
        p.ast_exprs += 100;
        p.leave_ast_body();
        p.ast_stmts += 3;
        p.leave_ast_body();
        assert_eq!(p.ast_body_units[&key("callee", 1)], (100, 1));
        assert_eq!(p.ast_body_units[&key("caller", 1)], (5, 1));
    }

    /// Repeated invocations accumulate units and count.
    #[test]
    fn exclusive_attribution_accumulates_across_invocations() {
        let mut p = PerfCounters::default();
        for _ in 0..3 {
            p.enter_ast_body(name("hot"), 1, true);
            p.ast_exprs += 4;
            p.leave_ast_body();
        }
        assert_eq!(p.ast_body_units[&key("hot", 1)], (12, 3));
    }

    /// An unbalanced leave (a body left without a matching enter) must not
    /// panic or corrupt a sibling's total.
    #[test]
    fn unbalanced_leave_is_ignored() {
        let mut p = PerfCounters::default();
        p.leave_ast_body();
        p.enter_ast_body(name("body"), 1, true);
        p.ast_exprs += 1;
        p.leave_ast_body();
        assert_eq!(p.ast_body_units[&key("body", 1)], (1, 1));
    }

    #[test]
    fn record_op_histograms_by_opcode() {
        let mut p = PerfCounters::default();
        p.record_op(Op::Add);
        p.record_op(Op::Add);
        p.record_op(Op::Call);
        assert_eq!(p.vm_ops, 3);
        assert_eq!(p.vm_op_hist[Op::Add as usize], 2);
        assert_eq!(p.vm_op_hist[Op::Call as usize], 1);
    }

    /// Two functions sharing a name must stay separate rows, and the id is
    /// shown only for the ambiguous name — a unique name stays bare so the
    /// common report reads cleanly (#537 review, third pass).
    #[test]
    fn report_disambiguates_only_ambiguous_names() {
        let mut p = PerfCounters::default();
        for (n, id, units) in [("same", 10u64, 700u64), ("same", 11, 20), ("solo", 12, 5)] {
            p.enter_ast_body(name(n), id, true);
            p.ast_exprs += units;
            p.leave_ast_body();
        }
        let out = p.report();
        assert!(out.contains("BODY\tsame#10\t700\t"), "{out}");
        assert!(out.contains("BODY\tsame#11\t20\t"), "{out}");
        assert!(out.contains("BODY\tsolo\t5\t"), "{out}");
        assert!(
            !out.contains("BODY\tsolo#"),
            "unique name must stay bare:\n{out}"
        );
    }

    /// A bail recorded against one of two same-named functions must not be
    /// reported against the other.
    #[test]
    fn bail_reasons_do_not_leak_between_same_named_functions() {
        let mut p = PerfCounters::default();
        p.record_bail("statement:Try", name("same"), 10);
        for id in [10u64, 11] {
            p.enter_ast_body(name("same"), id, true);
            p.ast_exprs += 1;
            p.leave_ast_body();
        }
        let out = p.report();
        assert!(
            out.contains("BODY\tsame#10\t1\t50.00%\t1\tstatement:Try"),
            "{out}"
        );
        assert!(out.contains("BODY\tsame#11\t1\t50.00%\t1\t-"), "{out}");
    }

    #[test]
    fn report_renders_shares_bails_and_bodies() {
        let mut p = PerfCounters::default();
        p.record_op(Op::Add);
        p.record_op(Op::Add);
        p.record_op(Op::GetElement);
        p.body_compiled = 1;
        p.body_ast = 1;
        p.record_bail("statement:Labeled", name("sortMinDown"), 1);
        p.enter_ast_body(name("sortMinDown"), 1, true);
        p.ast_exprs += 9;
        p.leave_ast_body();
        let out = p.report();
        assert!(out.contains("PERF\tvm_ops\t3\n"), "{out}");
        assert!(
            out.contains("PERF\tvm_ops_per_compiled_body\t3.00\n"),
            "{out}"
        );
        assert!(out.contains("BAIL\tstatement:Labeled\t1\n"), "{out}");
        assert!(
            out.contains("BODY\tsortMinDown\t9\t100.00%\t1\tstatement:Labeled\n"),
            "{out}"
        );
        // Histogram is ordered by count, not by discriminant.
        let add = out.find("OP\tAdd").expect("Add row");
        let get = out.find("OP\tGetElement").expect("GetElement row");
        assert!(add < get, "{out}");
        assert!(out.contains("OP\tAdd\t2\t66.67%\n"), "{out}");
    }

    /// The two synthetic labels must be distinct interned values, since a
    /// generator body and an `eval` body are separate buckets in the ranking.
    #[test]
    fn synthetic_body_labels_are_distinct() {
        let p = PerfCounters::default();
        assert_ne!(p.name_non_function_body.as_ref(), p.name_eval_body.as_ref());
        assert!(p.name_non_function_body.starts_with('<'));
        assert!(p.name_eval_body.starts_with('<'));
    }

    /// A body that never bailed is reported with a `-` reason rather than
    /// borrowing an unrelated body's label.
    #[test]
    fn report_marks_a_compiled_bodys_absent_bail_reason() {
        let mut p = PerfCounters::default();
        p.enter_ast_body(name("plain"), 1, true);
        p.ast_stmts += 1;
        p.leave_ast_body();
        assert!(p.report().contains("BODY\tplain\t1\t100.00%\t1\t-\n"));
    }
}
