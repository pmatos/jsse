//! The stack frames of a `node -e` program (issue #1229). Node runs the text
//! as a script named `[eval]` that a wrapper script named `[eval]-wrapper`
//! reaches through its own internal modules, so an error made at the top level
//! reads `at [eval]:1:7`, `at runScriptInThisContext (node:internal/vm:...)`,
//! `at [eval]-wrapper:6:24` and so on, and a throw there has a source position
//! for the fatal-error report. The program still runs on the global
//! environment; native stand-in frames (the `vm` script mechanism) give it that
//! shape, with the literal positions of Node 26.8 for the internal ones, which
//! is exact because `process.version` is that release. Inert before the stack
//! hooks are installed.

use super::super::*;
use std::rc::Rc;

/// From the outermost frame in: name, file, and the line and column the frame
/// is at while the next one runs.
const CHAIN: [(&str, &str, u32, u32); 7] = [
    ("", "node:internal/main/eval_string", 71, 3),
    ("evalTypeScript", "node:internal/process/execution", 329, 3),
    ("evalFunction", "node:internal/process/execution", 317, 30),
    (
        "runScriptInContext",
        "node:internal/process/execution",
        483,
        60,
    ),
    ("", "[eval]-wrapper", 6, 24),
    ("", "node:internal/process/execution", 485, 12),
    ("runScriptInThisContext", "node:internal/vm", 219, 10),
];

/// The frames of `CHAIN` and the `[eval]` program's own.
const FRAMES: usize = CHAIN.len() + 1;

impl Interpreter {
    /// Remember the parse origin of the `-e` source, which its functions
    /// share, and name it `[eval]`; the next script run gets the frames.
    pub fn node_note_eval_origin(&mut self, origin: Rc<str>) {
        let stack = &mut self.node_host.stack;
        if !stack.installed {
            return;
        }
        stack.name_pending(&origin, "[eval]", 0, 0, 0);
        stack.eval_origin = Some(origin);
        stack.eval_armed = true;
    }

    pub(crate) fn node_push_eval_frames(&mut self, env: &EnvRef) -> bool {
        if !std::mem::take(&mut self.node_host.stack.eval_armed) {
            return false;
        }
        let Some(origin) = self.node_host.stack.eval_origin.clone() else {
            return false;
        };
        let mut pushed = 0;
        for (name, file, line, column) in CHAIN {
            let blanks = (line - 1) as usize;
            let text: Rc<str> = Rc::from("\n".repeat(blanks) + &" ".repeat(column as usize - 1));
            self.node_host.stack.name_pending(&text, file, 0, 0, 0);
            if !self.push_named_script_frame(&text, env, name) {
                break;
            }
            self.node_host.call_pos = (blanks + column as usize) as u32;
            pushed += 1;
        }
        if pushed == CHAIN.len() && self.push_script_frame(&origin, env) {
            return true;
        }
        for _ in 0..pushed {
            self.pop_script_frame();
        }
        false
    }

    /// `exec_script_body`, under the `-e` frames when they are armed.
    pub(crate) fn exec_framed(&mut self, body: &Body, env: &EnvRef) -> Completion {
        let framed = self.node_push_eval_frames(env);
        let result = self.exec_script_body(body, env);
        if framed {
            for _ in 0..FRAMES {
                self.pop_script_frame();
            }
        }
        result
    }
}
