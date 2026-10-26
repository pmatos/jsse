//! Event-loop floor for Node host mode (issue #1229). The prelude's tick and
//! immediate queues, timers and `process` lifecycle events are built on these;
//! the loop itself is `interpreter/event_loop/node_event_loop.rs`.

use super::super::*;

fn arg(args: &[JsValue], index: usize) -> JsValue {
    args.get(index).cloned().unwrap_or(JsValue::UNDEFINED)
}

impl Interpreter {
    fn install_loop_fn(
        &mut self,
        name: &str,
        arity: usize,
        body: impl Fn(&mut Interpreter, &[JsValue]) -> Completion + 'static,
    ) {
        let function = self.create_function(JsFunction::native(
            name.to_string(),
            arity,
            move |interp, _this, args| body(interp, args),
        ));
        self.install_host_global(name, function);
    }

    pub(super) fn setup_node_host_loop(&mut self) {
        // __host_loop_install(tick, uncaught, rejections, lifecycle)
        self.install_loop_fn("__host_loop_install", 4, |interp, args| {
            interp.scheduler.node_loop.install(
                arg(args, 0),
                arg(args, 1),
                arg(args, 2),
                arg(args, 3),
            );
            Completion::Normal(JsValue::UNDEFINED)
        });

        // __host_loop_set_immediate(processImmediate)
        self.install_loop_fn("__host_loop_set_immediate", 1, |interp, args| {
            interp
                .scheduler
                .node_loop
                .set_immediate_callback(arg(args, 0));
            Completion::Normal(JsValue::UNDEFINED)
        });

        // __host_tick_scheduled(bool): the nextTick queue is (non-)empty
        self.install_loop_fn("__host_tick_scheduled", 1, |interp, args| {
            interp.scheduler.node_loop.tick_scheduled = interp.to_boolean_val(&arg(args, 0));
            Completion::Normal(JsValue::UNDEFINED)
        });

        // __host_run_microtasks(): drain promise jobs only
        self.install_loop_fn("__host_run_microtasks", 0, |interp, _| {
            interp.drain_microtasks();
            exit_or_undefined(interp)
        });

        // __host_checkpoint(): ticks, promise jobs and unhandled rejections
        self.install_loop_fn("__host_checkpoint", 0, |interp, _| {
            interp.node_checkpoint();
            exit_or_undefined(interp)
        });

        // __host_immediate_info(count, refCount)
        self.install_loop_fn("__host_immediate_info", 2, |interp, args| {
            interp.scheduler.node_loop.immediate_count = count(&arg(args, 0));
            interp.scheduler.node_loop.immediate_refed = count(&arg(args, 1));
            Completion::Normal(JsValue::UNDEFINED)
        });

        // __host_timer_ref(id, bool): an unref'd timer does not keep the loop alive
        self.install_loop_fn("__host_timer_ref", 2, |interp, args| {
            if let Some(id) = arg(args, 0).as_number() {
                let refed = interp.to_boolean_val(&arg(args, 1));
                interp.scheduler.set_timer_ref(id as u64, refed);
            }
            Completion::Normal(JsValue::UNDEFINED)
        });
    }
}

fn count(value: &JsValue) -> u32 {
    value.as_number().map_or(0, |n| n.max(0.0) as u32)
}

fn exit_or_undefined(interp: &Interpreter) -> Completion {
    match interp.pending_exit {
        Some(code) => Completion::Exit(code),
        None => Completion::Normal(JsValue::UNDEFINED),
    }
}
