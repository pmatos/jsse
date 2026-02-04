#![allow(
    dead_code,
    clippy::type_complexity,
    clippy::too_many_arguments,
    clippy::wrong_self_convention,
    clippy::collapsible_if,
    clippy::collapsible_match,
    clippy::if_same_then_else,
    clippy::single_match,
    clippy::needless_range_loop,
    clippy::while_let_loop,
    clippy::cloned_ref_to_slice_refs,
    clippy::unnecessary_unwrap
)]

mod ast;
mod interpreter;
mod lexer;
mod parser;
mod types;

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
}

enum EngineError {
    Parse(String),
    Runtime(String),
}

fn run_source(source: &str, is_module: bool, path: Option<&Path>) -> Result<(), EngineError> {
    run_source_with_interp(
        &mut interpreter::Interpreter::new(),
        source,
        is_module,
        path,
    )
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

fn execute_code(code: &str, is_module: bool, path: Option<&Path>) -> ExitCode {
    match run_source(code, is_module, path) {
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

fn run_file(path: &Path, force_module: bool) -> ExitCode {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {e}", path.display());
            return ExitCode::from(1);
        }
    };
    let is_module = force_module || path.extension().is_some_and(|ext| ext == "mjs");
    let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    execute_code(&source, is_module, Some(&abs_path))
}

fn run_repl() -> ExitCode {
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
                    match run_source(trimmed, false, None) {
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

fn main() -> ExitCode {
    let cli = Cli::parse();

    // If no preludes, use simpler path
    if cli.prelude.is_empty() {
        if let Some(code) = &cli.eval {
            return execute_code(code, cli.module, None);
        }

        if let Some(path) = &cli.file {
            return run_file(path, cli.module);
        }

        return run_repl();
    }

    // With preludes, we need to use a single interpreter instance
    let mut interp = interpreter::Interpreter::new();

    // Run prelude files as scripts
    for prelude_path in &cli.prelude {
        let source = match std::fs::read_to_string(prelude_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error reading prelude {}: {e}", prelude_path.display());
                return ExitCode::from(1);
            }
        };
        if let Err(e) = run_source_with_interp(&mut interp, &source, false, None) {
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

    // Now run the main file
    if let Some(code) = &cli.eval {
        match run_source_with_interp(&mut interp, code, cli.module, None) {
            Ok(()) => ExitCode::SUCCESS,
            Err(EngineError::Parse(msg)) => {
                eprintln!("SyntaxError: {msg}");
                ExitCode::from(2)
            }
            Err(EngineError::Runtime(msg)) => {
                eprintln!("{msg}");
                ExitCode::from(1)
            }
        }
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
        match run_source_with_interp(&mut interp, &source, is_module, Some(&abs_path)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(EngineError::Parse(msg)) => {
                eprintln!("SyntaxError: {msg}");
                ExitCode::from(2)
            }
            Err(EngineError::Runtime(msg)) => {
                eprintln!("{msg}");
                ExitCode::from(1)
            }
        }
    } else {
        run_repl()
    }
}
