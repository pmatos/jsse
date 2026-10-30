"""Corrupted reference baselines for the control run of scripts/run-ts-project-oracle.py (issue #1286).

When every test of a slice passes, the failing path of the project runner (baseline diff, `New baseline created`
error, local file under tests/baselines/local/project/<name>/<node|amd>/...) is never exercised. The control picks a
few suites of different json files and corrupts one reference file of each in a private copy of the reference tree;
Node and jsse must fail the same tests with the same messages and write the same local files.
"""

import os
import shutil

CONTROL_SUITES = 5
# (suffix of the baseline file to corrupt, action): each picked suite gets the next row
CONTROL_MODES = (
    (".json", "append"),  # module resolution tracing
    (".js", "replace"),  # JS output
    (".js.map", "remove"),  # a reference the JS output test needs but is missing
    (".d.ts", "append"),  # JS output (declarations are emitted files too)
    (".json", "remove"),
)
MODULE_DIR = {"@module: commonjs": "node", "@module: amd": "amd"}


def make_control(ref_src, chosen, dest):
    """Corrupt reference files of up to CONTROL_SUITES suites into `dest`; returns (suites, what was done)."""
    picked, done, files = [], [], set()
    for s in chosen:
        if s.base in files or len(picked) == CONTROL_SUITES:
            continue
        stem = s.base.rsplit(".", 1)[0]
        mod = MODULE_DIR.get(s.config)
        src = os.path.join(ref_src, "project", stem)
        if mod is None or not os.path.isdir(src):
            continue
        suffix, action = CONTROL_MODES[len(picked)]
        mod_dir = os.path.join(src, mod)
        rel = sorted(
            os.path.relpath(os.path.join(d, n), src)
            for d, _, names in os.walk(mod_dir)
            for n in names
            if n.endswith(suffix) and not n.endswith(".dts.errors.txt")
        )
        if not rel:
            continue
        shutil.copytree(src, os.path.join(dest, "project", stem))
        path = os.path.join(dest, "project", stem, rel[0])
        if action == "remove":
            os.remove(path)
        else:
            with open(path, "a" if action == "append" else "w") as f:
                f.write("\ncorrupted reference\n")
        files.add(s.base)
        picked.append(s)
        done.append(f"{stem}/{rel[0]}: {action}")
    return picked, done
