"""Corrupted reference baselines for the control run of scripts/run-ts-conformance-1284-oracle.py (issue #1284).

scripts/ts_compiler_control.py picks the baselines of a suite by the file stem, so for a test with several `@target`
configurations it may corrupt the reference of `x(target=es2015).js` and then run the suite `x.ts (@target: es5)`, which
never reads it: the control checks nothing. The async tests are exactly such files, so this module keeps the same
modes and file choice but only considers the baselines of the configuration of the suite it picks.
"""

import os
import re
import shutil

import ts_compiler_control as shared

CONTROL_SUITES = shared.CONTROL_SUITES
CONTROL_MODES = shared.CONTROL_MODES

_PAIR = re.compile(r"@(\w+):\s*([^@]*?)\s*(?:,\s*(?=@)|$)")
_EXT = r"\.(?:js|types|symbols|errors\.txt|js\.map|sourcemap\.txt|trace\.json)"
_PAREN = re.compile(r"\(([^)]*)\)(" + _EXT + ")$")
_PLAIN = re.compile(_EXT + "$")


def options(config):
    """`@target: es5, @strict: true` -> {'target=es5', 'strict=true'}, as spelled inside a baseline file name."""
    return {f"{k.lower()}={v.lower()}" for k, v in _PAIR.findall(config or "")}


def baselines_of(suite, listing):
    """Reference files of exactly this suite's configuration: `stem(opts).ext` for a configured one, else `stem.ext`."""
    stem = suite.base.rsplit(".", 1)[0]
    want = options(suite.config)
    out = []
    for name in listing:
        if not name.startswith(stem):
            continue
        rest = name[len(stem) :]
        m = _PAREN.match(rest)
        if m:
            if want and set(re.split(r",(?=\w+=)", m.group(1).lower())) == want:
                out.append(name)
        elif _PLAIN.match(rest) and not want:
            out.append(name)
    return sorted(out)


def make_control(ref, chosen, dest):
    """Corrupt the reference baselines of up to seven suites into `dest`; returns (suites, what was done)."""
    listing = os.listdir(ref)
    picked, done = [], []
    for s in chosen:
        names = baselines_of(s, listing)
        if len(names) < 2 or any(p.key == s.key for p in picked):
            continue
        exts, action = CONTROL_MODES[len(picked)]
        stem = s.base.rsplit(".", 1)[0]
        found = [n for e in exts for n in names if n.endswith(e)]
        if action == "add":
            if any(n.endswith(".errors.txt") for n in names):
                continue
            paren = (
                re.match(re.escape(stem) + r"(\([^)]*\))?\.", names[0]).group(1) or ""
            )
            found = [stem + paren + exts[0]]
        if not found:
            continue
        for n in names:
            shutil.copy(os.path.join(ref, n), os.path.join(dest, n))
        target = found[0]
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
