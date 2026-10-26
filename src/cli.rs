use crate::cli_node::NodeMainKind;
use crate::{interpreter, parser};

use clap::{CommandFactory, Parser};
use std::ffi::OsString;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "jsse", version, about = "A JavaScript engine in Rust")]
struct Cli {
    /// JavaScript file to execute
    file: Option<PathBuf>,

    /// Evaluate inline JavaScript
    #[arg(short = 'e', long = "eval")]
    eval: Option<String>,

    /// Parse as ES module (auto-detected for .mjs files)
    #[arg(short = 'm', long = "module")]
    module: bool,

    /// Prelude files to run as scripts before the main file (for test harness)
    #[arg(long = "prelude")]
    prelude: Vec<PathBuf>,

    /// Allow the main agent to block (Atomics.wait)
    #[arg(long = "can-block")]
    can_block: bool,

    /// Disable the bytecode compiler + VM (run everything on the tree-walker)
    #[arg(long = "no-bytecode")]
    no_bytecode: bool,

    /// Enable Node host mode: installs the syscall floor (issue #229, the
    /// internal `__host_*` globals) and evaluates the embedded Node prelude
    /// (issue #1216: `process`, `console`, `util`, `Buffer`, `TextEncoder`,
    /// `TextDecoder`, `crypto`). Off by default; deliberately NOT auto-enabled
    /// by `--prelude`, since the test262 harness loads via `--prelude` and must
    /// keep the default global environment.
    #[arg(long = "node")]
    node: bool,

    /// Stop the run, uncatchably, after N counted steps (statement boundaries,
    /// loop back-edges, microtask jobs, timer callbacks); exits with status 124.
    #[arg(long = "step-budget", value_name = "N")]
    step_budget: Option<u64>,

    /// Everything after the script path (or after `-e CODE`), verbatim. Filled
    /// by `split_script_args`, never by clap.
    #[arg(skip)]
    script_args: Vec<OsString>,
}

/// Split `argv` into the part clap parses and the script's own arguments.
///
/// Matches `node`: the first non-option token is the script and everything
/// after it is passed through untouched (including `--` and flags like
/// `--version`). A `--` before the script ends option parsing and is consumed.
/// With `-e CODE` there is no script, so the first leftover token and
/// everything after it are script arguments.
fn split_script_args(argv: Vec<OsString>) -> (Vec<OsString>, Vec<OsString>) {
    let cmd = Cli::command();
    let find = |short: Option<char>, long: Option<&str>| {
        cmd.get_arguments().find(|a| {
            (short.is_some() && a.get_short() == short) || (long.is_some() && a.get_long() == long)
        })
    };
    let takes_value = |short: Option<char>, long: Option<&str>| {
        find(short, long).is_some_and(|a| a.get_action().takes_values())
    };
    let is_eval = |short: Option<char>, long: Option<&str>| {
        find(short, long).is_some_and(|a| a.get_id() == "eval")
    };

    let mut head = Vec::new();
    let mut has_eval = false;
    let mut i = 0;
    let mut only_positionals = false;
    while i < argv.len() {
        let tok = argv[i].clone();
        let text = tok.to_string_lossy().into_owned();
        if i == 0 {
            head.push(tok);
            i += 1;
            continue;
        }
        if !only_positionals && text == "--" {
            only_positionals = true;
            head.push(tok);
            i += 1;
            continue;
        }
        let is_option = !only_positionals && text.starts_with('-') && text != "-";
        if !is_option {
            if has_eval {
                return (head, argv[i..].to_vec());
            }
            head.push(tok);
            return (head, argv[i + 1..].to_vec());
        }
        head.push(tok);
        i += 1;
        if let Some(long) = text.strip_prefix("--") {
            if let Some((name, _)) = long.split_once('=') {
                has_eval |= is_eval(None, Some(name));
            } else {
                has_eval |= is_eval(None, Some(long));
                if takes_value(None, Some(long)) && i < argv.len() {
                    head.push(argv[i].clone());
                    i += 1;
                }
            }
        } else {
            let chars: Vec<char> = text.chars().skip(1).collect();
            for (n, c) in chars.iter().enumerate() {
                has_eval |= is_eval(Some(*c), None);
                if takes_value(Some(*c), None) {
                    if n + 1 == chars.len() && i < argv.len() {
                        head.push(argv[i].clone());
                        i += 1;
                    }
                    break;
                }
            }
        }
    }
    (head, Vec::new())
}

/// What `process.argv` is built from under `--node`.
struct NodeLaunch {
    script: Option<PathBuf>,
    args: Vec<String>,
    main_kind: NodeMainKind,
}

/// The program to run for the script `path`. Under `--node` a script file is
/// the CommonJS main module: the prelude's loader reads it, so the launcher
/// runs a stub instead.
fn read_main_source(path: &Path, node: Option<&NodeLaunch>) -> std::io::Result<String> {
    match node {
        Some(n) if n.main_kind == NodeMainKind::Cjs => {
            Ok(interpreter::Interpreter::node_main_bootstrap().to_string())
        }
        _ => std::fs::read_to_string(path),
    }
}

fn is_module_main(path: &Path, force_module: bool, node: Option<&NodeLaunch>) -> bool {
    force_module
        || path.extension().is_some_and(|ext| ext == "mjs")
        || node.is_some_and(|n| n.main_kind == NodeMainKind::Esm)
}

enum EngineError {
    Parse(String),
    Runtime(String),
}

fn run_source_with_interp(
    interp: &mut interpreter::Interpreter,
    source: &str,
    is_module: bool,
    path: Option<&Path>,
) -> Result<(), EngineError> {
    let mut p = parser::Parser::new(source).map_err(|e| EngineError::Parse(format!("{e:?}")))?;
    let program = if is_module {
        p.parse_program_as_module()
            .map_err(|e| EngineError::Parse(format!("{e:?}")))?
    } else {
        p.parse_program()
            .map_err(|e| EngineError::Parse(format!("{e:?}")))?
    };
    let result = if let Some(p) = path {
        interp.note_script_source(p, source);
        interp.run_with_path(&program, p)
    } else {
        interp.run(&program)
    };
    match result {
        interpreter::Completion::Throw(_) if interp.node_main_throw_handled() => Ok(()),
        interpreter::Completion::Throw(val) => Err(EngineError::Runtime(interp.format_value(&val))),
        _ => Ok(()),
    }
}

/// Map a run result to a process exit code. A pending `__host_exit` (issue
/// #229) takes precedence: its code becomes the process status and the sentinel
/// throw is not reported as an error. `pending_exit` is only set by the node
/// host floor (`--node`) or an exhausted `--step-budget`.
/// Emits the opt-in execution counters (issue #526). Called from every path
/// that finishes executing JavaScript — `exit_code_from_result` covers the
/// eval/file/prelude exits, and the REPL exits call it directly — so the
/// documented "every instrumented run writes counters to stderr" holds rather
/// than applying only to `execute_code`.
#[cfg(feature = "perf-counters")]
fn report_perf_counters(interp: &interpreter::Interpreter) {
    eprint!("{}", interp.perf_counters_report());
}

/// Reports counters, then returns `code`. Used by the `run_main` early returns
/// that bypass `exit_code_from_result` but have already executed JavaScript —
/// a prelude that throws still accumulated counts worth reporting (#537
/// review, second pass).
fn exit_after_execution(_interp: &interpreter::Interpreter, code: u8) -> ExitCode {
    #[cfg(feature = "perf-counters")]
    report_perf_counters(_interp);
    ExitCode::from(code)
}

fn report_step_budget(interp: &interpreter::Interpreter) {
    if interp.step_budget_exhausted() {
        eprintln!(
            "jsse: step budget exhausted after {} steps",
            interp.steps_used()
        );
    }
}

fn exit_code_from_result(
    interp: &interpreter::Interpreter,
    result: Result<(), EngineError>,
) -> ExitCode {
    #[cfg(feature = "perf-counters")]
    report_perf_counters(interp);
    if let Some(code) = interp.pending_exit {
        report_step_budget(interp);
        return ExitCode::from((code as u32 & 0xff) as u8);
    }
    match result {
        Ok(()) => interp
            .node_host_exit_code()
            .map_or(ExitCode::SUCCESS, |code| {
                ExitCode::from((code as u32 & 0xff) as u8)
            }),
        Err(EngineError::Parse(msg)) => {
            eprintln!("SyntaxError: {msg}");
            ExitCode::from(2)
        }
        Err(EngineError::Runtime(msg)) => {
            // SyntaxErrors thrown during module resolution should use exit code 2
            if msg.starts_with("SyntaxError:") {
                eprintln!("{msg}");
                ExitCode::from(2)
            } else {
                eprintln!("{msg}");
                ExitCode::from(1)
            }
        }
    }
}

fn new_interp(
    can_block: bool,
    bytecode: bool,
    node: Option<&NodeLaunch>,
    step_budget: Option<u64>,
) -> interpreter::Interpreter {
    let mut interp = interpreter::Interpreter::new();
    interp.can_block = can_block;
    interp.bytecode_enabled = bytecode;
    if let Some(launch) = node {
        interp.enable_node_host();
        let argv0 = std::env::args_os()
            .next()
            .map_or_else(|| "jsse".to_string(), |a| a.to_string_lossy().into_owned());
        let exec_path = std::env::current_exe()
            .map_or_else(|_| argv0.clone(), |p| p.to_string_lossy().into_owned());
        interp.set_node_host_argv(argv0, exec_path, launch.script.clone(), launch.args.clone());
        interp.set_node_host_main_kind(launch.main_kind);
        if let Err(msg) = interp.load_node_prelude() {
            eprintln!("jsse: internal error: {msg}");
            std::process::exit(70);
        }
    }
    interp.set_step_budget(step_budget);
    interp
}

fn execute_code(
    code: &str,
    is_module: bool,
    path: Option<&Path>,
    can_block: bool,
    bytecode: bool,
    node: Option<&NodeLaunch>,
    step_budget: Option<u64>,
) -> ExitCode {
    let mut interp = new_interp(can_block, bytecode, node, step_budget);
    let result = run_source_with_interp(&mut interp, code, is_module, path);
    interp.node_host_finish();
    let exit = exit_code_from_result(&interp, result);
    retire_interpreter(interp);
    exit
}

/// Release builds skip the heap teardown (nothing in it is observable at exit); debug builds,
/// including `release-checked`, keep it for the arena assertions. Leak checkers see the heap as leaked.
fn retire_interpreter(interp: interpreter::Interpreter) {
    if cfg!(debug_assertions) {
        drop(interp);
    } else {
        std::mem::forget(interp);
    }
}

fn run_file(
    path: &Path,
    force_module: bool,
    can_block: bool,
    bytecode: bool,
    node: Option<&NodeLaunch>,
    step_budget: Option<u64>,
) -> ExitCode {
    let source = match read_main_source(path, node) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {e}", path.display());
            return ExitCode::from(1);
        }
    };
    let is_module = is_module_main(path, force_module, node);
    let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    execute_code(
        &source,
        is_module,
        Some(&abs_path),
        can_block,
        bytecode,
        node,
        step_budget,
    )
}

fn run_repl(interp: &mut interpreter::Interpreter) -> ExitCode {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    println!("jsse v{}", env!("CARGO_PKG_VERSION"));
    println!("Type JavaScript expressions. Press Ctrl-D to exit.");

    loop {
        print!("> ");
        if stdout.flush().is_err() {
            break;
        }

        let mut line = String::new();
        let read_result = stdin.lock().read_line(&mut line);

        match read_result {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    let repl_result = run_source_with_interp(interp, trimmed, false, None);
                    // A `__host_exit` (issue #229) surfaces as a sentinel
                    // Runtime error; honor the exit code before reporting it,
                    // matching the file/eval paths' `exit_code_from_result`.
                    if let Some(code) = interp.pending_exit {
                        report_step_budget(interp);
                        return ExitCode::from((code as u32 & 0xff) as u8);
                    }
                    match repl_result {
                        Ok(()) => {}
                        Err(EngineError::Parse(msg)) => eprintln!("SyntaxError: {msg}"),
                        Err(EngineError::Runtime(msg)) => eprintln!("{msg}"),
                    }
                }
            }
            Err(e) => {
                eprintln!("Read error: {e}");
                return ExitCode::from(1);
            }
        }
    }

    println!();
    ExitCode::SUCCESS
}

pub(crate) fn run_main() -> ExitCode {
    let (head, script_args) = split_script_args(std::env::args_os().collect());
    let mut cli = Cli::parse_from(head);
    cli.script_args = script_args;
    let launch = cli.node.then(|| NodeLaunch {
        main_kind: NodeMainKind::for_launch(cli.file.as_deref(), cli.eval.is_some(), cli.module),
        script: cli.file.clone(),
        args: cli
            .script_args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect(),
    });
    let node = launch.as_ref();

    // If no preludes, use simpler path
    if cli.prelude.is_empty() {
        if let Some(code) = &cli.eval {
            return execute_code(
                code,
                cli.module,
                None,
                cli.can_block,
                !cli.no_bytecode,
                node,
                cli.step_budget,
            );
        }

        if let Some(path) = &cli.file {
            return run_file(
                path,
                cli.module,
                cli.can_block,
                !cli.no_bytecode,
                node,
                cli.step_budget,
            );
        }

        let mut interp = new_interp(cli.can_block, !cli.no_bytecode, node, cli.step_budget);
        let code = run_repl(&mut interp);
        #[cfg(feature = "perf-counters")]
        report_perf_counters(&interp);
        return code;
    }

    // With preludes, we need to use a single interpreter instance
    let mut interp = new_interp(cli.can_block, !cli.no_bytecode, node, cli.step_budget);
    let code = run_with_preludes(&mut interp, &cli, node);
    retire_interpreter(interp);
    code
}

fn run_with_preludes(
    interp: &mut interpreter::Interpreter,
    cli: &Cli,
    node: Option<&NodeLaunch>,
) -> ExitCode {
    // Run prelude files as scripts
    for prelude_path in &cli.prelude {
        let source = match std::fs::read_to_string(prelude_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error reading prelude {}: {e}", prelude_path.display());
                return exit_after_execution(interp, 1);
            }
        };
        let prelude_result = run_source_with_interp(interp, &source, false, None);
        // A prelude that called `__host_exit` is a clean exit, not an error.
        if interp.pending_exit.is_some() {
            return exit_code_from_result(interp, prelude_result);
        }
        if let Err(e) = prelude_result {
            match e {
                EngineError::Parse(msg) => {
                    eprintln!("SyntaxError in prelude: {msg}");
                    return exit_after_execution(interp, 2);
                }
                EngineError::Runtime(msg) => {
                    eprintln!("Error in prelude: {msg}");
                    return exit_after_execution(interp, 1);
                }
            }
        }
    }

    // Now run the main input.
    if let Some(code) = &cli.eval {
        let result = run_source_with_interp(interp, code, cli.module, None);
        interp.node_host_finish();
        exit_code_from_result(interp, result)
    } else if let Some(path) = &cli.file {
        let source = match read_main_source(path, node) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error reading {}: {e}", path.display());
                return exit_after_execution(interp, 1);
            }
        };
        let is_module = is_module_main(path, cli.module, node);
        let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let result = run_source_with_interp(interp, &source, is_module, Some(&abs_path));
        interp.node_host_finish();
        exit_code_from_result(interp, result)
    } else {
        let code = run_repl(interp);
        #[cfg(feature = "perf-counters")]
        report_perf_counters(interp);
        code
    }
}
