#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

pub mod ast;
mod cli;
pub(crate) mod emoji_strings;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod types;
pub(crate) mod unicode_tables;

/// Runs the `jsse` CLI (argument parsing, file/eval/REPL/prelude dispatch) on
/// the engine-sized stack (see [`run_on_engine_stack`]). The sole entry point
/// `src/main.rs` calls — everything else the CLI needs stays `pub(crate)`
/// rather than swelling the library's public API, since only this function
/// and the fuzz-target helpers below need to cross the bin/lib boundary.
pub fn run_cli() -> std::process::ExitCode {
    run_on_engine_stack(cli::run_main)
}

/// Stack reserved for parsing/interpreting: deep (but bounded, see
/// `MAX_PARSE_DEPTH`/`CALL_DEPTH_HARD_LIMIT`) recursion needs room to reach
/// those catchable limits and throw before the native stack overflows into a
/// SIGABRT. Sized to fit inside the 512 MiB RLIMIT_AS the test harness (and
/// fuzz targets, see `fuzz/`) impose.
pub const ENGINE_STACK_SIZE: usize = 128 * 1024 * 1024;

/// Runs `f` on a thread with [`ENGINE_STACK_SIZE`] of stack, falling back to
/// the current thread if that large a stack can't be reserved (e.g. a tight
/// `RLIMIT_AS`). Any panic in `f` is propagated to the caller via
/// `resume_unwind` rather than swallowed at the join, so callers that rely on
/// process-level panic handling (e.g. libFuzzer's panic hook) still see it.
///
/// `F: Copy` (rather than plain `FnOnce`) so that if the OS thread spawn
/// itself fails, the same `f` can still be run as the fallback: a `Copy`
/// closure capturing only `Copy` data (e.g. a `&str` a fuzz target derived
/// from its input bytes) remains usable after being handed to `spawn_scoped`,
/// with no `'static` bound needed.
pub fn run_on_engine_stack<F, R>(f: F) -> R
where
    F: Fn() -> R + Send + Copy,
    R: Send,
{
    let spawned = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("jsse-engine".to_string())
            .stack_size(ENGINE_STACK_SIZE)
            .spawn_scoped(scope, f)
            .map(|handle| handle.join())
    });
    match spawned {
        Ok(Ok(result)) => result,
        Ok(Err(payload)) => std::panic::resume_unwind(payload),
        Err(_) => f(),
    }
}

/// Parses `data` and discards the result, panicking only if the parser
/// itself panics/aborts. Backs the `parse_roundtrip` fuzz target
/// (`fuzz/fuzz_targets/parse_roundtrip.rs`): a parse *error* is a normal
/// outcome for arbitrary bytes, not a finding, so the `Result` is discarded
/// entirely inside `run_on_engine_stack`'s closure — `ast::Program` isn't
/// `Send` (it's `Rc`-based), so it could not cross the thread boundary
/// anyway.
pub fn fuzz_parse_bytes(data: &[u8]) {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };
    run_on_engine_stack(move || {
        let _ = parser::Parser::new(src).and_then(|mut p| p.parse_program());
    });
}

#[cfg(test)]
mod lib_tests {
    use super::*;

    #[test]
    fn run_on_engine_stack_returns_result() {
        assert_eq!(run_on_engine_stack(|| 1 + 1), 2);
    }

    #[test]
    fn run_on_engine_stack_borrows_non_static_data() {
        // `ast::Program` holds `Rc`s and isn't `Send`, so (matching how a real
        // fuzz target must behave) the parse result is consumed entirely
        // inside the closure; only the `bool` verdict crosses the thread.
        let owned = "[".repeat(parser::MAX_PARSE_DEPTH as usize * 2);
        let borrowed: &str = &owned;
        let is_err = run_on_engine_stack(move || {
            parser::Parser::new(borrowed)
                .and_then(|mut p| p.parse_program())
                .is_err()
        });
        assert!(
            is_err,
            "deeply nested input should hit MAX_PARSE_DEPTH, not abort"
        );
    }

    #[test]
    fn fuzz_parse_bytes_empty_input() {
        fuzz_parse_bytes(b"");
    }

    #[test]
    fn fuzz_parse_bytes_invalid_utf8() {
        fuzz_parse_bytes(&[0xff, 0xfe, 0x00, 0x80]);
    }

    #[test]
    fn fuzz_parse_bytes_valid_program() {
        fuzz_parse_bytes(b"function f(x) { return x + 1; } f(41);");
    }

    #[test]
    fn fuzz_parse_bytes_deep_nesting() {
        fuzz_parse_bytes("[".repeat(parser::MAX_PARSE_DEPTH as usize * 2).as_bytes());
    }
}
