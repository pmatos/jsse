//! Stand-in stack frames for ES module top level (issue #1229). A module body
//! runs with no function of its own, so an error made there had a stack with
//! no frame (`[Error: m]`) and the fatal-error report no position. While the
//! hooks are installed the body, and each resumption of a top-level `await`,
//! runs under the same kind of native stand-in frame as a `vm` script, named
//! by the module's `file:` URL. Inert before the hooks are installed.

use super::super::*;
use std::rc::Rc;

impl Interpreter {
    /// Remember the parse origin of `key`'s source, which the frames of its
    /// functions already share, and name it after the module's file.
    pub(crate) fn node_note_module_origin(&mut self, key: &ModuleKey, origin: Rc<str>) {
        if !self.node_host.stack.installed {
            return;
        }
        if let Some(path) = key.file_path() {
            self.node_host
                .stack
                .name_module_source(&origin, &path.to_string_lossy());
        }
        self.node_host
            .stack
            .module_origins
            .insert(key.clone(), origin);
    }

    /// The same for the main ES module, which the CLI parses itself.
    pub fn node_note_main_module_origin(&mut self, path: &std::path::Path, origin: Rc<str>) {
        self.node_note_module_origin(&ModuleKey::for_file(path.to_path_buf()), origin);
    }

    /// Push the top-level frame of module `key`, whose scope is `env`. The
    /// caller pops it with `node_pop_module_frame` when this returns true.
    /// A module that finishes evaluates its importers inside its own frame;
    /// that frame is hidden while theirs runs.
    pub(crate) fn node_push_module_frame(&mut self, key: &ModuleKey, env: &EnvRef) -> bool {
        let Some(origin) = self.node_host.stack.module_origins.get(key).cloned() else {
            return false;
        };
        if !self.push_script_frame(&origin, env) {
            return false;
        }
        let id = self.call_stack_frames.last().map_or(0, |f| f.func_obj_id);
        let stack = &mut self.node_host.stack;
        let hidden = stack.module_frames.iter().map(|&hidden| (hidden, id));
        stack.hidden_frames.extend(hidden);
        stack.module_frames.push(id);
        true
    }

    /// The same for the top-level `await` state machine `async_id`, if it is a
    /// module's.
    pub(crate) fn node_push_async_module_frame(&mut self, async_id: u64) -> bool {
        if !self.node_host.stack.installed {
            return false;
        }
        let Some((key, env)) = self
            .scheduler
            .async_function_state_mut(async_id)
            .and_then(|state| Some((state.module_path.clone()?, state.func_env.clone())))
        else {
            return false;
        };
        self.node_push_module_frame(&key, &env)
    }

    pub(crate) fn node_pop_module_frame(&mut self) {
        self.pop_script_frame();
        let stack = &mut self.node_host.stack;
        let Some(id) = stack.module_frames.pop() else {
            return;
        };
        while stack.hidden_frames.last().is_some_and(|h| h.1 == id) {
            stack.hidden_frames.pop();
        }
    }
}
