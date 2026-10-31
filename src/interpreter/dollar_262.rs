use super::*;

impl Interpreter {
    pub(crate) fn create_dollar_262(&mut self, realm_id: usize) -> JsValue {
        let dollar_262_id = self.create_object_id();

        // $262.detachArrayBuffer
        let detach_fn = self.create_function(JsFunction::native(
            "detachArrayBuffer".to_string(),
            1,
            |interp, _this, args| {
                let buf = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                interp.detach_arraybuffer(&buf)
            },
        ));
        self.get_object_cell_expect(dollar_262_id)
            .borrow_mut()
            .insert_builtin("detachArrayBuffer".to_string(), detach_fn);

        // $262.gc
        let gc_fn = self.create_function(JsFunction::native(
            "gc".to_string(),
            0,
            |interp, _this, _args| {
                interp.gc.request();
                interp.gc_safepoint();
                Completion::Normal(JsValue::UNDEFINED)
            },
        ));
        self.get_object_cell_expect(dollar_262_id)
            .borrow_mut()
            .insert_builtin("gc".to_string(), gc_fn);

        // $262.clearKeptObjects
        let clear_kept_fn = self.create_function(JsFunction::native(
            "clearKeptObjects".to_string(),
            0,
            |interp, _this, _args| {
                interp.kept_alive.clear();
                Completion::Normal(JsValue::UNDEFINED)
            },
        ));
        self.get_object_cell_expect(dollar_262_id)
            .borrow_mut()
            .insert_builtin("clearKeptObjects".to_string(), clear_kept_fn);

        // $262.global — reference to the realm's global object
        let global_env = self.realms[realm_id].global_env.clone();
        if let Some(go_id) = global_env.borrow().global_object_id {
            self.get_object_cell_expect(dollar_262_id)
                .borrow_mut()
                .insert_builtin("global".to_string(), JsValue::object(go_id));
        }

        // $262.createRealm
        let create_realm_fn = self.create_function(JsFunction::native(
            "createRealm".to_string(),
            0,
            |interp, _this, _args| {
                let new_realm_id = interp.create_new_realm();
                let new_dollar_262 = interp.create_dollar_262(new_realm_id);
                Completion::Normal(new_dollar_262)
            },
        ));
        self.get_object_cell_expect(dollar_262_id)
            .borrow_mut()
            .insert_builtin("createRealm".to_string(), create_realm_fn);

        // $262.evalScript — parse and execute code in this $262's realm
        let eval_realm_id = realm_id;
        let eval_script_fn = self.create_function(JsFunction::native(
            "evalScript".to_string(),
            1,
            move |interp, _this, args| {
                let code_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let (code, stand_ins) = if let Some(s) = (code_val).as_string() {
                    crate::interpreter::builtins::regexp::encode_source_units(s.as_units())
                } else {
                    match interp.to_string_lossy(&code_val) {
                        Ok(s) => (s, false),
                        Err(e) => return Completion::Throw(e),
                    }
                };
                let mut p = match crate::parser::Parser::for_source(&code, stand_ins) {
                    Ok(p) => p,
                    Err(_) => {
                        return Completion::Throw(
                            interp.create_error("SyntaxError", "Invalid eval source"),
                        );
                    }
                };
                let program = match p.parse_program() {
                    Ok(prog) => prog,
                    Err(e) => {
                        return Completion::Throw(interp.create_error("SyntaxError", &e.message));
                    }
                };
                let result = interp.with_realm(Some(eval_realm_id), |interp| interp.run(&program));
                match result {
                    Completion::Normal(v) => Completion::Normal(v),
                    Completion::Empty => Completion::Normal(JsValue::UNDEFINED),
                    other => other,
                }
            },
        ));
        self.get_object_cell_expect(dollar_262_id)
            .borrow_mut()
            .insert_builtin("evalScript".to_string(), eval_script_fn);

        // $262.IsHTMLDDA — B.3.6 [[IsHTMLDDA]] internal slot
        let htmldda_obj_id = self.create_object_id();
        self.get_object_cell_expect(htmldda_obj_id)
            .borrow_mut()
            .callable = Some(Box::new(JsFunction::native(
            "".to_string(),
            0,
            |_interp, _this, _args| Completion::Normal(JsValue::NULL),
        )));
        self.get_object_cell_expect(htmldda_obj_id)
            .borrow_mut()
            .is_htmldda = true;
        let htmldda_val = JsValue::object(htmldda_obj_id);
        self.get_object_cell_expect(dollar_262_id)
            .borrow_mut()
            .insert_builtin("IsHTMLDDA".to_string(), htmldda_val);

        // $262.agent
        let agent_obj_id = self.create_object_id();

        // $262.agent.start(script)
        let _reports_clone = self.agent_reports.clone();
        let start_fn = self.create_function(JsFunction::native(
            "start".to_string(),
            1,
            move |interp, _this, args| {
                let script_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let script = match interp.to_string_lossy(&script_val) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };

                let (tx, rx) = std::sync::mpsc::channel::<AgentBroadcastMsg>();
                interp.agent_broadcast_txs.push(tx);

                let reports = interp.agent_reports.clone();
                let handle = std::thread::spawn(move || {
                    let mut agent_interp = Interpreter::new();
                    agent_interp.is_agent_thread = true;
                    agent_interp.can_block = true;
                    agent_interp.agent_reports = reports;
                    agent_interp.agent_broadcast_rx = Some(rx);

                    // Set up agent-side $262.agent on the global
                    setup_agent_side_262(&mut agent_interp);

                    let mut p = match crate::parser::Parser::new(&script) {
                        Ok(p) => p,
                        Err(_) => return,
                    };
                    let program = match p.parse_program() {
                        Ok(prog) => prog,
                        Err(_) => return,
                    };
                    let _ = agent_interp.run(&program);
                    agent_interp.drain_microtasks_blocking();
                });
                interp.agent_handles.push(handle);

                Completion::Normal(JsValue::UNDEFINED)
            },
        ));
        self.get_object_cell_expect(agent_obj_id)
            .borrow_mut()
            .insert_builtin("start".to_string(), start_fn);

        // $262.agent.broadcast(sab)
        let broadcast_fn = self.create_function(JsFunction::native(
            "broadcast".to_string(),
            1,
            |interp, _this, args| {
                let sab_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if let Some(o) = sab_val
                    .as_object_id()
                    .map(|id| crate::types::JsObject { id })
                    && let Some(cell) = interp.get_object_cell(o.id)
                {
                    let sab_shared = cell.borrow().sab_shared().cloned();
                    if let Some(inner) = sab_shared {
                        for tx in &interp.agent_broadcast_txs {
                            let _ = tx.send(AgentBroadcastMsg {
                                sab_shared: inner.clone(),
                            });
                        }
                    }
                }
                Completion::Normal(JsValue::UNDEFINED)
            },
        ));
        self.get_object_cell_expect(agent_obj_id)
            .borrow_mut()
            .insert_builtin("broadcast".to_string(), broadcast_fn);

        // $262.agent.getReport()
        let get_report_fn = self.create_function(JsFunction::native(
            "getReport".to_string(),
            0,
            |interp, _this, _args| {
                let (ref reports_lock, _) = *interp.agent_reports;
                let mut reports = reports_lock.lock().unwrap();
                if let Some(report) = reports.pop_front() {
                    Completion::Normal(JsValue::string(JsString::from_str(&report)))
                } else {
                    Completion::Normal(JsValue::NULL)
                }
            },
        ));
        self.get_object_cell_expect(agent_obj_id)
            .borrow_mut()
            .insert_builtin("getReport".to_string(), get_report_fn);

        // $262.agent.getReportAsync()
        let get_report_async_fn = self.create_function(JsFunction::native(
            "getReportAsync".to_string(),
            0,
            |interp, _this, _args| {
                let (resolve_fn, _reject_fn, promise_val) = interp.create_promise_parts();

                let immediate_report = {
                    let (ref reports_lock, _) = *interp.agent_reports;
                    reports_lock.lock().unwrap().pop_front()
                };
                if let Some(report) = immediate_report {
                    let report_val = JsValue::string(JsString::from_str(&report));
                    let _ = interp.call_function(&resolve_fn, &JsValue::UNDEFINED, &[report_val]);
                    return Completion::Normal(promise_val);
                }

                interp.scheduler.incr_pending_async_jobs();
                let reports = interp.agent_reports.clone();
                let resolve = resolve_fn;
                let pending = interp.agent_async_completions.clone();
                let pending_jobs = interp.scheduler.pending_async_jobs_handle();
                let pending_promise_ids = interp.scheduler.pending_async_promise_ids_handle();
                let promise_id = if let Some(o) = (promise_val)
                    .as_object_id()
                    .map(|id| crate::types::JsObject { id })
                {
                    o.id
                } else {
                    0
                };
                if promise_id != 0 {
                    pending_promise_ids.lock().unwrap().insert(promise_id);
                }

                std::thread::spawn(move || {
                    let report = {
                        let (ref reports_lock, ref reports_cvar) = *reports;
                        let mut queue = reports_lock.lock().unwrap();
                        loop {
                            if let Some(report) = queue.pop_front() {
                                break report;
                            }
                            queue = reports_cvar.wait(queue).unwrap();
                        }
                    };
                    let report_val = JsValue::string(JsString::from_str(&report));
                    let (ref mtx, ref completion_cvar) = *pending;
                    mtx.lock()
                        .unwrap()
                        .push(Box::new(move |interp: &mut Interpreter| {
                            let _ =
                                interp.call_function(&resolve, &JsValue::UNDEFINED, &[report_val]);
                            if promise_id != 0 {
                                pending_promise_ids.lock().unwrap().remove(&promise_id);
                            }
                            pending_jobs.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                        }));
                    completion_cvar.notify_one();
                });

                Completion::Normal(promise_val)
            },
        ));
        self.get_object_cell_expect(agent_obj_id)
            .borrow_mut()
            .insert_builtin("getReportAsync".to_string(), get_report_async_fn);

        // $262.agent.sleep(ms)
        let sleep_fn = self.create_function(JsFunction::native(
            "sleep".to_string(),
            1,
            |interp, _this, args| {
                let ms_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let ms = match interp.to_number_value(&ms_val) {
                    Ok(n) => n.max(0.0) as u64,
                    Err(e) => return Completion::Throw(e),
                };
                std::thread::sleep(std::time::Duration::from_millis(ms));
                Completion::Normal(JsValue::UNDEFINED)
            },
        ));
        self.get_object_cell_expect(agent_obj_id)
            .borrow_mut()
            .insert_builtin("sleep".to_string(), sleep_fn);

        // $262.agent.monotonicNow()
        let start_time = std::time::Instant::now();
        let monotonic_fn = self.create_function(JsFunction::native(
            "monotonicNow".to_string(),
            0,
            move |_interp, _this, _args| {
                let elapsed = start_time.elapsed().as_millis() as f64;
                Completion::Normal(JsValue::number(elapsed))
            },
        ));
        self.get_object_cell_expect(agent_obj_id)
            .borrow_mut()
            .insert_builtin("monotonicNow".to_string(), monotonic_fn);

        // $262.agent.leaving()
        let leaving_fn = self.create_function(JsFunction::native(
            "leaving".to_string(),
            0,
            |_interp, _this, _args| Completion::Normal(JsValue::UNDEFINED),
        ));
        self.get_object_cell_expect(agent_obj_id)
            .borrow_mut()
            .insert_builtin("leaving".to_string(), leaving_fn);

        let agent_val = JsValue::object(agent_obj_id);
        self.get_object_cell_expect(dollar_262_id)
            .borrow_mut()
            .insert_builtin("agent".to_string(), agent_val);

        // $262.AbstractModuleSource — §28.1.1.1 (source-phase imports proposal)
        let ams_fn = self.abstract_module_source_constructor(realm_id);
        self.get_object_cell_expect(dollar_262_id)
            .borrow_mut()
            .insert_builtin("AbstractModuleSource".to_string(), ams_fn);

        JsValue::object(dollar_262_id)
    }
}
