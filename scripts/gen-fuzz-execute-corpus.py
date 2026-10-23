#!/usr/bin/env python3
"""Generate the test262-derived seeds for the `execute` fuzz target.

Picks small, flag-free, deterministic test262 tests, spreads them across
top-level directories, and inlines the harness (sta.js + assert.js) so each
seed is self-contained. Output is deterministic so reruns are reviewable.

    git submodule update --init --depth 1 test262
    uv run python scripts/gen-fuzz-execute-corpus.py
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TEST262 = ROOT / "test262"
OUT = ROOT / "fuzz" / "corpus" / "execute"

MAX_TEST_BYTES = 2048
PER_DIR = 2
MAX_SEEDS = 40
ROOTS = ("language", "built-ins")

# Mirrors src/lib.rs: inputs with these identifiers are skipped or not compared,
# which would make a seed's replay prove nothing.
MARKERS = (
    "agent", "waitAsync", "sleep", "Date", "DateTimeFormat", "Temporal", "WeakRef",
    "FinalizationRegistry", "$262", "Atomics", "SharedArrayBuffer",
    "performance", "gc",
)  # fmt: skip
MARKER_RE = re.compile(
    r"(?<![\w$])(?:" + "|".join(re.escape(m) for m in MARKERS) + r")(?![\w$])"
)
FRONT_MATTER = re.compile(r"/\*---(.*?)---\*/", re.S)


def eligible(text: str) -> bool:
    if len(text) > MAX_TEST_BYTES or MARKER_RE.search(text):
        return False
    meta = FRONT_MATTER.search(text)
    if not meta:
        return False
    body = meta.group(1)
    return not re.search(r"^\s*(flags|includes|negative):", body, re.M)


def main() -> int:
    if not (TEST262 / "test").is_dir():
        print(
            "test262 submodule is empty: git submodule update --init --depth 1 test262"
        )
        return 1
    harness = ""
    for name in ("sta.js", "assert.js"):
        text = (TEST262 / "harness" / name).read_text(encoding="utf-8")
        if MARKER_RE.search(text):
            print(f"harness/{name} contains a nondeterminism marker")
            return 1
        harness += text + "\n"

    OUT.mkdir(parents=True, exist_ok=True)
    for old in OUT.glob("test262-*.js"):
        old.unlink()

    picks: list[Path] = []
    for root in ROOTS:
        for directory in sorted(
            p for p in (TEST262 / "test" / root).iterdir() if p.is_dir()
        ):
            taken = 0
            for path in sorted(directory.rglob("*.js")):
                if "_FIXTURE" in path.name:
                    continue
                if eligible(path.read_text(encoding="utf-8")):
                    picks.append(path)
                    taken += 1
                    if taken == PER_DIR:
                        break
    step = max(1, len(picks) // MAX_SEEDS)
    picks = picks[::step][:MAX_SEEDS]

    for path in picks:
        rel = path.relative_to(TEST262 / "test").with_suffix("")
        name = "test262-" + "-".join(rel.parts) + ".js"
        (OUT / name).write_text(
            harness + path.read_text(encoding="utf-8"), encoding="utf-8"
        )
    print(f"wrote {len(picks)} seeds to {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
