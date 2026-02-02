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
    clippy::cloned_ref_to_slice_refs
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
}

enum EngineError {
    Parse(String),
    Runtime(String),
}

fn run_source(source: &str) -> Result<(), EngineError> {
    let mut p = parser::Parser::new(source).map_err(|e| EngineError::Parse(format!("{e:?}")))?;
    let program = p
        .parse_program()
        .map_err(|e| EngineError::Parse(format!("{e:?}")))?;
    let mut interp = interpreter::Interpreter::new();
    match interp.run(&program) {
        interpreter::Completion::Throw(val) => Err(EngineError::Runtime(interp.format_value(&val))),
        _ => Ok(()),
    }
}

fn execute_code(code: &str) -> ExitCode {
    match run_source(code) {
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
}

fn run_file(path: &Path) -> ExitCode {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {e}", path.display());
            return ExitCode::from(1);
        }
    };
    execute_code(&source)
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
                    match run_source(trimmed) {
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

    if let Some(code) = &cli.eval {
        return execute_code(code);
    }

    if let Some(path) = &cli.file {
        return run_file(path);
    }

    run_repl()
}
