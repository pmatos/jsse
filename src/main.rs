#[allow(dead_code)]
mod lexer;
#[allow(dead_code)]
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

fn run_source(_source: &str) -> Result<(), EngineError> {
    Err(EngineError::NotImplemented)
}

enum EngineError {
    NotImplemented,
}

fn execute_code(code: &str) -> ExitCode {
    match run_source(code) {
        Ok(()) => ExitCode::SUCCESS,
        Err(EngineError::NotImplemented) => ExitCode::from(1),
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
                if !trimmed.is_empty()
                    && let Err(EngineError::NotImplemented) = run_source(trimmed)
                {
                    eprintln!("Engine not yet implemented");
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
