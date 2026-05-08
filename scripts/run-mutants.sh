#!/usr/bin/env bash
# Run cargo-mutants locally with the jsse-specific oracle wired up.
#
# Usage:
#   ./scripts/run-mutants.sh                          # all mutants, in-place
#   ./scripts/run-mutants.sh --shard 0/8              # single shard
#   ./scripts/run-mutants.sh --file src/lexer.rs      # single file
#   ./scripts/run-mutants.sh --list                   # enumerate, don't run
#
# All arguments are forwarded to `cargo mutants`. The oracle is `cargo test
# --release`, which includes `tests/test262_smoke_oracle.rs` (gated on
# JSSE_MUTANTS_ORACLE — set automatically here).
#
# Exclusions and per-mutant timeout floor live in `.cargo/mutants.toml`.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

if ! command -v cargo-mutants >/dev/null 2>&1; then
    echo "error: cargo-mutants is not installed." >&2
    echo "       run: cargo install cargo-mutants --locked" >&2
    exit 1
fi

# `tests/test262_smoke_oracle.rs` shells out to `uv run python …`; the GitHub
# Actions runner sets uv up via setup-uv@v7, but locally uv typically lives in
# ~/.local/bin which isn't on every shell's PATH.
if ! command -v uv >/dev/null 2>&1 && [[ -x "$HOME/.local/bin/uv" ]]; then
    export PATH="$HOME/.local/bin:$PATH"
fi
if ! command -v uv >/dev/null 2>&1; then
    echo "error: uv is not on PATH and not at ~/.local/bin/uv." >&2
    echo "       see https://docs.astral.sh/uv/ for install instructions." >&2
    exit 1
fi

cd "$PROJECT_DIR"

export JSSE_MUTANTS_ORACLE=1

# If the user passed their own `--` separator, trust the rest of their args.
# Otherwise append `-- --release` so cargo test runs with the release profile
# (debug builds of the JS engine are too slow for the test262 oracle).
has_separator=false
for arg in "$@"; do
    if [[ "$arg" == "--" ]]; then
        has_separator=true
        break
    fi
done

if $has_separator; then
    exec cargo mutants --in-place --baseline=skip --timeout 600 "$@"
else
    exec cargo mutants --in-place --baseline=skip --timeout 600 "$@" -- --release
fi
