//! V8-compatible stack capture for Node host mode (issue #1228).
//!
//! The prelude (`scripts/node-v8-error.js`) installs the `stack` accessor pair
//! with `__host_stack_install`; from then on every Error construction and
//! `Error.captureStackTrace` records the live call stack here, without running
//! script. Frames are plain data in a hidden slot of the error, so the
//! collector traces them like any private element; file, line and column are
//! resolved only when the prelude formats the trace. Nothing in this file runs
//! unless the prelude installed the hooks, so default mode is untouched. See
//! `docs/adr/2026-10-26-1000-node-host-v8-errors.md`.

use super::super::*;
pub(crate) use super::node_host_stack_sources::NodeStackState;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) const HOOK_ERROR: PrivateId = PrivateId(u64::MAX - 2);
pub(super) const HOOK_GETTER: PrivateId = PrivateId(u64::MAX - 3);
pub(super) const HOOK_SETTER: PrivateId = PrivateId(u64::MAX - 4);
const FRAMES: PrivateId = PrivateId(u64::MAX - 5);

const FLAG_CONSTRUCTOR: f64 = 1.0;
const FLAG_TOPLEVEL: f64 = 2.0;
const RECORD_LEN: usize = 5;

static CALL_POSITIONS: AtomicBool = AtomicBool::new(false);

/// Whether the bytecode compiler should record call positions. Process-wide
/// because chunks are compiled lazily and shared; set once by the prelude.
pub(crate) fn call_positions_enabled() -> bool {
    CALL_POSITIONS.load(Ordering::Relaxed)
}

/// Where a capture starts: after the innermost frame of this function
/// (`captureStackTrace`'s argument; absent means nothing is captured), or,
/// for an error built by `new`, after the constructors that made it.
#[derive(Clone, Copy)]
pub(crate) enum Until {
    Seen(u64),
    Constructor(u64),
}

struct RawFrame {
    func: u64,
    pos: u32,
    receiver: Option<JsValue>,
    constructor: bool,
}

fn string_value(text: &str) -> JsValue {
    JsValue::string(JsString::from_str(text))
}

impl Interpreter {
    /// Called by the CLI for the main script, so its frames carry its path.
    /// The CommonJS bootstrap stub stands in for the file and stays unnamed.
    pub fn note_script_source(&mut self, path: &std::path::Path, source: &str, module: bool) {
        if self.node_host.stack.installed && source != Self::node_main_bootstrap() {
            let path = path.to_string_lossy();
            let name: Rc<str> = Rc::from(if module {
                super::node_host_stack_sources::file_url(&path)
            } else {
                path.into_owned()
            });
            self.node_host
                .stack
                .scripts
                .push((name, source.to_string()));
        }
    }

    fn own_data_string(&self, id: u64, key: &str) -> Option<String> {
        let obj = self.get_object(id)?;
        let value = obj.borrow().get_own_property(key)?.value?;
        value.as_string().map(|s| s.to_string())
    }

    fn receiver_type_name(&self, receiver: &JsValue) -> Option<String> {
        let mut id = receiver.as_object_id()?;
        if self.node_host.vm.is_context_global(id)
            || self.realm().global_object == Some(id)
            || self.realms[0].global_object == Some(id)
        {
            return None;
        }
        if self.is_callable(receiver) {
            return Some(
                self.own_data_string(id, "name")
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "Function".to_string()),
            );
        }
        for _ in 0..64 {
            let obj = self.get_object(id)?;
            let (ctor, proto) = {
                let b = obj.borrow();
                if b.is_proxy() {
                    break;
                }
                let ctor = b
                    .get_own_property("constructor")
                    .and_then(|d| d.value)
                    .and_then(|v| v.as_object_id());
                (ctor, b.prototype_id)
            };
            if let Some(ctor) = ctor {
                if let Some(name) = self.own_data_string(ctor, "name").filter(|n| !n.is_empty()) {
                    return Some(name);
                }
                break;
            }
            id = proto?;
        }
        Some("Object".to_string())
    }

    /// How many frames, counted from the first one after `skip_top`, end with
    /// `target`'s own (V8's SKIP_UNTIL_SEEN). A derived constructor without a
    /// body has no frame here, so when `target` is not on the stack the
    /// leading frames of its superclass constructors go instead.
    fn frames_through(&self, skip_top: usize, until: Until) -> usize {
        let (Until::Seen(target) | Until::Constructor(target)) = until;
        let ids = self
            .call_stack_frames
            .iter()
            .rev()
            .skip(skip_top)
            .map(|f| f.func_obj_id);
        if let Some(at) = ids.clone().position(|id| id == target) {
            return at + 1;
        }
        if matches!(until, Until::Seen(_)) {
            return usize::MAX / 2;
        }
        let mut ancestors = Vec::new();
        let mut next = matches!(until, Until::Constructor(_)).then_some(target);
        while let Some(id) = next.filter(|_| ancestors.len() < 64) {
            ancestors.push(id);
            next = self.get_object(id).and_then(|o| o.borrow().prototype_id);
        }
        ids.take_while(|id| ancestors.contains(id)).count()
    }

    fn raw_frames(&self, skip_top: usize, until: Option<Until>, limit: usize) -> Vec<RawFrame> {
        let frames = &self.call_stack_frames;
        let mut out = Vec::new();
        let skip = skip_top + until.map_or(0, |until| self.frames_through(skip_top, until));
        for (n, i) in (0..frames.len()).rev().enumerate() {
            if n < skip {
                continue;
            }
            let frame = &frames[i];
            if out.len() >= limit {
                break;
            }
            if frame.func_obj_id == 0 {
                continue;
            }
            let pos = match frames.get(i + 1) {
                Some(callee) => callee.caller_pos,
                None => self.node_host.call_pos,
            };
            let (receiver, constructor) = match self.call_stack_envs.get(frame.env_index) {
                Some(env) if !frame.is_eval => {
                    let env = env.borrow();
                    (env.get_this(), env.get_new_target().is_object())
                }
                _ => (None, false),
            };
            out.push(RawFrame {
                func: frame.func_obj_id,
                pos,
                receiver,
                constructor,
            });
        }
        out
    }

    fn frame_record(&mut self, raw: &RawFrame, record: &mut Vec<JsValue>) {
        let mut name = None;
        let mut origin = None;
        let mut arrow = false;
        let mut module_file = None;
        if let Some(obj) = self.get_object(raw.func)
            && let Some(JsFunction::User {
                name: declared,
                source_text,
                is_arrow,
                closure,
                ..
            }) = obj.borrow().callable.as_deref()
        {
            name = declared.clone();
            arrow = *is_arrow;
            origin = source_text.as_ref().map(|t| {
                let (source, start) = t.origin();
                (source.clone(), start)
            });
            if self.node_host.esm.is_active() {
                module_file = Environment::find_module_path(closure)
                    .and_then(|key| key.file_path().map(|p| p.to_string_lossy().into_owned()));
            }
        }
        if let (Some(file), Some((source, _))) = (&module_file, &origin) {
            self.node_host.stack.name_module_source(source, file);
        }
        if origin.is_none()
            && let Some(source) = self.node_host.stack.script_frame_source(raw.func)
        {
            origin = Some((source, 0));
        }
        if let Some(own) = self.own_data_string(raw.func, "name") {
            name = Some(own);
        }
        let type_name = match (&raw.receiver, arrow) {
            (Some(receiver), false) => self.receiver_type_name(receiver),
            _ => None,
        };
        if !arrow
            && name.as_deref().is_none_or(str::is_empty)
            && let Some(receiver) = &raw.receiver
        {
            name = self.method_name_of(receiver, raw.func);
        }
        let (source_id, pos) = match origin {
            Some((source, start)) => {
                let id = self.node_host.stack.source_id(&source);
                let pos = if raw.pos != 0 {
                    raw.pos
                } else {
                    start as u32 + 1
                };
                (id as f64, f64::from(pos))
            }
            None => (-1.0, 0.0),
        };
        let mut flags = 0.0;
        if type_name.is_none() {
            flags += FLAG_TOPLEVEL;
        }
        let type_name = type_name.filter(|_| !raw.constructor);
        if raw.constructor {
            flags += FLAG_CONSTRUCTOR;
        }
        let text = |s: Option<String>| {
            s.filter(|s| !s.is_empty())
                .map_or(JsValue::UNDEFINED, |s| string_value(&s))
        };
        record.push(text(name));
        record.push(text(type_name));
        record.push(JsValue::number(source_id));
        record.push(JsValue::number(pos));
        record.push(JsValue::number(flags));
    }

    fn stack_hook(&self, key: PrivateId) -> Option<JsValue> {
        let global = self.realm().global_object?;
        self.get_object(global)?.borrow().hidden_slot(key)
    }

    fn stack_trace_limit(&self) -> Option<usize> {
        let ctor = self.stack_hook(HOOK_ERROR)?.as_object_id()?;
        let n = self
            .get_object(ctor)?
            .borrow()
            .get_own_property("stackTraceLimit")?
            .value?
            .as_number()?;
        Some(if n.is_nan() { 0 } else { n.max(0.0) as usize })
    }

    /// Record the current stack on `target` and give it the lazy `stack`
    /// accessor. `skip_top` drops the innermost prelude frames, `until` drops
    /// everything up to and including that function (`captureStackTrace`'s
    /// constructor argument). Does nothing before the prelude installed the
    /// hooks or when `Error.stackTraceLimit` is not a number.
    pub(crate) fn node_capture_stack(
        &mut self,
        target: u64,
        skip_top: usize,
        until: Option<Until>,
    ) {
        if !self.node_host.stack.installed {
            return;
        }
        let (Some(getter), Some(setter)) =
            (self.stack_hook(HOOK_GETTER), self.stack_hook(HOOK_SETTER))
        else {
            return;
        };
        let Some(handle) = self.get_object_cell(target).cloned() else {
            return;
        };
        {
            let b = handle.borrow();
            let own = b.get_own_property("stack");
            if b.is_proxy() || own.map_or(!b.extensible, |d| d.configurable == Some(false)) {
                return;
            }
        }
        let frames = match self.stack_trace_limit() {
            Some(limit) => {
                let raws = self.raw_frames(skip_top, until, limit);
                let mut record = Vec::with_capacity(raws.len() * RECORD_LEN);
                for raw in &raws {
                    self.frame_record(raw, &mut record);
                }
                self.create_array(record)
            }
            None => JsValue::UNDEFINED,
        };
        {
            let mut b = handle.borrow_mut();
            b.set_hidden_slot(FRAMES, frames.clone());
            b.insert_property(
                "stack".to_string(),
                PropertyDescriptor::accessor(
                    Some(getter.clone()),
                    Some(setter.clone()),
                    false,
                    true,
                ),
            );
        }
        for value in [&frames, &getter, &setter] {
            self.gc_write_barrier_value(&handle, value);
        }
    }

    pub(super) fn setup_node_host_stack(&mut self) {
        let install = self.create_function(JsFunction::native(
            "__host_stack_install".to_string(),
            3,
            |interp, _this, args| {
                let Some(global) = interp.realm().global_object else {
                    return Completion::Normal(JsValue::UNDEFINED);
                };
                let Some(handle) = interp.get_object_cell(global).cloned() else {
                    return Completion::Normal(JsValue::UNDEFINED);
                };
                let hooks = [HOOK_ERROR, HOOK_GETTER, HOOK_SETTER];
                for (key, value) in hooks.into_iter().zip(args) {
                    handle.borrow_mut().set_hidden_slot(key, value.clone());
                    interp.gc_write_barrier_value(&handle, value);
                }
                interp.node_host.stack.installed = true;
                CALL_POSITIONS.store(true, Ordering::Relaxed);
                Completion::Normal(JsValue::UNDEFINED)
            },
        ));
        self.install_host_global("__host_stack_install", install);

        let capture = self.create_function(JsFunction::native(
            "__host_stack_capture".to_string(),
            2,
            |interp, _this, args| {
                let until = args.get(1).and_then(JsValue::as_object_id).map(Until::Seen);
                if let Some(target) = args.first().and_then(JsValue::as_object_id) {
                    interp.node_capture_stack(target, 1, until);
                }
                Completion::Normal(JsValue::UNDEFINED)
            },
        ));
        self.install_host_global("__host_stack_capture", capture);

        let frames = self.create_function(JsFunction::native(
            "__host_stack_frames".to_string(),
            1,
            |interp, _this, args| {
                let slot = args
                    .first()
                    .and_then(JsValue::as_object_id)
                    .and_then(|id| interp.get_object(id))
                    .and_then(|obj| obj.borrow().hidden_slot(FRAMES));
                Completion::Normal(slot.unwrap_or(JsValue::UNDEFINED))
            },
        ));
        self.install_host_global("__host_stack_frames", frames);

        let locate = self.create_function(JsFunction::native(
            "__host_stack_locate".to_string(),
            2,
            |interp, _this, args| {
                let number = |i: usize| args.get(i).and_then(JsValue::as_number).unwrap_or(-1.0);
                let id = number(0);
                let pos = number(1);
                if id < 0.0 || pos < 1.0 {
                    return Completion::Normal(JsValue::UNDEFINED);
                }
                let Some((file, line, column)) =
                    interp.node_host.stack.locate(id as usize, pos as u32)
                else {
                    return Completion::Normal(JsValue::UNDEFINED);
                };
                let file = file.map_or(JsValue::UNDEFINED, |f| string_value(&f));
                Completion::Normal(interp.create_array(vec![
                    file,
                    JsValue::number(line as f64),
                    JsValue::number(column as f64),
                ]))
            },
        ));
        self.install_host_global("__host_stack_locate", locate);

        let name_source = self.create_function(JsFunction::native(
            "__host_stack_name_source".to_string(),
            3,
            |interp, _this, args| {
                let Some(func) = args.first().and_then(JsValue::as_object_id) else {
                    return Completion::Normal(JsValue::UNDEFINED);
                };
                let name = match args.get(1) {
                    Some(v) if v.is_string() => v.as_string().map(|s| s.to_string()),
                    _ => None,
                };
                let delta = args.get(2).and_then(JsValue::as_number).unwrap_or(0.0) as i32;
                let source = interp.get_object(func).and_then(|obj| {
                    match obj.borrow().callable.as_deref() {
                        Some(JsFunction::User {
                            source_text: Some(t),
                            ..
                        }) => Some(t.origin().0.clone()),
                        _ => None,
                    }
                });
                if let (Some(source), Some(name)) = (source, name) {
                    let id = interp.node_host.stack.source_id(&source);
                    let entry = &mut interp.node_host.stack.sources[id];
                    entry.name = Some(Rc::from(name.as_str()));
                    entry.line_delta = delta;
                }
                Completion::Normal(JsValue::UNDEFINED)
            },
        ));
        self.install_host_global("__host_stack_name_source", name_source);
    }
}

impl Interpreter {
    /// The position of the call about to run, moved into the new frame so the
    /// frame below it knows where it is; the callee starts with none.
    #[inline]
    pub(crate) fn take_call_pos(&mut self) -> u32 {
        std::mem::take(&mut self.node_host.call_pos)
    }

    #[inline]
    pub(crate) fn pop_call_frame(&mut self) {
        if let Some(frame) = self.call_stack_frames.pop() {
            self.node_host.call_pos = frame.caller_pos;
        }
    }
}

const ERROR_CONSTRUCTORS: [&str; 9] = [
    "Error",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "AggregateError",
    "SuppressedError",
];

impl Interpreter {
    /// Capture a stack on an error the engine just created, and return it.
    pub(crate) fn node_capture_error(&mut self, id: u64) -> JsValue {
        self.node_capture_stack(id, 0, None);
        JsValue::object(id)
    }

    /// Wrap the realm's error constructors so an instance made through them
    /// records its stack once the hooks are installed. The wrapper changes
    /// nothing observable before that.
    pub(super) fn wrap_error_constructors_for_stack(&mut self) {
        for name in ERROR_CONSTRUCTORS {
            let Some(ctor) = self.get_global_var(name).and_then(|v| v.as_object_id()) else {
                continue;
            };
            let Some(handle) = self.get_object_cell(ctor).cloned() else {
                continue;
            };
            let mut object = handle.borrow_mut();
            let Some(JsFunction::Native(_, _, native, _)) = object.callable.as_deref_mut() else {
                continue;
            };
            let inner = native.clone();
            *native = Rc::new(move |interp, this, args| {
                let until = interp.new_target.as_ref().and_then(JsValue::as_object_id);
                let result = inner(interp, this, args);
                if interp.node_host.stack.installed
                    && let Completion::Normal(value) = &result
                    && let Some(error) = value.as_object_id()
                {
                    let until = until.filter(|&n| n != ctor).map(Until::Constructor);
                    interp.node_capture_stack(error, 0, until);
                }
                result
            });
        }
    }
}

impl Interpreter {
    /// Gives a hosted async body its own frame, so `--node` stack traces show
    /// it; the frame is visible only once the prelude installed the hooks.
    /// The body's environment is the innermost one on `call_stack_envs`.
    pub(crate) fn push_async_stack_frame(&mut self, func_obj_id: u64) -> bool {
        if !self.node_host.stack.installed {
            return false;
        }
        let caller_pos = self.take_call_pos();
        self.call_stack_frames.push(CallFrame {
            func_obj_id,
            arguments: CallFrameArguments::None,
            is_eval: false,
            caller_pos,
            env_index: self.call_stack_envs.len() - 1,
        });
        true
    }
}
