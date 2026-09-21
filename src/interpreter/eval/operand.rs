//! The one place that decides what a `StateTerminator`'s operand expression
//! produced.
//!
//! Three state-machine drivers — the sync generator and async generator in
//! `generator_runtime.rs`, and `async_function_resume` in `eval.rs` — evaluate
//! terminator operands at 22 sites. Each site used to re-derive the same
//! question inline: *which completion kinds carry a value here, and which abort
//! the driver?* The three drivers had drifted into three answers, and 11 of the
//! 22 sites silently turned a [`Completion::Exit`] — an uncatchable
//! `__host_exit`, issue #242 — into `JsValue::UNDEFINED`, so an async function
//! kept running past its own process exit.
//!
//! Routing a throw and tearing down on an exit stay with each driver: both need
//! half a dozen of the driver's locals and resolve by `continue`/`return` out of
//! its state loop, which cannot cross a function boundary. What lives here is
//! the classification — the part that had no business differing.

use super::*;

/// What a `StateTerminator` operand expression produced, once any tail-call
/// chain has been driven to a real completion.
///
/// The split between [`Operand::Abort`] and [`Operand::Other`] is deliberate
/// and load-bearing. `Exit` is the only completion every driver must propagate,
/// so it gets its own variant and no driver can reach it through a catch-all.
/// `Return`/`Break`/`Continue`/`Empty` stay in `Other` because the drivers
/// legitimately answer differently for them today, and folding them in would
/// turn an `Exit` fix into an unreviewed behaviour change at every site.
#[derive(Debug)]
pub(crate) enum Operand {
    /// `Completion::Normal` — the operand's value.
    Value(JsValue),
    /// `Completion::Throw` — a catchable JS exception. Routing it is genuinely
    /// driver-specific (a try-stack walk, a promise rejection, or a
    /// `pending_exception` stash), so the seam hands it straight back.
    Throw(JsValue),
    /// `Completion::Yield` — the operand suspended mid-expression.
    Suspend(JsValue),
    /// `Completion::Exit` — an uncatchable host exit. The driver must tear down
    /// and propagate it verbatim; it is never a value.
    Abort(Completion),
    /// `Return` / `Break` / `Continue` / `Empty`. An *expression* should not
    /// produce these, and each driver keeps whatever it does today.
    Other(Completion),
}

/// Classify a drained completion.
///
/// Pure: no interpreter, no control flow. This is the rule that drifted three
/// ways, and being a total match over [`Completion`] it is also the one place
/// that stops compiling if a variant is ever added — which is precisely how
/// `Completion::Exit` slipped past 11 catch-all arms when it was introduced.
pub(crate) fn classify_operand(completion: Completion) -> Operand {
    match completion {
        Completion::Normal(v) => Operand::Value(v),
        Completion::Throw(e) => Operand::Throw(e),
        Completion::Yield(v) => Operand::Suspend(v),
        exit @ Completion::Exit(_) => Operand::Abort(exit),
        other @ (Completion::Return(_)
        | Completion::Break(..)
        | Completion::Continue(..)
        | Completion::Empty
        | Completion::TailCall { .. }) => Operand::Other(other),
    }
}

impl Interpreter {
    /// Evaluate a `StateTerminator` operand expression: run it, drive any
    /// tail-call chain to a real completion, then classify.
    ///
    /// The drain used to exist at only 6 of the 22 operand sites. It is
    /// normalised here as hardening — `Completion::TailCall` is only produced
    /// under `in_tail_position`, which the drivers never set, so no test claims
    /// this as a fixed defect.
    pub(crate) fn eval_operand(&mut self, operand: &Expression, env: &EnvRef) -> Operand {
        let mut result = self.eval_expr(operand, env);
        while let Completion::TailCall { func, this, args } = result {
            result = self.call_function(&func, &this, &args);
        }
        classify_operand(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule that drifted, pinned directly.
    ///
    /// `Exit` classifying as [`Operand::Abort`] rather than falling into a
    /// value-producing arm is the whole point of the seam: before it, three
    /// drivers each answered this inline and eleven of the twenty-two operand
    /// sites read a host exit as `undefined`.
    #[test]
    fn classify_operand_separates_values_from_completions_that_abort() {
        assert!(matches!(
            classify_operand(Completion::Exit(42)),
            Operand::Abort(Completion::Exit(42))
        ));
        assert!(matches!(
            classify_operand(Completion::Normal(JsValue::UNDEFINED)),
            Operand::Value(_)
        ));
        assert!(matches!(
            classify_operand(Completion::Throw(JsValue::UNDEFINED)),
            Operand::Throw(_)
        ));
        assert!(matches!(
            classify_operand(Completion::Yield(JsValue::UNDEFINED)),
            Operand::Suspend(_)
        ));
        for other in [
            Completion::Empty,
            Completion::Return(JsValue::UNDEFINED),
            Completion::Break(None, None),
            Completion::Continue(None, None),
        ] {
            assert!(matches!(classify_operand(other), Operand::Other(_)));
        }
    }

    /// `Exit` must never be reachable through the variant the drivers treat as
    /// "no value, keep going" — that equivalence is what the bug was.
    #[test]
    fn classify_operand_never_files_an_exit_under_other() {
        assert!(!matches!(
            classify_operand(Completion::Exit(7)),
            Operand::Other(_) | Operand::Value(_) | Operand::Suspend(_)
        ));
    }
}
