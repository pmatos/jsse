"""Cross-check the position V8 reports for a dynamic import() (issue #1524).

Each shape in tests/node-import-positions-1524/shapes.txt is a module body
containing an import() of a missing module. Its promise rejects with an error
whose stack names the frame of the caller; the position of that frame is the
last source position V8's bytecode generator emitted before the call. The file
holds the position Node reports (`LINE:COLUMN`, lines counted from the first
line of the shape) and the test replays the shapes on jsse.

    uv run python scripts/run-node-import-positions-1524-oracle.py [options]

Options:
  --node            run the shapes on Node and rewrite the expected positions
  --jsse PATH       the jsse binary (default target/release/jsse)
  --no-bytecode     run jsse on the tree-walker
  --no-cross-check  do not run Node, compare with the file only
  --jobs N          processes at a time (at most 4)
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SHAPES = ROOT / "tests" / "node-import-positions-1524" / "shapes.txt"
PRELUDE = ROOT / "tests" / "node-import-positions-1524" / "prelude.mjs"
IMPORT = 'import("./nope.mjs")'


def load(path: Path) -> list[tuple[str, str, str]]:
    """(name, expected, code) of each `=== name | expected` block."""
    shapes = []
    name = expected = None
    body: list[str] = []
    for line in path.read_text().split("\n"):
        if line.startswith("=== "):
            if name is not None:
                shapes.append((name, expected or "", "\n".join(body)))
            head, _, expected = line[4:].partition(" | ")
            name, body = head.strip(), []
        elif name is not None:
            body.append(line)
    if name is not None:
        shapes.append((name, expected or "", "\n".join(body)))
    return [(n, e.strip(), c.rstrip("\n")) for n, e, c in shapes]


def save(path: Path, shapes: list[tuple[str, str, str]]) -> None:
    out = []
    for name, expected, code in shapes:
        out.append(f"=== {name} | {expected}\n{code}\n")
    path.write_text("".join(out))


def position(engine: list[str], code: str, work: Path) -> str:
    prelude = PRELUDE.read_text()
    lines = prelude.count("\n")
    (work / "probe.mjs").write_text(prelude + code.replace("IMP", IMPORT) + "\n")
    try:
        out = subprocess.run(
            engine + ["probe.mjs"], capture_output=True, text=True, timeout=60, cwd=work
        )
    except subprocess.TimeoutExpired:
        return "TIMEOUT"
    last = out.stdout.strip().splitlines()[-1:] or ["NOOUT"]
    parts = last[0].rstrip(")").split(":")
    if len(parts) >= 2 and parts[0].isdigit() and parts[1].isdigit():
        return f"{int(parts[0]) - lines}:{parts[1]}"
    return last[0]


def run_all(engine: list[str], shapes, jobs: int) -> list[str]:
    def one(shape):
        with tempfile.TemporaryDirectory() as tmp:
            return position(engine, shape[2], Path(tmp))

    with ThreadPoolExecutor(max_workers=jobs) as pool:
        return list(pool.map(one, shapes))


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--node", action="store_true")
    ap.add_argument("--jsse", default=str(ROOT / "target" / "release" / "jsse"))
    ap.add_argument("--no-bytecode", action="store_true")
    ap.add_argument("--no-cross-check", action="store_true")
    ap.add_argument("--jobs", type=int, default=4)
    args = ap.parse_args()
    jobs = max(1, min(args.jobs, 4))
    shapes = load(SHAPES)
    node = shutil.which("node")
    if args.node:
        if not node:
            print("node not found", file=sys.stderr)
            return 2
        got = run_all([node], shapes, jobs)
        save(SHAPES, [(n, g, c) for (n, _, c), g in zip(shapes, got)])
        print(f"wrote {len(shapes)} expected positions")
        return 0
    engine = [args.jsse, "--node"] + (["--no-bytecode"] if args.no_bytecode else [])
    failed = 0
    for (name, expected, _), got in zip(shapes, run_all(engine, shapes, jobs)):
        if got != expected:
            failed += 1
            print(f"FAIL {name}: expected {expected}, jsse {got}")
    if node and not args.no_cross_check:
        for (name, expected, _), got in zip(shapes, run_all([node], shapes, jobs)):
            if got != expected:
                failed += 1
                print(f"FAIL {name}: expected {expected}, node {got} (stale file?)")
    print(f"{len(shapes)} shapes, {failed} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
