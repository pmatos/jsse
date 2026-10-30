"""Control run of scripts/run-ts-transpile-oracle.py (issue #1287).

When every test of a slice passes, the failing path of the harness is never exercised. The control picks up to five
test files and makes their tests fail on purpose in a private copy of what they read, so that Node and jsse must fail
the same tests with the same messages and write the same local baselines:

  * the reference baseline of a test (`tests/baselines/reference/transpile/<name>.js` or `.d.ts`) appended to,
    rewritten or removed: the baseline diff, the `New baseline created` error and the local file;
  * a test file with a statement appended (`export const corrupted = 12345;`, `export function corrupted() {
    return 1; }`): the transpiled JS and declaration output no longer equal the reference, which checks that the
    transpile itself runs.
"""

import os
import shutil

import ts_transpile_slices as ts

CONTROL_TESTS = 5
BASELINE_MODES = ("append", "replace", "remove")
APPENDED = (
    "export const corrupted = 12345;",
    "export function corrupted() { return 1; }",
)


def make_control(ts_dir, chosen, rest, work):
    """Corrupt what up to CONTROL_TESTS test files read; returns (tests, ref dir, cases dir, what was done).

    `chosen` is the slice, `rest` the other files of the runner (a slice smaller than five files still gets its
    controls from them). The private reference dir holds `transpile/` with the references of every picked file (the
    first one of each baseline-mode file corrupted); the private cases dir holds `transpile/` with the picked files,
    the edit-mode ones changed.
    """
    ref_src = os.path.join(ts_dir, ts.REFERENCE)
    ref = os.path.join(work, "ref", "transpile")
    cases = os.path.join(work, "cases")
    os.makedirs(ref)
    os.makedirs(os.path.join(cases, "transpile"))

    picked, done = [], []
    for t in list(chosen) + list(rest):
        if len(picked) == CONTROL_TESTS or t in picked:
            continue
        names = [
            n
            for n in map(ts.reference_names, (x.split(" ", 3)[3] for x in t.tests))
            if n
        ]
        names = [n for n in names if os.path.isfile(os.path.join(ref_src, n))]
        if not names:
            continue
        for n in names:
            shutil.copy(os.path.join(ref_src, n), os.path.join(ref, n))
        with open(os.path.join(ts_dir, t.file), encoding="utf-8", newline="") as f:
            text = f.read()
        dest = ts.case_path(cases, t)
        mode = len(picked)
        if mode < len(BASELINE_MODES):
            m = BASELINE_MODES[mode]
            victim = os.path.join(ref, names[0])
            if m == "remove":
                os.remove(victim)
            else:
                with open(victim, "a" if m == "append" else "w", encoding="utf-8") as f:
                    f.write("\ncorrupted reference\n")
            done.append(f"{names[0]}: {m}")
        else:
            extra = APPENDED[mode - len(BASELINE_MODES)]
            text = text.rstrip("\n") + "\n" + extra + "\n"
            done.append(f"{t.base}: {extra}")
        with open(dest, "w", encoding="utf-8", newline="") as f:
            f.write(text)
        picked.append(t)
    return picked, os.path.dirname(ref), cases, done
