"""Corrupted reference baselines for the control run of scripts/run-ts-compiler-oracle.py (issue #1268)."""

import os
import shutil

CONTROL_SUITES = 7
# (extensions to look for, action): each picked suite gets the next row
CONTROL_MODES = (
    ((".js",), "append"),
    ((".errors.txt", ".types"), "append"),
    ((".types", ".symbols"), "remove"),
    ((".symbols", ".js"), "replace"),
    ((".errors.txt", ".js"), "remove"),
    ((".js.map", ".sourcemap.txt", ".js"), "append"),
    (
        (".errors.txt",),
        "add",
    ),  # a reference for a baseline the test no longer produces (".delete" file)
)


def make_control(ref, chosen, dest):
    """Corrupt the reference baselines of up to seven suites into `dest`; returns (suites, what was done)."""
    listing = os.listdir(ref)
    picked, done = [], []
    for s in chosen:
        stem = s.base.rsplit(".", 1)[0]
        names = sorted(n for n in listing if n.startswith((stem + ".", stem + "(")))
        if len(names) < 2 or any(p.base == s.base for p in picked):
            continue
        exts, action = CONTROL_MODES[len(picked)]
        if action == "add" and any(n.endswith(".errors.txt") for n in names):
            continue
        for n in names:
            shutil.copy(os.path.join(ref, n), os.path.join(dest, n))
        target = next((n for e in exts for n in names if n.endswith(e)), names[0])
        if action == "add":
            target = stem + exts[0]
        path = os.path.join(dest, target)
        if action == "remove":
            os.remove(path)
        else:
            with open(path, "a" if action == "append" else "w") as f:
                f.write("\ncorrupted reference\n")
        picked.append(s)
        done.append(f"{target}: {action}")
        if len(picked) == CONTROL_SUITES:
            break
    return picked, done
