//! Builtin frames of `--node` stack traces (issue #1482). V8 prints a frame
//! such as `at Array.map (<anonymous>)` for a builtin that is on the stack when
//! an error is made, whether the builtin called back into script or threw
//! itself. A native call does not push a `CallFrame` (a dozen readers assume
//! the top one is a function activation), so the builtins that are running are
//! kept on a list of their own, each with the number of `CallFrame`s below it,
//! and `raw_frames` interleaves the two. Nothing here runs before the stack
//! hooks are installed.

use super::*;

/// The `Function.prototype`, `Reflect` and global functions V8 runs without a
/// frame of their own (they tail call, or are the error constructors the
/// capture skips anyway).
const FRAMELESS_GLOBALS: [&str; 13] = [
    "eval",
    "structuredClone",
    "Array",
    "Function",
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

/// `String.prototype.replace` and friends tail call these on a RegExp, so
/// V8 shows no frame for them under another builtin.
const REGEXP_SYMBOL_METHODS: [&str; 5] = [
    "[Symbol.replace]",
    "[Symbol.match]",
    "[Symbol.matchAll]",
    "[Symbol.search]",
    "[Symbol.split]",
];

pub(crate) struct NativeFrame {
    func: u64,
    depth: usize,
    receiver: JsValue,
    construct: bool,
}

#[derive(Default)]
pub(crate) struct NativeFrames {
    frames: Vec<NativeFrame>,
    visible: Vec<bool>,
    nested_hidden: Vec<u64>,
}

impl NativeFrames {
    fn shows(&self, func: u64) -> bool {
        self.visible.get(func as usize).copied().unwrap_or(false)
    }
}

pub(super) const NOT_PUSHED: usize = usize::MAX;

impl Interpreter {
    fn own_function(&self, object: u64, key: &str) -> Option<u64> {
        self.get_object(object)?
            .borrow()
            .get_own_property(key)?
            .value?
            .as_object_id()
    }

    /// Notes which of the functions that exist before the host floor is built
    /// are builtins V8 shows in a trace: every named native, except the ones
    /// that have no frame there. Natives made later (promise resolvers, host
    /// plumbing) are never shown.
    pub(super) fn note_native_frame_intrinsics(&mut self) {
        let limit = self.objects.live_count() as u64;
        let mut frameless: Vec<u64> = Vec::new();
        for name in FRAMELESS_GLOBALS {
            frameless.extend(self.get_global_var(name).and_then(|v| v.as_object_id()));
        }
        let function = self
            .get_global_var("Function")
            .and_then(|v| v.as_object_id());
        let prototype = function.and_then(|f| self.own_function(f, "prototype"));
        let reflect = self
            .get_global_var("Reflect")
            .and_then(|v| v.as_object_id());
        for (object, keys) in [
            (prototype, ["call", "apply"]),
            (reflect, ["apply", "construct"]),
        ] {
            for key in keys {
                frameless.extend(object.and_then(|o| self.own_function(o, key)));
            }
        }
        let mut visible = vec![false; limit as usize];
        for id in 0..limit {
            let named = self.get_object(id).is_some_and(|obj| {
                matches!(
                    obj.borrow().callable.as_deref(),
                    Some(JsFunction::Native(name, _, _, _)) if !name.is_empty()
                )
            });
            visible[id as usize] = named && !frameless.contains(&id);
        }
        let nested_hidden = (0..limit)
            .filter(|&id| visible[id as usize])
            .filter(|&id| {
                self.own_data_string(id, "name")
                    .is_some_and(|name| REGEXP_SYMBOL_METHODS.contains(&name.as_str()))
            })
            .collect();
        self.node_host.stack.natives.nested_hidden = nested_hidden;
        self.node_host.stack.natives.visible = visible;
    }

    /// Puts the native function about to run on the frame list when it is one
    /// V8 shows. The returned mark goes to `node_leave_native`.
    #[inline]
    pub(crate) fn node_enter_native(
        &mut self,
        func: u64,
        receiver: &JsValue,
        construct: bool,
    ) -> usize {
        let depth = self.call_stack_frames.len();
        let stack = &mut self.node_host.stack;
        if !stack.installed || !stack.natives.shows(func) {
            return NOT_PUSHED;
        }
        let natives = &stack.natives;
        if natives.nested_hidden.contains(&func)
            && natives.frames.last().is_some_and(|f| f.depth == depth)
        {
            return NOT_PUSHED;
        }
        let mark = stack.natives.frames.len();
        stack.natives.frames.push(NativeFrame {
            func,
            depth,
            receiver: receiver.clone(),
            construct,
        });
        mark
    }

    #[inline]
    pub(crate) fn node_leave_native(&mut self, mark: usize) {
        if mark != NOT_PUSHED {
            self.node_host.stack.natives.frames.truncate(mark);
        }
    }

    /// The captured frames, innermost first: the activations on the call stack
    /// with the builtins that were running between them. A builtin sits above
    /// the `depth` activations that were live when it was called. The first
    /// `skip` activations, and the builtins above the last of them, are left
    /// out (what `skip_top` and `Until` ask to hide).
    pub(super) fn raw_frames(
        &self,
        skip_top: usize,
        until: Option<Until>,
        limit: usize,
    ) -> Vec<RawFrame> {
        let frames = &self.call_stack_frames;
        let natives = &self.node_host.stack.natives.frames;
        let mut out = Vec::new();
        let skip = skip_top + until.map_or(0, |until| self.frames_through(skip_top, until));
        let mut pending = natives.len();
        let mut native_frames_above = |out: &mut Vec<RawFrame>, n: usize, depth_floor: usize| {
            while pending > 0 && natives[pending - 1].depth >= depth_floor {
                pending -= 1;
                let native = &natives[pending];
                if n >= skip && out.len() < limit {
                    out.push(RawFrame {
                        func: native.func,
                        pos: 0,
                        receiver: Some(native.receiver.clone()),
                        constructor: native.construct,
                        called_with: false,
                        native: true,
                    });
                }
            }
        };
        for (n, i) in (0..frames.len()).rev().enumerate() {
            native_frames_above(&mut out, n, i + 1);
            if n < skip {
                continue;
            }
            let frame = &frames[i];
            if out.len() >= limit {
                break;
            }
            let id = frame.func_obj_id;
            if id == 0 || self.node_host.stack.hidden_frames.iter().any(|h| h.0 == id) {
                continue;
            }
            let pos = match frames.get(i + 1) {
                Some(callee) => callee.caller_pos,
                None => self.node_host.call_pos,
            };
            let arrow_this = self.node_arrow_receiver(i);
            let called_with = arrow_this.is_some();
            let (receiver, constructor) = match self.call_stack_envs.get(frame.env_index) {
                Some(_) if called_with => (arrow_this, false),
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
                called_with,
                native: false,
            });
        }
        native_frames_above(&mut out, frames.len(), 0);
        out
    }

    /// The record of a builtin frame: its name and receiver type, no source.
    pub(super) fn native_frame_record(&mut self, raw: &RawFrame, record: &mut Vec<JsValue>) {
        let name = self.own_data_string(raw.func, "name");
        let type_name = match &raw.receiver {
            Some(receiver) if !raw.constructor => self.receiver_type_name(receiver, &name),
            _ => None,
        };
        let mut flags = 0.0;
        if type_name.is_none() {
            flags += FLAG_TOPLEVEL;
        }
        if raw.constructor {
            flags += FLAG_CONSTRUCTOR;
        }
        let text = |s: Option<String>| {
            s.filter(|s| !s.is_empty())
                .map_or(JsValue::UNDEFINED, |s| string_value(&s))
        };
        record.push(text(name));
        record.push(text(type_name));
        record.push(JsValue::number(-1.0));
        record.push(JsValue::number(0.0));
        record.push(JsValue::number(flags));
    }
}
