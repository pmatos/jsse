"""Control run of scripts/run-ts-fourslash-oracle.py (issue #1288).

When every test of a slice passes, the failing path of the harness is never exercised. The control picks a few tests
and makes them fail on purpose in a private copy of what they read, so that Node and jsse must fail the same tests
with the same messages and write the same local baselines:

  * a reference baseline (`tests/baselines/reference/<test>.baseline*`, or the tsserver log of the server runner)
    appended to, rewritten or removed: the baseline diff, the `New baseline created` error and the local file;
  * a test file with an assertion appended (`verify.numberOfErrorsInCurrentFile(12345)`,
    `verify.currentFileContentIs("corrupted")`): the harness' own assertion errors.
"""

import os
import shutil

CONTROL_TESTS = 5
BASELINE_MODES = ("append", "replace", "remove")
ASSERTIONS = (
    "verify.numberOfErrorsInCurrentFile(12345);",
    'verify.currentFileContentIs("corrupted");',
)
SERVER_LOG = os.path.join("tsserver", "fourslashServer")


def baseline_files(ref, stem):
    """Relative paths under `ref` of the baselines a test named `stem` compares against (those that exist)."""
    out = []
    for d in ("", SERVER_LOG):
        try:
            names = os.listdir(os.path.join(ref, d))
        except OSError:
            continue
        out += [
            os.path.join(d, n)
            for n in sorted(names)
            if n.startswith(stem + ".baseline") or (d and n == stem + ".js")
        ]
    return out


def make_control(ts_dir, chosen, rest, work):
    """Corrupt what up to CONTROL_TESTS tests read; returns (tests, ref dir, cases dir, what was done).

    `chosen` is the slice, `rest` the other tests of the runner (a slice without any baseline still gets baseline
    controls from them). The private `cases` dir holds fourslash.ts (a link) and the control tests' files, the
    assertion ones edited.
    """
    ref_src = os.path.join(ts_dir, "tests", "baselines", "reference")
    ref, cases = os.path.join(work, "ref"), os.path.join(work, "cases")
    os.makedirs(ref)
    os.makedirs(os.path.join(cases, "fourslash"))

    def case_path(t):
        path = os.path.join(cases, t.file[len("tests/cases/") :])
        os.makedirs(os.path.dirname(path), exist_ok=True)
        return path

    picked, done, modes = [], [], 0
    for t in list(chosen) + list(rest):
        if modes == 3 or t in picked:
            continue
        stem = t.base.rsplit(".", 1)[0]
        found = baseline_files(ref_src, stem)
        if not found:
            continue
        dest = os.path.join(ref, found[0])
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        shutil.copy(os.path.join(ref_src, found[0]), dest)
        mode = BASELINE_MODES[modes]
        modes += 1
        if mode == "remove":
            os.remove(dest)
        else:
            with open(dest, "a" if mode == "append" else "w") as f:
                f.write("\ncorrupted reference\n")
        shutil.copy(os.path.join(ts_dir, t.file), case_path(t))
        picked.append(t)
        done.append(f"{found[0]}: {mode}")
    for t in chosen:
        if len(picked) == CONTROL_TESTS or t in picked:
            continue
        assertion = ASSERTIONS[len(picked) % 2]
        with open(os.path.join(ts_dir, t.file), encoding="utf-8") as f:
            text = f.read()
        with open(case_path(t), "w", encoding="utf-8") as f:
            f.write(text.rstrip("\n") + "\n\n" + assertion + "\n")
        picked.append(t)
        done.append(f"{t.base}: {assertion}")
    os.symlink(
        os.path.join(ts_dir, "tests", "cases", "fourslash", "fourslash.ts"),
        os.path.join(cases, "fourslash", "fourslash.ts"),
    )
    return picked, ref, cases, done
