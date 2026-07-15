#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]
mod ast;
pub(crate) mod emoji_strings;
mod interpreter;
mod lexer;
mod parser;
mod types;
pub(crate) mod unicode_tables;

use clap::Parser;
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

    /// Enable the bytecode compiler + VM for eligible functions
    #[arg(long = "bytecode")]
    bytecode: bool,

    /// Enable the Node host-compat syscall floor (issue #229): installs the
    /// internal, non-enumerable `__host_*` globals (byte I/O, OS entropy,
    /// monotonic clock, process exit) used by the Node prelude. Off by default;
    /// deliberately NOT auto-enabled by `--prelude`, since the test262 harness
    /// loads via `--prelude` and must keep the default global environment.
    #[arg(long = "node")]
    node: bool,
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
        interp.run_with_path(&program, p)
    } else {
        interp.run(&program)
    };
    match result {
        interpreter::Completion::Throw(val) => Err(EngineError::Runtime(interp.format_value(&val))),
        _ => Ok(()),
    }
}

/// Map a run result to a process exit code. A pending `__host_exit` (issue
/// #229) takes precedence: its code becomes the process status and the sentinel
/// throw is not reported as an error. `pending_exit` is always `None` unless the
/// node host floor was enabled, so this is behaviour-identical to before when
/// `--node` is absent.
fn exit_code_from_result(
    interp: &interpreter::Interpreter,
    result: Result<(), EngineError>,
) -> ExitCode {
    if let Some(code) = interp.pending_exit {
        return ExitCode::from((code as u32 & 0xff) as u8);
    }
    match result {
        Ok(()) => ExitCode::SUCCESS,
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

fn new_interp(can_block: bool, bytecode: bool, node: bool) -> interpreter::Interpreter {
    let mut interp = interpreter::Interpreter::new();
    interp.can_block = can_block;
    interp.bytecode_enabled = bytecode;
    if node {
        interp.enable_node_host();
    }
    interp
}

fn execute_code(
    code: &str,
    is_module: bool,
    path: Option<&Path>,
    can_block: bool,
    bytecode: bool,
    node: bool,
) -> ExitCode {
    let mut interp = new_interp(can_block, bytecode, node);
    let result = run_source_with_interp(&mut interp, code, is_module, path);
    exit_code_from_result(&interp, result)
}

fn run_file(
    path: &Path,
    force_module: bool,
    can_block: bool,
    bytecode: bool,
    node: bool,
) -> ExitCode {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {e}", path.display());
            return ExitCode::from(1);
        }
    };
    let is_module = force_module || path.extension().is_some_and(|ext| ext == "mjs");
    let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    execute_code(
        &source,
        is_module,
        Some(&abs_path),
        can_block,
        bytecode,
        node,
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
                    match run_source_with_interp(interp, trimmed, false, None) {
                        Ok(()) => {}
                        Err(EngineError::Parse(msg)) => eprintln!("SyntaxError: {msg}"),
                        Err(EngineError::Runtime(msg)) => eprintln!("{msg}"),
                    }
                    if let Some(code) = interp.pending_exit {
                        return ExitCode::from((code as u32 & 0xff) as u8);
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

fn main() -> ExitCode {
    let cli = Cli::parse();

    // If no preludes, use simpler path
    if cli.prelude.is_empty() {
        if let Some(code) = &cli.eval {
            return execute_code(
                code,
                cli.module,
                None,
                cli.can_block,
                cli.bytecode,
                cli.node,
            );
        }

        if let Some(path) = &cli.file {
            return run_file(path, cli.module, cli.can_block, cli.bytecode, cli.node);
        }

        let mut interp = new_interp(cli.can_block, cli.bytecode, cli.node);
        return run_repl(&mut interp);
    }

    // With preludes, we need to use a single interpreter instance
    let mut interp = new_interp(cli.can_block, cli.bytecode, cli.node);

    // Run prelude files as scripts
    for prelude_path in &cli.prelude {
        let source = match std::fs::read_to_string(prelude_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error reading prelude {}: {e}", prelude_path.display());
                return ExitCode::from(1);
            }
        };
        let prelude_result = run_source_with_interp(&mut interp, &source, false, None);
        // A prelude that called `__host_exit` is a clean exit, not an error.
        if interp.pending_exit.is_some() {
            return exit_code_from_result(&interp, prelude_result);
        }
        if let Err(e) = prelude_result {
            match e {
                EngineError::Parse(msg) => {
                    eprintln!("SyntaxError in prelude: {msg}");
                    return ExitCode::from(2);
                }
                EngineError::Runtime(msg) => {
                    eprintln!("Error in prelude: {msg}");
                    return ExitCode::from(1);
                }
            }
        }
    }

    // Now run the main input.
    if let Some(code) = &cli.eval {
        let result = run_source_with_interp(&mut interp, code, cli.module, None);
        exit_code_from_result(&interp, result)
    } else if let Some(path) = &cli.file {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error reading {}: {e}", path.display());
                return ExitCode::from(1);
            }
        };
        let is_module = cli.module || path.extension().is_some_and(|ext| ext == "mjs");
        let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let result = run_source_with_interp(&mut interp, &source, is_module, Some(&abs_path));
        exit_code_from_result(&interp, result)
    } else {
        run_repl(&mut interp)
    }
}
