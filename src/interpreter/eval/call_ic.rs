use super::*;
use crate::interpreter::ic::{CallIcKind, CallIcSlot};

/// Invoke an already evaluated call at an IC Site in the current Body.
///
/// Callers keep the callee, receiver and arguments rooted across this call.
/// Direct eval and tail-call production remain in the execution adapters.
impl Interpreter {
    pub(crate) fn call_at_site(
        &mut self,
        site_id: CallSiteId,
        callee: &JsValue,
        this_value: &JsValue,
        args: &[JsValue],
    ) -> Completion {
        let callee_id = if !site_id.is_unassigned() && self.with_scope_depth == 0 {
            callee.as_object_id()
        } else {
            None
        };
        let Some(callee_id) = callee_id else {
            return self.call_function(callee, this_value, args);
        };

        let probe_hit = matches!(
            self.call_slot(site_id),
            CallIcSlot::Mono { callee_obj_id, callee_shape_id, .. }
                if callee_id == callee_obj_id
                    && self.get_object(callee_id)
                        .is_some_and(|obj| obj.borrow().shape_id == callee_shape_id)
        );
        if probe_hit {
            self.call_ic_hit_count.set(self.call_ic_hit_count.get() + 1);
        } else {
            self.call_ic_slow_path_count
                .set(self.call_ic_slow_path_count.get() + 1);
        }

        // No arena or IC-store borrow crosses user code. The VM can borrow
        // its arguments directly from the still-rooted operand stack.
        let result = if probe_hit {
            self.call_function_ic_validated(callee, this_value, args)
        } else {
            self.call_function(callee, this_value, args)
        };
        if !probe_hit && matches!(result, Completion::Normal(_)) {
            // Nested calls may have updated this same Body's same site.
            // Merge against the restored caller's current slot, so an outer
            // miss cannot revive a recursively established Megamorphic site.
            let slot = self.call_slot(site_id);
            let observation = self.classify_for_call_ic(callee_id);
            let next = match (slot, observation) {
                (CallIcSlot::Megamorphic, _) => CallIcSlot::Megamorphic,
                (_, None) => CallIcSlot::Empty,
                (CallIcSlot::Empty, Some(slot)) => slot,
                (
                    CallIcSlot::Mono {
                        callee_obj_id: previous,
                        ..
                    },
                    Some(
                        slot @ CallIcSlot::Mono {
                            callee_obj_id: current,
                            ..
                        },
                    ),
                ) if previous == current => slot,
                (CallIcSlot::Mono { .. }, Some(_)) => CallIcSlot::Megamorphic,
            };
            self.set_call_slot(site_id, next);
        }
        result
    }

    /// Classify the callable's post-call shape. Proxies, wrapped and bound
    /// functions, and class constructors retain ordinary entry checks.
    fn classify_for_call_ic(&self, callee_obj_id: u64) -> Option<CallIcSlot> {
        let obj_rc = self.get_object(callee_obj_id)?;
        let obj = obj_rc.borrow();
        if obj.proxy().is_some()
            || obj.wrapped().is_some()
            || obj.bound().is_some()
            || obj.is_class_constructor()
        {
            return None;
        }
        let kind = match obj.callable.as_deref()? {
            JsFunction::Native(..) => CallIcKind::NativeFn,
            JsFunction::User { .. } => CallIcKind::UserFn,
        };
        Some(CallIcSlot::Mono {
            callee_obj_id,
            callee_shape_id: obj.shape_id,
            kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BodyIcInfo;

    const SITE: CallSiteId = CallSiteId(0, 0);

    fn with_site(test: impl FnOnce(&mut Interpreter)) {
        let body = Body {
            statements: Rc::new(vec![]),
            ic: BodyIcInfo {
                call_site_count: 1,
                prop_site_count: 0,
                assigned: true,
            },
        };
        Interpreter::new().with_ic_body(&body, |interp| interp.with_gc_root_scope(test));
    }

    fn native(
        interp: &mut Interpreter,
        invoke: impl Fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion + 'static,
    ) -> JsValue {
        let function = interp.create_function(JsFunction::native("test".to_string(), 0, invoke));
        interp.gc_root_value(&function);
        function
    }

    fn constant(interp: &mut Interpreter, value: f64) -> JsValue {
        native(interp, move |_, _, _| {
            Completion::Normal(JsValue::number(value))
        })
    }

    fn invoke(interp: &mut Interpreter, function: &JsValue) -> Completion {
        interp.call_at_site(SITE, function, &JsValue::UNDEFINED, &[])
    }

    fn assert_normal(result: Completion, expected: f64) {
        assert!(
            matches!(&result, Completion::Normal(value) if value.as_number() == Some(expected)),
            "expected Normal({expected}), got {result:?}"
        );
    }

    fn mono(interp: &mut Interpreter) -> (u64, u64) {
        match interp.call_slot(SITE) {
            CallIcSlot::Mono {
                callee_obj_id,
                callee_shape_id,
                kind: CallIcKind::NativeFn,
            } => (callee_obj_id, callee_shape_id),
            slot => panic!("expected native Mono, got {slot:?}"),
        }
    }

    fn counters(interp: &Interpreter) -> (u64, u64, u64) {
        (
            interp.call_ic_hit_count.get(),
            interp.call_ic_slow_path_count.get(),
            interp.call_ic_fast_dispatch_count.get(),
        )
    }

    #[test]
    fn cold_call_records_and_warm_call_uses_validated_dispatch() {
        with_site(|interp| {
            let function = constant(interp, 17.0);
            let id = function.as_object_id().unwrap();
            let shape = interp.get_object_cell_expect(id).borrow().shape_id;
            let before = counters(interp);

            assert_normal(invoke(interp, &function), 17.0);
            assert_eq!(mono(interp), (id, shape));
            assert_eq!(counters(interp), (before.0, before.1 + 1, before.2));

            assert_normal(invoke(interp, &function), 17.0);
            assert_eq!(mono(interp), (id, shape));
            assert_eq!(counters(interp), (before.0 + 1, before.1 + 1, before.2 + 1));
        });
    }

    #[test]
    fn changed_shape_misses_and_refreshes_the_same_callee() {
        with_site(|interp| {
            let function = constant(interp, 3.0);
            assert_normal(invoke(interp, &function), 3.0);
            let (id, old_shape) = mono(interp);
            let new_shape = crate::interpreter::types::fresh_shape_id();
            interp.get_object_cell_expect(id).borrow_mut().shape_id = new_shape;
            assert_ne!(old_shape, new_shape);
            let before = counters(interp);

            assert_normal(invoke(interp, &function), 3.0);
            assert_eq!(mono(interp), (id, new_shape));
            assert_eq!(counters(interp), (before.0, before.1 + 1, before.2));
            assert_normal(invoke(interp, &function), 3.0);
            assert_eq!(counters(interp), (before.0 + 1, before.1 + 1, before.2 + 1));
        });
    }

    #[test]
    fn successful_miss_classifies_the_callees_post_call_shape() {
        with_site(|interp| {
            let changed = std::cell::Cell::new(false);
            let function = native(interp, move |interp, this, _| {
                if !changed.replace(true) {
                    interp
                        .get_object_cell_expect(this.as_object_id().unwrap())
                        .borrow_mut()
                        .shape_id = crate::interpreter::types::fresh_shape_id();
                }
                Completion::Normal(JsValue::number(3.0))
            });
            let id = function.as_object_id().unwrap();
            let old_shape = interp.get_object_cell_expect(id).borrow().shape_id;
            assert_normal(interp.call_at_site(SITE, &function, &function, &[]), 3.0);
            let new_shape = interp.get_object_cell_expect(id).borrow().shape_id;
            assert_ne!(old_shape, new_shape);
            assert_eq!(mono(interp), (id, new_shape));
            let before = counters(interp);
            assert_normal(interp.call_at_site(SITE, &function, &function, &[]), 3.0);
            assert_eq!(counters(interp), (before.0 + 1, before.1, before.2 + 1));
        });
    }

    #[test]
    fn distinct_callees_make_megamorphic_terminal_even_for_uncacheable_calls() {
        with_site(|interp| {
            let first = constant(interp, 1.0);
            let second = constant(interp, 2.0);
            let handler = interp.create_object_id();
            interp.gc_root_id(handler);
            let proxy = interp.create_object_id();
            interp.gc_root_id(proxy);
            interp.get_object_cell_expect(proxy).borrow_mut().kind = ObjectKind::Proxy(Box::new(
                ProxyData::active(first.as_object_id().unwrap(), handler),
            ));
            let proxy = JsValue::object(proxy);

            // A successful uncacheable call resets a monomorphic site.
            assert_normal(invoke(interp, &first), 1.0);
            assert_normal(invoke(interp, &proxy), 1.0);
            assert!(matches!(interp.call_slot(SITE), CallIcSlot::Empty));

            assert_normal(invoke(interp, &first), 1.0);
            assert_normal(invoke(interp, &second), 2.0);
            assert!(matches!(interp.call_slot(SITE), CallIcSlot::Megamorphic));
            let before = counters(interp);
            assert_normal(invoke(interp, &first), 1.0);
            assert_normal(invoke(interp, &proxy), 1.0);
            assert!(matches!(interp.call_slot(SITE), CallIcSlot::Megamorphic));
            assert_eq!(counters(interp), (before.0, before.1 + 2, before.2));
        });
    }

    #[test]
    fn non_normal_misses_preserve_the_slot_and_completion() {
        with_site(|interp| {
            let warm = constant(interp, 1.0);
            assert_normal(invoke(interp, &warm), 1.0);
            let original = mono(interp);
            let before = counters(interp);

            for case in 0..5 {
                let function = native(interp, move |_, _, _| match case {
                    0 => Completion::Throw(JsValue::number(23.0)),
                    1 => Completion::Return(JsValue::number(23.0)),
                    2 => Completion::Yield(JsValue::number(23.0)),
                    3 => Completion::Exit(23),
                    _ => Completion::Empty,
                });
                let result = invoke(interp, &function);
                let preserved = match (case, &result) {
                    (0, Completion::Throw(value))
                    | (1, Completion::Return(value))
                    | (2, Completion::Yield(value)) => value.as_number() == Some(23.0),
                    (3, Completion::Exit(23)) | (4, Completion::Empty) => true,
                    _ => false,
                };
                assert!(preserved, "case {case} changed completion: {result:?}");
                assert_eq!(mono(interp), original);
            }
            assert_eq!(counters(interp), (before.0, before.1 + 5, before.2));
        });
    }

    #[test]
    fn hit_that_throws_still_counts_as_a_hit_and_preserves_the_slot() {
        with_site(|interp| {
            let calls = Rc::new(std::cell::Cell::new(0));
            let observed = calls.clone();
            let function = native(interp, move |_, _, _| {
                observed.set(observed.get() + 1);
                if observed.get() == 1 {
                    Completion::Normal(JsValue::number(1.0))
                } else {
                    Completion::Throw(JsValue::number(2.0))
                }
            });
            assert_normal(invoke(interp, &function), 1.0);
            let original = mono(interp);
            let before = counters(interp);

            assert!(
                matches!(invoke(interp, &function), Completion::Throw(value) if value.as_number() == Some(2.0))
            );
            assert_eq!(mono(interp), original);
            assert_eq!(counters(interp), (before.0 + 1, before.1, before.2 + 1));
        });
    }

    #[test]
    fn bypasses_do_not_probe_record_or_count() {
        with_site(|interp| {
            let warm = constant(interp, 1.0);
            let bypassed = constant(interp, 2.0);
            assert_normal(invoke(interp, &warm), 1.0);
            let original = mono(interp);
            let before = counters(interp);

            assert_normal(
                interp.call_at_site(CallSiteId::UNASSIGNED, &bypassed, &JsValue::UNDEFINED, &[]),
                2.0,
            );
            interp.with_scope_depth += 1;
            let result = invoke(interp, &bypassed);
            interp.with_scope_depth -= 1;
            assert_normal(result, 2.0);
            assert!(matches!(
                invoke(interp, &JsValue::number(4.0)),
                Completion::Throw(_)
            ));
            assert_eq!(mono(interp), original);
            assert_eq!(counters(interp), before);
        });

        // An unassigned site can be invoked without any active Body at all.
        Interpreter::new().with_gc_root_scope(|interp| {
            let function = constant(interp, 7.0);
            assert_normal(
                interp.call_at_site(CallSiteId::UNASSIGNED, &function, &JsValue::UNDEFINED, &[]),
                7.0,
            );
        });
    }

    #[test]
    fn both_dispatches_forward_the_receiver_and_arguments_unchanged() {
        with_site(|interp| {
            let receiver = JsValue::object(interp.create_object_id());
            let argument = JsValue::object(interp.create_object_id());
            interp.gc_root_value(&receiver);
            interp.gc_root_value(&argument);
            let receiver_id = receiver.as_object_id();
            let argument_id = argument.as_object_id();
            let function = native(interp, move |_, this, args| {
                assert_eq!(this.as_object_id(), receiver_id);
                assert_eq!(args.len(), 2);
                assert_eq!(args[0].as_object_id(), argument_id);
                assert_eq!(args[1].as_number(), Some(29.0));
                Completion::Normal(args[0].clone())
            });
            for _ in 0..2 {
                let result = interp.call_at_site(
                    SITE,
                    &function,
                    &receiver,
                    &[argument.clone(), JsValue::number(29.0)],
                );
                assert!(
                    matches!(result, Completion::Normal(value) if value.as_object_id() == argument_id)
                );
            }
        });
    }

    #[test]
    fn reentrant_calls_merge_the_post_call_slot_without_reviving_megamorphic() {
        with_site(|interp| {
            let nested_first = constant(interp, 1.0);
            let nested_second = constant(interp, 2.0);
            let outer = native(interp, move |interp, _, _| {
                assert_normal(invoke(interp, &nested_first), 1.0);
                assert_normal(invoke(interp, &nested_second), 2.0);
                assert!(matches!(interp.call_slot(SITE), CallIcSlot::Megamorphic));
                Completion::Normal(JsValue::number(3.0))
            });
            let before = counters(interp);

            assert_normal(invoke(interp, &outer), 3.0);
            assert!(matches!(interp.call_slot(SITE), CallIcSlot::Megamorphic));
            assert_eq!(counters(interp), (before.0, before.1 + 3, before.2));
        });
    }
}
