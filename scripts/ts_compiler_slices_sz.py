"""Slice rows for the compiler runner's test files S to Z (issue #1273), for run-ts-compiler-oracle.py.

    uv run python scripts/run-ts-compiler-oracle.py --slices-module ts_compiler_slices_sz \
        --known ts-suite/compiler-known-failures-s-z.txt s-z

`scripts/run-ts-compiler-sz-oracle.py` wraps that and runs the letters one after the other. The suite files
start with S, T, U, V, W or Y only (no X or Z), so those six letters partition `s-z`.
"""

LETTERS = ("s", "t", "u", "v", "w", "y")

SLICES = {letter: f"prefix:{letter.upper()}" for letter in LETTERS}
SLICES["s-z"] = "prefix:S-Z"
