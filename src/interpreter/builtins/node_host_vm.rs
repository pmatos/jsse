//! The `vm` binding for Node host mode (issue #1238). One hidden global,
//! `__host_vm`, holds the primitives `node-lib/internal/vm/contextify.js`
//! builds Node's `contextify` binding from: new realms, scripts compiled once
//! and run in any realm with Script semantics, and `compileFunction`.
//! `node-builtin-loader.js` captures the object, deletes the global and seeds it
//! as the private module `internal/host/vm`. Nothing here is installed without
//! `--node`. See `docs/adr/2026-10-26-2300-node-host-vm.md`.

use super::super::*;
use super::node_host_stack::{HOOK_ERROR, HOOK_GETTER, HOOK_SETTER};
use crate::parser::Parser;
use std::collections::HashMap;
use std::rc::Rc;

/// Properties a fresh realm gets from jsse that a V8 context does not have.
const NOT_IN_V8_CONTEXT: [&str; 10] = [
    "$262",
    "print",
    "setTimeout",
    "setInterval",
    "clearTimeout",
    "clearInterval",
    "structuredClone",
    "btoa",
    "atob",
    "ShadowRealm",
];

struct Compiled {
    program: Program,
    origin: Rc<str>,
}

#[derive(Default)]
pub(crate) struct VmState {
    scripts: HashMap<u32, Rc<Compiled>>,
    next_id: u32,
    /// Global object of each context realm -> its realm index.
    globals: HashMap<u64, usize>,
}

impl VmState {
    pub(super) fn is_context_global(&self, id: u64) -> bool {
        self.globals.contains_key(&id)
    }
}

struct Failure {
    message: String,
    span: (usize, usize),
    text: String,
}

fn js_str(s: &str) -> JsValue {
    JsValue::string(JsString::from_str(s))
}

fn done(v: JsValue) -> Completion {
    Completion::Normal(v)
}

fn type_error(interp: &mut Interpreter, message: &str) -> Completion {
    Completion::Throw(interp.create_type_error(message))
}

fn string_arg(interp: &mut Interpreter, args: &[JsValue], i: usize) -> Result<String, Completion> {
    let v = args.get(i).unwrap_or(JsValue::undefined_ref());
    interp.to_string_lossy(v).map_err(Completion::Throw)
}

fn int_arg(args: &[JsValue], i: usize) -> i32 {
    args.get(i)
        .and_then(JsValue::as_number)
        .map_or(0, |n| n as i32)
}

fn parse(source: &JsValue, interp: &mut Interpreter) -> Result<(Program, Rc<str>), Failure> {
    let (code, stand_ins) = match source.as_string() {
        Some(s) => crate::interpreter::builtins::regexp::encode_source_units(s.as_units()),
        None => (interp.to_string_lossy(source).unwrap_or_default(), false),
    };
    let failure = |message: String, span: (usize, usize), text: &str| Failure {
        message,
        span,
        text: text.to_string(),
    };
    let mut parser =
        Parser::for_source(&code, stand_ins).map_err(|e| failure(e.message, (0, 0), &code))?;
    match parser.parse_program() {
        Ok(program) => Ok((program, parser.source_origin())),
        Err(e) => Err(failure(e.message, parser.failure_span(), &code)),
    }
}

fn is_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `[message, line, lineText, column, width]` for a script that did not parse,
/// the facts Node prints in the `file:line` header above a SyntaxError stack.
/// `skip_lines` leading lines of the text are synthetic wrapper.
fn failure_value(interp: &mut Interpreter, f: &Failure, line_offset: i32, skip: usize) -> JsValue {
    let text = f.text.as_str();
    let mut start = f.span.0.min(text.len());
    while !text.is_char_boundary(start) {
        start -= 1;
    }
    let (mut line, mut line_start) = (0usize, 0usize);
    for (at, c) in text.char_indices() {
        if at >= start {
            break;
        }
        if c == '\r' && text[at + 1..].starts_with('\n') {
            continue;
        }
        if is_terminator(c) {
            line += 1;
            line_start = at + c.len_utf8();
        }
    }
    let mut line_end = text[line_start..]
        .find(is_terminator)
        .map_or(text.len(), |p| line_start + p);
    let mut end = f.span.1.clamp(start, line_end);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let closing = text.rfind('\n').map_or(0, |p| p + 1);
    if skip > 0 && line_start >= closing && closing > 0 {
        // Ran off the end of the body into the synthetic `})`: V8 reports the
        // end of the body's last line.
        line_end = closing - 1;
        line_start = text[..line_end].rfind('\n').map_or(0, |p| p + 1);
        line = line.saturating_sub(1);
        (start, end) = (line_end, line_end);
    }
    let units = |s: &str| s.encode_utf16().count() as f64;
    let shown = (line.saturating_sub(skip) as i64 + 1 + i64::from(line_offset)).max(1);
    let items = vec![
        js_str(&f.message),
        JsValue::number(shown as f64),
        js_str(&text[line_start..line_end]),
        JsValue::number(units(&text[line_start..start])),
        JsValue::number(units(&text[start..end])),
    ];
    interp.create_array(items)
}

fn realm_of(interp: &Interpreter, global: &JsValue) -> Option<usize> {
    let Some(id) = global.as_object_id() else {
        return Some(interp.current_realm_id);
    };
    interp.node_host.vm.globals.get(&id).copied().or_else(|| {
        interp
            .realms
            .iter()
            .position(|r| r.global_object == Some(id))
    })
}

impl Interpreter {
    fn push_script_frame(&mut self, origin: &Rc<str>, env: &EnvRef) -> bool {
        if !self.node_host.stack.installed {
            return false;
        }
        let stand_in = self.create_function(JsFunction::native(String::new(), 0, |_, _, _| {
            Completion::Normal(JsValue::UNDEFINED)
        }));
        let Some(func_obj_id) = stand_in.as_object_id() else {
            return false;
        };
        self.node_host
            .stack
            .script_frames
            .push((func_obj_id, origin.clone()));
        let caller_pos = self.take_call_pos();
        self.call_stack_envs.push(env.clone());
        self.call_stack_frames.push(CallFrame {
            func_obj_id,
            arguments: CallFrameArguments::None,
            is_eval: true,
            caller_pos,
            env_index: self.call_stack_envs.len() - 1,
        });
        true
    }

    /// Give `target`, the global object of a new realm, the hooks `from_realm`
    /// installed, so errors made there record stacks too; `error` is that
    /// realm's own `Error`, which `stackTraceLimit` is read from.
    fn copy_stack_hooks(&mut self, from_realm: usize, target: u64, error: JsValue) {
        if !self.node_host.stack.installed {
            return;
        }
        let source = self.realms[from_realm]
            .global_object
            .and_then(|id| self.get_object(id));
        let Some(source) = source else { return };
        let Some(handle) = self.get_object_cell(target).cloned() else {
            return;
        };
        let hooks = [
            (HOOK_ERROR, Some(error)),
            (HOOK_GETTER, source.borrow().hidden_slot(HOOK_GETTER)),
            (HOOK_SETTER, source.borrow().hidden_slot(HOOK_SETTER)),
        ];
        for (key, value) in hooks {
            if let Some(value) = value {
                handle.borrow_mut().set_hidden_slot(key, value.clone());
                self.gc_write_barrier_value(&handle, &value);
            }
        }
    }

    fn pop_script_frame(&mut self) {
        self.pop_call_frame();
        self.call_stack_envs.pop();
        self.node_host.stack.script_frames.pop();
    }
}

fn new_context(interp: &mut Interpreter, _this: &JsValue, _args: &[JsValue]) -> Completion {
    let realm = interp.create_new_realm();
    let Some(global) = interp.realms[realm].global_object else {
        return type_error(interp, "vm: the new realm has no global object");
    };
    for name in NOT_IN_V8_CONTEXT {
        interp
            .get_object_cell_expect(global)
            .borrow_mut()
            .remove_property(name);
        interp.realms[realm]
            .global_env
            .borrow_mut()
            .bindings
            .remove(name);
    }
    interp.node_host.vm.globals.insert(global, realm);
    done(JsValue::object(global))
}

fn prepare(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let Some(global) = args.first().and_then(JsValue::as_object_id) else {
        return type_error(interp, "vm: prepare needs a global object");
    };
    let error = interp
        .get_object_cell_expect(global)
        .borrow()
        .get_property_value("Error");
    if let Some(error) = error {
        interp.copy_stack_hooks(interp.current_realm_id, global, error);
    }
    done(JsValue::UNDEFINED)
}

/// Drop the declarative `Var` bindings a context's global environment keeps
/// beside the global object's own properties (implicit globals, function
/// declarations), so the properties alone answer for those names and a sync
/// that replaces one cannot be shadowed by a stale binding.
fn settle(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let Some(realm) = realm_of(interp, args.first().unwrap_or(JsValue::undefined_ref())) else {
        return type_error(interp, "vm: not the global object of a context");
    };
    let Some(global) = interp.realms[realm]
        .global_object
        .and_then(|id| interp.get_object_cell(id).cloned())
    else {
        return done(JsValue::UNDEFINED);
    };
    let env = interp.realms[realm].global_env.clone();
    let mirrored: Vec<String> = {
        let e = env.borrow();
        (0..)
            .map_while(|i| e.bindings.entry_at(i))
            .filter(|(name, b)| {
                b.kind == BindingKind::Var && global.borrow().has_own_property(name.as_str())
            })
            .map(|(name, _)| name.as_str().to_string())
            .collect()
    };
    for name in &mirrored {
        env.borrow_mut().bindings.remove(name.as_str());
    }
    done(JsValue::UNDEFINED)
}

fn compile(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let source = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
    let filename = match string_arg(interp, args, 1) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let (line_offset, column_offset) = (int_arg(args, 2), int_arg(args, 3));
    match parse(&source, interp) {
        Err(f) => done(failure_value(interp, &f, line_offset, 0)),
        Ok((program, origin)) => {
            interp
                .node_host
                .stack
                .name_pending(&origin, &filename, line_offset, column_offset, 0);
            let id = interp.node_host.vm.next_id;
            interp.node_host.vm.next_id += 1;
            interp
                .node_host
                .vm
                .scripts
                .insert(id, Rc::new(Compiled { program, origin }));
            done(JsValue::number(f64::from(id)))
        }
    }
}

fn run(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let id = int_arg(args, 1) as u32;
    let Some(compiled) = interp.node_host.vm.scripts.get(&id).cloned() else {
        return type_error(interp, "vm: the script was released");
    };
    let Some(realm) = realm_of(interp, args.first().unwrap_or(JsValue::undefined_ref())) else {
        return type_error(interp, "vm: not the global object of a context");
    };
    let caller_realm = interp.current_realm_id;
    interp.current_realm_id = realm;
    let env = interp.realm().global_env.clone();
    let strict = std::mem::replace(
        &mut env.borrow_mut().strict,
        compiled.program.body_is_strict,
    );
    let framed = interp.push_script_frame(&compiled.origin, &env);
    let result = interp.exec_script_body(&compiled.program.body, &env);
    if framed {
        interp.pop_script_frame();
    }
    env.borrow_mut().strict = strict;
    interp.current_realm_id = caller_realm;
    match result {
        Completion::Empty => done(JsValue::UNDEFINED),
        other => other,
    }
}

fn release(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    interp
        .node_host
        .vm
        .scripts
        .remove(&(int_arg(args, 0) as u32));
    done(JsValue::UNDEFINED)
}

/// `compileFunction(wrapperSource, filename, lineOffset, columnOffset, global,
/// ...extensions)`: `wrapperSource` is `(function (params) {\nbody\n})`.
fn compile_function(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let source = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
    let filename = match string_arg(interp, args, 1) {
        Ok(s) => s,
        Err(c) => return c,
    };
    let (line_offset, column_offset) = (int_arg(args, 2), int_arg(args, 3));
    let Some(realm) = realm_of(interp, args.get(4).unwrap_or(JsValue::undefined_ref())) else {
        return type_error(interp, "vm: not the global object of a context");
    };
    let (program, origin) = match parse(&source, interp) {
        Ok(parsed) => parsed,
        Err(f) => return done(failure_value(interp, &f, line_offset, 1)),
    };
    let statements = program.body.as_slice();
    let [Statement::Expression(expr @ Expression::Function(_))] = statements else {
        let f = Failure {
            message: "Unexpected token '}'".to_string(),
            span: (0, 0),
            text: String::new(),
        };
        return done(failure_value(interp, &f, line_offset, 0));
    };
    interp.node_host.stack.name_pending(
        &origin,
        &filename,
        line_offset.saturating_sub(1),
        column_offset,
        1,
    );
    let caller_realm = interp.current_realm_id;
    interp.current_realm_id = realm;
    let mut env = interp.realm().global_env.clone();
    for extension in args.iter().skip(5) {
        if let Some(id) = extension.as_object_id() {
            env = Environment::new_with_object(env, id);
        }
    }
    let result = interp.eval_expr(expr, &env);
    interp.current_realm_id = caller_realm;
    result
}

impl Interpreter {
    pub(super) fn setup_node_host_vm(&mut self) {
        let binding = self.create_object_id();
        type Native = fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion;
        let functions: [(&str, usize, Native); 7] = [
            ("newContext", 0, new_context),
            ("prepare", 1, prepare),
            ("settle", 1, settle),
            ("compile", 4, compile),
            ("run", 2, run),
            ("release", 1, release),
            ("compileFunction", 5, compile_function),
        ];
        for (name, arity, native) in functions {
            let f = self.create_function(JsFunction::native(String::new(), arity, native));
            self.get_object_cell_expect(binding)
                .borrow_mut()
                .insert_value(name, f);
        }
        self.install_host_global("__host_vm", JsValue::object(binding));
    }
}
