//! Synchronous job lifetime and outermost ClearKeptObjects.

use super::*;

impl Interpreter {
    /// Run a synchronous job, releasing kept WeakRef targets only when the
    /// outermost job returns. Callers own input roots and queue checkpoints.
    /// All returned results pass through unchanged, including abrupt ones.
    pub(crate) fn with_job<R>(&mut self, body: impl FnOnce(&mut Self) -> R) -> R {
        self.job_depth += 1;
        let result = body(self);
        debug_assert!(self.job_depth > 0);
        self.job_depth -= 1;
        if self.job_depth == 0 {
            self.kept_alive.clear();
        }
        result
    }

    pub(crate) fn run(&mut self, program: &Program) -> Completion {
        let depth = self.gc_root_frame();
        let result = self.run_program(program);
        self.gc_assert_root_depth(depth, "a program run");
        result
    }

    fn run_program(&mut self, program: &Program) -> Completion {
        self.gc_safepoint();
        let result = self.with_job(|interp| {
            let result = match program.source_type {
                SourceType::Script => {
                    let global = interp.realm().global_env.clone();
                    if program.body_is_strict {
                        global.borrow_mut().strict = true;
                    }
                    interp.exec_framed(&program.body, &global)
                }
                SourceType::Module => interp.run_module(program, None),
            };
            // Latch a top-level host exit before leaving the job so draining
            // sees the terminal sink and skips queued work.
            if let Completion::Exit(code) = &result {
                interp.pending_exit = Some(*code);
            }
            result
        });
        self.drain_after_run(&result);
        result
    }

    pub(crate) fn run_with_path(&mut self, program: &Program, path: &Path) -> Completion {
        let depth = self.gc_root_frame();
        let result = self.run_program_with_path(program, path);
        self.gc_assert_root_depth(depth, "a program run");
        result
    }

    fn run_program_with_path(&mut self, program: &Program, path: &Path) -> Completion {
        self.gc_safepoint();
        match program.source_type {
            SourceType::Script => {
                self.with_module_key(Some(ModuleKey::for_file(path.to_path_buf())), |interp| {
                    let global = interp.realm().global_env.clone();
                    if program.body_is_strict {
                        global.borrow_mut().strict = true;
                    }
                    let r = interp.with_job(|interp| {
                        let r = interp.exec_script_body(&program.body, &global);
                        if let Completion::Exit(code) = &r {
                            interp.pending_exit = Some(*code);
                        }
                        r
                    });
                    // Async callbacks need the file's key for relative imports.
                    interp.drain_after_run(&r);
                    r
                })
            }
            SourceType::Module => {
                let module_key = ModuleKey::for_file(path.to_path_buf());
                let r = self.with_job(|interp| {
                    interp.node_esm_enter_main(&module_key);
                    let r = interp.run_module(program, Some(module_key.clone()));
                    if let Completion::Exit(code) = &r {
                        interp.pending_exit = Some(*code);
                    }
                    r
                });
                // Keep path set during microtask draining so async callbacks can use relative imports
                self.with_module_key(Some(module_key), |interp| interp.drain_after_run(&r));
                r
            }
        }
    }

    /// Runs one dequeued job with the values it keeps alive rooted for its
    /// duration. Once off the queue the job's own values are reachable from
    /// nowhere else.
    pub(crate) fn run_microtask(&mut self, task: scheduler::Microtask) -> Completion {
        use scheduler::Microtask;
        let frame = self.gc_root_frame();
        task.for_each_root(|value| self.gc_root_value(value));
        let rooted_depth = self.gc_root_frame();
        let result = self.with_job(|interp| match task {
            Microtask::Closure { job, .. } => job(interp),
            Microtask::Reaction { reaction, argument } => {
                interp.node_run_reaction_job(reaction, argument)
            }
            Microtask::ResolveThenable {
                promise_id,
                thenable,
                then_fn,
            } => interp.run_promise_resolve_thenable_job(promise_id, thenable, then_fn),
        });
        self.gc_assert_root_depth(rooted_depth, "a microtask job");
        self.gc_unroot_frame(frame);
        result
    }
}
