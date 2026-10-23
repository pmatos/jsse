"""Feature-level micro suite: jsse vs boa(s); inner ms (EXTRA line) and process wall."""

import argparse
import json
import re
import subprocess
import time
from pathlib import Path

ap = argparse.ArgumentParser()
ap.add_argument("--dir", required=True)
ap.add_argument("--out", required=True)
ap.add_argument("--engine", action="append", required=True, help="label=path")
ap.add_argument("--node")
ap.add_argument("--repeats", type=int, default=3)
ap.add_argument("--cpu", type=int, default=2)
ap.add_argument("--timeout", type=int, default=120)
ap.add_argument("--filter", default="")
a = ap.parse_args()
engines = dict(e.split("=", 1) for e in a.engine)
files = sorted(p for p in Path(a.dir).glob("*.js") if a.filter in p.name)
res = {}


def run(cmd, f):
    t = time.perf_counter()
    try:
        p = subprocess.run(
            ["taskset", "-c", str(a.cpu), *cmd, str(f)],
            capture_output=True,
            text=True,
            timeout=a.timeout,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return {"status": "timeout", "wall": a.timeout}
    w = time.perf_counter() - t
    m = re.search(r"^EXTRA (\S+) (\d+) (.*)$", p.stdout, re.MULTILINE)
    ok = p.returncode == 0 and m
    return {
        "status": "pass" if ok else "error",
        "wall": w,
        "inner": int(m.group(2)) if m else None,
        "value": m.group(3) if m else None,
        "err": (p.stderr or p.stdout)[-300:] if not ok else "",
    }


for f in files:
    name = f.stem
    r = {}
    if a.node:
        r["node"] = [run([a.node], f)]
    for i in range(a.repeats):
        order = list(engines) if i % 2 == 0 else list(engines)[::-1]
        for lab in order:
            r.setdefault(lab, []).append(run([engines[lab]], f))
    res[name] = r
    print(
        name,
        {k: [x["status"][0] + str(x.get("inner")) for x in v] for k, v in r.items()},
        flush=True,
    )
    with open(a.out, "w") as fh:
        json.dump(res, fh, indent=1)
