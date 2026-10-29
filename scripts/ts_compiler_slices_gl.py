"""Slice rows for the compiler runner's test files G to L (issue #1271), for run-ts-compiler-oracle.py.

    uv run python scripts/run-ts-compiler-oracle.py --slices-module ts_compiler_slices_gl \
        --known ts-suite/compiler-known-failures-g-l.txt g-l

`scripts/run-ts-compiler-gl-oracle.py` wraps that and runs the letters one after the other.
"""

LETTERS = ("g", "h", "i", "j", "k", "l")

SLICES = {letter: f"prefix:{letter.upper()}" for letter in LETTERS}
SLICES["g-l"] = "prefix:G-L"
