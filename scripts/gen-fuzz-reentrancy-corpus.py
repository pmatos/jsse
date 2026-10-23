#!/usr/bin/env python3
"""Generate the seeds for the `reentrancy` fuzz target.

One single-scenario seed per (op, host) pairing, plus a few multi-scenario
seeds. Variant order is read from the `ALL` tables in src/fuzz_reentrancy/, so
a seed's bytes always decode to the pairing in its name. Output is
deterministic so reruns are reviewable.

    uv run python scripts/gen-fuzz-reentrancy-corpus.py
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src" / "fuzz_reentrancy"
OUT = ROOT / "fuzz" / "corpus" / "reentrancy"

HOOKS = {"proxytrap": 11, "generator": 5, "accessor": 2, "callback": 3}


def variants(path: Path, enum: str) -> list[str]:
    text = path.read_text()
    table = re.search(rf"const ALL: \[{enum}; \d+\] = \[(.*?)\];", text, re.S)
    if not table:
        sys.exit(f"no {enum}::ALL table in {path}")
    return re.findall(rf"{enum}::(\w+)", table.group(1))


def kebab(name: str) -> str:
    return re.sub(r"(?<!^)(?=[A-Z])", "-", name).lower()


def seed(count: int, scenarios: list[tuple[int, int, int, int, int, int]]) -> bytes:
    body = bytes(b for s in scenarios for b in s)
    return bytes([count - 1]) + body


def main() -> int:
    ops = variants(SRC / "ops.rs", "Op")
    hosts = variants(SRC / "hosts.rs", "Host")
    actions = variants(SRC / "hosts.rs", "Action")
    OUT.mkdir(parents=True, exist_ok=True)
    for stale in OUT.glob("*.bin"):
        stale.unlink()

    written = 0
    for oi, op in enumerate(ops):
        for hi, host in enumerate(hosts):
            hook = (oi * 3 + hi) % HOOKS[host.lower()]
            action = (oi + hi * 5) % len(actions)
            data = seed(1, [(oi, hi, hook, action, oi % 4, 0)])
            name = f"{kebab(op)}-{kebab(host)}-{kebab(actions[action])}.bin"
            (OUT / name).write_bytes(data)
            written += 1

    # Multi-scenario seeds: every action once, rotating through ops and hosts.
    for chunk in range(0, len(actions), 4):
        scenarios = []
        for k, action in enumerate(range(chunk, min(chunk + 4, len(actions)))):
            scenarios.append(
                ((chunk + k) * 7 % len(ops), (chunk + k) % len(hosts), k, action, k, 1)
            )
        data = seed(len(scenarios), scenarios)
        (OUT / f"multi-{chunk // 4}.bin").write_bytes(data)
        written += 1
    print(f"wrote {written} seeds to {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
