//! Synchronous TCO suppression scopes; a suspension ends the scope.

use super::Interpreter;

impl Interpreter {
    /// Suppress tail calls for this synchronous extent when `suppress` is true.
    /// False preserves ambient suppression; it never enables tail calls.
    #[inline]
    pub(crate) fn with_tco_suppressed<R>(
        &mut self,
        suppress: bool,
        run: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.with_tco_depth(self.tco_suppress_depth + u32::from(suppress), run)
    }

    /// A user activation starts with no inherited suppression. Other tail-call
    /// eligibility checks still apply. Restore the caller on every returned result.
    #[inline]
    pub(crate) fn with_tco_reset<R>(&mut self, run: impl FnOnce(&mut Self) -> R) -> R {
        // The driver's marker describes its own body, not an ordinary callee.
        let state_machine = std::mem::replace(&mut self.in_state_machine, false);
        let result = self.with_tco_depth(0, run);
        self.in_state_machine = state_machine;
        result
    }

    /// Suppression only; strictness, host mode and expression position are separate.
    #[inline]
    pub(crate) fn tail_calls_suppressed(&self) -> bool {
        self.tco_suppress_depth != 0 || self.in_state_machine
    }

    #[inline]
    fn with_tco_depth<R>(&mut self, depth: u32, run: impl FnOnce(&mut Self) -> R) -> R {
        let caller = std::mem::replace(&mut self.tco_suppress_depth, depth);
        let result = run(self);
        self.tco_suppress_depth = caller;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Interpreter, types::Completion};
    use crate::types::JsValue;

    #[test]
    fn tail_call_scope_nesting_and_activation_reset() {
        let mut interp = Interpreter::new();
        assert!(!interp.tail_calls_suppressed());
        interp.with_tco_suppressed(false, |interp| {
            assert!(!interp.tail_calls_suppressed());
        });
        interp.with_tco_suppressed(true, |interp| {
            assert!(interp.tail_calls_suppressed());
            interp.with_tco_suppressed(true, |interp| {
                assert!(interp.tail_calls_suppressed());
            });
            assert!(interp.tail_calls_suppressed());
            interp.with_tco_suppressed(false, |interp| {
                assert!(interp.tail_calls_suppressed());
            });
            interp.with_tco_reset(|interp| {
                assert!(!interp.tail_calls_suppressed());
                interp.with_tco_reset(|interp| {
                    assert!(!interp.tail_calls_suppressed());
                });
                interp.with_tco_suppressed(true, |interp| {
                    assert!(interp.tail_calls_suppressed());
                });
                assert!(!interp.tail_calls_suppressed());
            });
            assert!(interp.tail_calls_suppressed());
        });
        assert!(!interp.tail_calls_suppressed());
    }

    #[test]
    fn tail_call_scope_restores_after_early_return_and_question_mark() {
        let mut interp = Interpreter::new();
        let result = interp.with_tco_suppressed(true, |interp| {
            assert!(interp.tail_calls_suppressed());
            let child = interp.with_tco_reset(|interp| -> Result<(), &str> {
                assert!(!interp.tail_calls_suppressed());
                let () = Err("child")?;
                unreachable!()
            });
            assert_eq!(child, Err("child"));
            assert!(interp.tail_calls_suppressed());
            if child.is_err() {
                return Err("parent");
            }
            Ok(17)
        });
        assert_eq!(result, Err("parent"));
        assert!(!interp.tail_calls_suppressed());
        assert_eq!(interp.with_tco_reset(|_| "unchanged"), "unchanged");
        assert!(!interp.tail_calls_suppressed());
    }

    #[test]
    fn tail_call_scope_isolates_the_state_machine_caller() {
        let mut interp = Interpreter::new();
        // The same entry state installed by all three state-machine drivers.
        interp.in_state_machine = true;
        assert!(interp.tail_calls_suppressed());
        for completion in [Completion::Empty, Completion::Throw(JsValue::number(29.0))] {
            let expected = format!("{completion:?}");
            let result = interp.with_tco_reset(|interp| {
                assert!(!interp.tail_calls_suppressed());
                interp.with_tco_suppressed(true, |interp| {
                    assert!(interp.tail_calls_suppressed());
                });
                assert!(!interp.tail_calls_suppressed());
                completion
            });
            assert_eq!(format!("{result:?}"), expected);
            assert!(interp.tail_calls_suppressed());
        }
    }

    #[test]
    fn tail_call_scope_forwards_completions_and_ends_at_suspension() {
        let mut interp = Interpreter::new();
        let payload = JsValue::number(23.0);
        for completion in [
            Completion::Normal(payload.clone()),
            Completion::Throw(payload.clone()),
            Completion::Return(payload.clone()),
            Completion::Yield(payload.clone()),
            Completion::Empty,
            Completion::Break(Some("loop".into()), Some(payload.clone())),
            Completion::Continue(None, None),
            Completion::TailCall {
                func: payload.clone(),
                this: JsValue::UNDEFINED,
                args: vec![payload.clone()],
            },
            Completion::Exit(7),
        ] {
            let expected = format!("{completion:?}");
            let result = interp.with_tco_suppressed(true, |interp| {
                assert!(interp.tail_calls_suppressed());
                interp.with_tco_reset(|interp| {
                    assert!(!interp.tail_calls_suppressed());
                    completion
                })
            });
            assert_eq!(format!("{result:?}"), expected);
            assert!(!interp.tail_calls_suppressed());
        }
        // A later resume enters independently; no suppression is parked.
        interp.with_tco_suppressed(true, |interp| {
            assert!(interp.tail_calls_suppressed());
        });
        assert!(!interp.tail_calls_suppressed());
    }
}
