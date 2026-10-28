//! `at async` frames of `--node` stack traces (issue #1368). V8 appends them
//! when a stack is captured inside a promise reaction job: it follows the
//! promise the job settles (the result promise of the async function an
//! `await` resumes, else the reaction's derived promise) through pending
//! promises that have exactly one reaction, and prints a frame for each async
//! function parked at an `await` on the way. Nothing here runs a getter, a
//! `then` or any script, and nothing runs before the hooks are installed.

use super::*;
use crate::interpreter::ModuleKey;

const FLAG_ASYNC: f64 = 4.0;
const MAX_HOPS: usize = 4096;

/// An async function or module body parked at an `await`.
pub(crate) struct ParkedAwait {
    pub(crate) func: u64,
    pub(crate) receiver: Option<JsValue>,
    pub(crate) called_with: bool,
    pub(crate) pos: u32,
    pub(crate) result: Option<u64>,
    pub(crate) module: Option<ModuleKey>,
}

impl Interpreter {
    fn parked_await(&self, id: u64) -> Option<ParkedAwait> {
        if let Some(parked) = self.parked_hosted_await(id) {
            return Some(parked);
        }
        let state = self.scheduler.async_function_state(id)?;
        Some(ParkedAwait {
            func: state.func_obj_id,
            receiver: state.func_env.borrow().get_this(),
            called_with: false,
            pos: state.await_pos,
            result: match state.resolvers {
                PromiseResolvers::Internal(promise) => Some(promise),
                PromiseResolvers::Functions { .. } => None,
            },
            module: state.module_path.clone(),
        })
    }

    pub(crate) fn node_note_await_pos(&mut self, async_id: u64, at: u32) {
        if !self.node_host.stack.installed {
            return;
        }
        if let Some(state) = self.scheduler.async_function_state_mut(async_id) {
            state.await_pos = at;
        }
    }

    /// Runs `job` as the settlement of `promise` for `at async` frames, as V8's
    /// module evaluation job is one of the promise `import()` returned.
    pub(crate) fn node_with_job_promise<T>(
        &mut self,
        promise: &JsValue,
        job: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let promise = promise
            .as_object_id()
            .filter(|_| self.node_host.stack.installed);
        let saved = std::mem::replace(&mut self.node_host.stack.job_promise, promise);
        let result = job(self);
        self.node_host.stack.job_promise = saved;
        result
    }

    /// Runs a reaction job with the promise it settles noted for the capture of
    /// any stack made while it runs.
    pub(crate) fn node_run_reaction_job(
        &mut self,
        reaction: PromiseReaction,
        argument: JsValue,
    ) -> Completion {
        if !self.node_host.stack.installed {
            return self.run_promise_reaction_job(reaction, argument);
        }
        let promise = match &reaction.target {
            ReactionTarget::AsyncResume(id) => self.parked_await(*id).and_then(|p| p.result),
            target => target.promise_id(),
        };
        let saved = std::mem::replace(&mut self.node_host.stack.job_promise, promise);
        let result = self.run_promise_reaction_job(reaction, argument);
        self.node_host.stack.job_promise = saved;
        result
    }

    fn async_awaiters(&self, room: usize) -> Vec<ParkedAwait> {
        let mut out = Vec::new();
        let mut next = self.node_host.stack.job_promise;
        for _ in 0..MAX_HOPS {
            let Some(promise) = next.take().filter(|_| out.len() < room) else {
                break;
            };
            let Some(object) = self.get_object(promise) else {
                break;
            };
            let target = match object.borrow().promise_data() {
                Some(data)
                    if matches!(data.state, PromiseState::Pending)
                        && data.fulfill_reactions.len() == 1 =>
                {
                    data.fulfill_reactions[0].target.clone()
                }
                _ => break,
            };
            match target {
                ReactionTarget::AsyncResume(id) => {
                    let Some(parked) = self.parked_await(id) else {
                        break;
                    };
                    next = parked.result;
                    out.push(parked);
                }
                other => next = other.promise_id(),
            }
        }
        out
    }

    /// The flat frame records of a capture: the live call stack, then, inside a
    /// reaction job, the `await`ing async functions.
    pub(super) fn capture_records(
        &mut self,
        skip_top: usize,
        until: Option<Until>,
        limit: usize,
    ) -> Vec<JsValue> {
        let raws = self.raw_frames(skip_top, until, limit);
        let mut record = Vec::with_capacity(raws.len() * RECORD_LEN);
        for raw in &raws {
            self.frame_record(raw, &mut record);
        }
        let skipped_all = until.is_some() && raws.is_empty();
        if self.node_host.stack.job_promise.is_some() && !skipped_all && raws.len() < limit {
            for parked in self.async_awaiters(limit - raws.len()) {
                self.async_record(parked, &mut record);
            }
        }
        record
    }

    fn async_record(&mut self, parked: ParkedAwait, record: &mut Vec<JsValue>) {
        if let Some(key) = &parked.module {
            let origin = self.node_host.stack.module_origins.get(key).cloned();
            let source = origin.map_or(-1.0, |origin| {
                self.node_host.stack.source_id(&origin) as f64
            });
            record.extend([
                JsValue::UNDEFINED,
                JsValue::UNDEFINED,
                JsValue::number(source),
                JsValue::number(f64::from(parked.pos.max(1))),
                JsValue::number(FLAG_TOPLEVEL + FLAG_ASYNC),
            ]);
            return;
        }
        let raw = RawFrame {
            func: parked.func,
            pos: parked.pos,
            receiver: parked.receiver,
            constructor: false,
            called_with: parked.called_with,
        };
        self.frame_record(&raw, record);
        let flags = record.pop().and_then(|v| v.as_number()).unwrap_or(0.0);
        record.push(JsValue::number(flags + FLAG_ASYNC));
    }
}
