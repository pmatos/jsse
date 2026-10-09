use super::super::*;

fn format_host_args(args: &[JsValue]) -> String {
    let parts: Vec<String> = args.iter().map(|v| format!("{v}")).collect();
    parts.join(" ")
}

fn console_stdout_write(
    _interp: &mut Interpreter,
    _this: &JsValue,
    args: &[JsValue],
) -> Completion {
    println!("{}", format_host_args(args));
    Completion::Normal(JsValue::UNDEFINED)
}

fn console_stderr_write(
    _interp: &mut Interpreter,
    _this: &JsValue,
    args: &[JsValue],
) -> Completion {
    use std::io::Write as _;
    let line = format!("{}\n", format_host_args(args));
    let _ = std::io::stderr().write_all(line.as_bytes());
    Completion::Normal(JsValue::UNDEFINED)
}

/// Shared body of `setTimeout` / `setInterval`: validate the callback, coerce
/// the delay, and arm a timer. Extra arguments are passed to the callback when
/// it fires. Returns the timer id.
fn arm_timer(interp: &mut Interpreter, args: &[JsValue], repeating: bool) -> Completion {
    let name = if repeating {
        "setInterval"
    } else {
        "setTimeout"
    };
    let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
    if !interp.is_callable(&callback) {
        return Completion::Throw(
            interp.create_type_error(&format!("{name} callback must be callable")),
        );
    }

    let delay_val = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
    let delay = match interp.to_integer_or_infinity_value(&delay_val) {
        Ok(n) => n,
        Err(e) => return Completion::Throw(e),
    };
    // A non-positive delay (NaN included, which coerces to 0) fires on the next
    // turn. An infinite one saturates to a deadline hundreds of millions of
    // years out, so the timer never fires but still holds the event loop open —
    // the same outcome as the thread that used to sleep for u64::MAX ms.
    let delay_ms = if delay <= 0.0 {
        0
    } else {
        delay.min(u64::MAX as f64) as u64
    };

    let timer_args: Vec<JsValue> = args.iter().skip(2).cloned().collect();
    let id = interp.scheduler.add_timer(
        callback,
        timer_args,
        std::time::Duration::from_millis(delay_ms),
        repeating,
    );
    Completion::Normal(JsValue::number(id as f64))
}

/// Shared body of `clearTimeout` / `clearInterval`. An id that is unknown, or
/// not a timer id at all, is a no-op rather than an error — as in Node.
fn disarm_timer(interp: &mut Interpreter, args: &[JsValue]) -> Completion {
    // Ids come from a monotonic counter, so a value outside the exactly
    // representable integer range cannot name a live timer.
    const MAX_SAFE_INTEGER: f64 = 9007199254740991.0;
    let id_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
    // Node resolves an id only from a number or a string and ignores anything
    // else without coercing it. Coercing here instead would run an object's
    // `valueOf` as an observable side effect Node does not have.
    if id_val.as_number().is_none() && id_val.as_string().is_none() {
        return Completion::Normal(JsValue::UNDEFINED);
    }
    if let Ok(n) = interp.to_number_value(&id_val)
        && (1.0..=MAX_SAFE_INTEGER).contains(&n)
        && n.fract() == 0.0
    {
        interp.scheduler.clear_timer(n as u64);
    }
    Completion::Normal(JsValue::UNDEFINED)
}

impl Interpreter {
    pub(super) fn setup_host_globals(&mut self) {
        let console_id = self.create_object_id();
        {
            let log_fn = self.create_function(JsFunction::native(
                "log".to_string(),
                0,
                console_stdout_write,
            ));
            self.get_object_cell_expect(console_id)
                .borrow_mut()
                .insert_builtin("log".to_string(), log_fn);

            let assert_fn = self.create_function(JsFunction::native(
                "assert".to_string(),
                0,
                |interp, _this, args| {
                    use std::io::Write as _;
                    if !args.first().is_some_and(|v| interp.to_boolean_val(v)) {
                        let line = if args.len() > 1 {
                            format!("Assertion failed: {}\n", format_host_args(&args[1..]))
                        } else {
                            "Assertion failed\n".to_string()
                        };
                        let _ = std::io::stderr().write_all(line.as_bytes());
                    }
                    Completion::Normal(JsValue::UNDEFINED)
                },
            ));
            self.get_object_cell_expect(console_id)
                .borrow_mut()
                .insert_builtin("assert".to_string(), assert_fn);

            let error_fn = self.create_function(JsFunction::native(
                "error".to_string(),
                0,
                console_stderr_write,
            ));
            self.get_object_cell_expect(console_id)
                .borrow_mut()
                .insert_builtin("error".to_string(), error_fn);

            let warn_fn = self.create_function(JsFunction::native(
                "warn".to_string(),
                0,
                console_stderr_write,
            ));
            self.get_object_cell_expect(console_id)
                .borrow_mut()
                .insert_builtin("warn".to_string(), warn_fn);

            let info_fn = self.create_function(JsFunction::native(
                "info".to_string(),
                0,
                console_stdout_write,
            ));
            self.get_object_cell_expect(console_id)
                .borrow_mut()
                .insert_builtin("info".to_string(), info_fn);

            let debug_fn = self.create_function(JsFunction::native(
                "debug".to_string(),
                0,
                console_stdout_write,
            ));
            self.get_object_cell_expect(console_id)
                .borrow_mut()
                .insert_builtin("debug".to_string(), debug_fn);
        }
        let console_val = JsValue::object(console_id);
        self.realm()
            .global_env
            .borrow_mut()
            .declare("console", BindingKind::Const);
        self.realm()
            .global_env
            .borrow_mut()
            .initialize_binding("console", console_val);

        // print global (needed by test262 async harness doneprintHandle.js)
        {
            let print_fn = self.create_function(JsFunction::native(
                "print".to_string(),
                1,
                |_interp, _this, args| {
                    println!("{}", format_host_args(args));
                    Completion::Normal(JsValue::UNDEFINED)
                },
            ));
            self.realm()
                .global_env
                .borrow_mut()
                .declare("print", BindingKind::Var);
            let env = self.realm().global_env.clone();
            let _ = self.env_set(&env, "print", print_fn);
        }

        // Host timers. Not ECMAScript intrinsics, but the test262 atomics
        // harness needs setTimeout, and real-world libraries need the whole
        // family. They are serviced on the event loop rather than by a thread
        // per call (issue #254).
        self.register_global_fn(
            "setTimeout",
            BindingKind::Var,
            JsFunction::native("setTimeout".to_string(), 2, |interp, _this, args| {
                arm_timer(interp, args, false)
            }),
        );
        self.register_global_fn(
            "setInterval",
            BindingKind::Var,
            JsFunction::native("setInterval".to_string(), 2, |interp, _this, args| {
                arm_timer(interp, args, true)
            }),
        );
        self.register_global_fn(
            "clearTimeout",
            BindingKind::Var,
            JsFunction::native("clearTimeout".to_string(), 1, |interp, _this, args| {
                disarm_timer(interp, args)
            }),
        );
        self.register_global_fn(
            "clearInterval",
            BindingKind::Var,
            JsFunction::native("clearInterval".to_string(), 1, |interp, _this, args| {
                disarm_timer(interp, args)
            }),
        );
    }
}
