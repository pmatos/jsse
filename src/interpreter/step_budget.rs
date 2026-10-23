//! Opt-in deterministic step budget: a counter ticked at every counted
//! safepoint (statement boundary, loop back-edge, microtask job, timer
//! callback). Exhaustion is fatal and uncatchable; it rides the
//! `Completion::Exit` / `pending_exit` machinery.

use super::{Completion, EnvRef, Interpreter, scheduler};
use crate::ast::Statement;
use crate::types::JsValue;

/// Process exit status for budget exhaustion (the `timeout(1)` convention).
const STEP_BUDGET_EXIT_CODE: i32 = 124;

#[derive(Default)]
pub(crate) struct StepBudget {
    limit: u64,
    used: u64,
    active: bool,
    exhausted: bool,
}

impl StepBudget {
    pub(crate) fn set(&mut self, limit: Option<u64>) {
        self.limit = limit.unwrap_or(0);
        self.active = limit.is_some();
        self.used = 0;
        self.exhausted = false;
    }

    #[inline(always)]
    pub(crate) fn is_active(&self) -> bool {
        self.active
    }

    /// Count one step (`used` saturates at the limit); true once the budget is exhausted (sticky).
    pub(crate) fn tick(&mut self) -> bool {
        if self.exhausted {
            return true;
        }
        if self.used == self.limit {
            self.exhausted = true;
        } else {
            self.used += 1;
        }
        self.exhausted
    }

    pub(crate) fn used(&self) -> u64 {
        self.used
    }

    pub(crate) fn exhausted(&self) -> bool {
        self.exhausted
    }
}

impl Interpreter {
    /// Install (or clear, with `None`) a deterministic step budget and reset
    /// the step count. One step is one counted safepoint: a statement
    /// boundary, a loop back-edge, a microtask job or a timer callback.
    /// Exhaustion is fatal and uncatchable: the run ends like `__host_exit`,
    /// with `pending_exit == Some(STEP_BUDGET_EXIT_CODE)`.
    pub(crate) fn set_step_budget(&mut self, limit: Option<u64>) {
        self.step_budget.set(limit);
        self.gc.set_budget_active(limit.is_some());
    }

    pub(crate) fn step_budget_exhausted(&self) -> bool {
        self.step_budget.exhausted()
    }

    pub(crate) fn steps_used(&self) -> u64 {
        self.step_budget.used()
    }

    /// `gc_safepoint` for hot paths: one inline test of `poll`, the call only
    /// when a request, stress mode or a step budget makes it do something.
    /// True when the budget is exhausted: the caller must stop with
    /// `Completion::Exit` (the code is latched in `pending_exit`).
    #[inline(always)]
    pub(crate) fn gc_safepoint_poll(&mut self) -> bool {
        if self.gc.safepoint_pending() {
            return self.gc_safepoint_slow();
        }
        false
    }

    #[cold]
    #[inline(never)]
    fn gc_safepoint_slow(&mut self) -> bool {
        let exhausted = self.budget_tick();
        if !exhausted {
            self.gc_safepoint();
        }
        exhausted
    }

    /// Run `stmt` after a safepoint; a spent budget yields `Exit` instead.
    #[inline(always)]
    pub(crate) fn exec_statement_at_safepoint(
        &mut self,
        stmt: &Statement,
        env: &EnvRef,
    ) -> Completion {
        if self.gc_safepoint_poll() {
            return self.budget_exit();
        }
        self.exec_statement(stmt, env)
    }

    /// Count one step, latching `pending_exit` on exhaustion. A plain flag
    /// read when no budget is installed.
    #[inline(always)]
    pub(crate) fn budget_tick(&mut self) -> bool {
        if self.step_budget.is_active() && self.step_budget.tick() {
            self.pending_exit.get_or_insert(STEP_BUDGET_EXIT_CODE);
            return true;
        }
        false
    }

    /// The `Completion` a safepoint site returns once `gc_safepoint_poll`
    /// reported exhaustion.
    pub(crate) fn budget_exit(&self) -> Completion {
        Completion::Exit(self.pending_exit.unwrap_or(STEP_BUDGET_EXIT_CODE))
    }

    /// A safepoint with `v` rooted across it; true when the budget is spent.
    #[inline(always)]
    pub(crate) fn safepoint_rooted(&mut self, v: &JsValue) -> bool {
        self.gc_root_value(v);
        let exhausted = self.gc_safepoint_poll();
        self.gc_unroot_value(v);
        exhausted
    }

    pub(crate) fn run_budgeted_microtask(&mut self, task: scheduler::Microtask) -> Completion {
        if self.budget_tick() {
            return self.budget_exit();
        }
        self.run_microtask(task)
    }

    pub(crate) fn call_budgeted_timer(
        &mut self,
        callback: &JsValue,
        args: &[JsValue],
    ) -> Completion {
        if self.budget_tick() {
            return self.budget_exit();
        }
        self.call_function(callback, &JsValue::UNDEFINED, args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_exactly_limit_steps_then_sticks() {
        let mut b = StepBudget::default();
        b.set(Some(3));
        assert!(!b.tick() && !b.tick() && !b.tick());
        assert!(b.tick());
        assert!(b.tick());
        assert!(b.exhausted());
        assert_eq!(b.used(), 3);
    }

    #[test]
    fn zero_budget_exhausts_on_first_tick() {
        let mut b = StepBudget::default();
        b.set(Some(0));
        assert!(b.tick());
    }

    #[test]
    fn unset_is_inactive_and_set_resets() {
        let mut b = StepBudget::default();
        assert!(!b.is_active());
        b.set(Some(1));
        assert!(b.is_active());
        b.tick();
        b.tick();
        assert!(b.exhausted());
        b.set(None);
        assert!(!b.is_active() && !b.exhausted() && b.used() == 0);
    }
}
