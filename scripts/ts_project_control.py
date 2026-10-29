"""Control run of scripts/run-ts-project-oracle.py (issue #1285).

When every test of a slice passes, the failing path of the harness is never exercised. The control picks up to five
JSON cases and makes them fail on purpose in a private copy of what they read, so that Node and jsse must fail the
same tests with the same messages and write the same local baselines:

  * the reference baselines of a case (`tests/baselines/reference/project/<case>/{node,amd}/...`): the resolution
    record appended to, an `.errors.txt` rewritten, an emitted `.js` removed, a `.d.ts` appended to;
  * the case file itself with a `bug` key added: the resolution record echoes the case, so it no longer matches.

The private reference directory holds only the baselines of the control cases (all of them, so that the unmodified
tests of those cases pass); the private `tests/cases` links everything of the checkout except the edited case file.
"""

import json
import os
import shutil

CONTROL_CASES = 5
MODES = ("append-json", "replace-errors", "remove-js", "append-dts", "edit-case")


def project_dir(root, stem):
    return os.path.join(root, "project", stem)


def files_of(directory):
    out = []
    for dirpath, _, names in os.walk(directory):
        out += [os.path.relpath(os.path.join(dirpath, n), directory) for n in names]
    return sorted(out)


def pick_file(files, mode):
    """The baseline file (relative to the case's directory) a mode corrupts, else None."""
    wanted = {
        "append-json": lambda f: f.startswith("node/") and f.endswith(".json"),
        "replace-errors": lambda f: f.endswith(".errors.txt"),
        "remove-js": lambda f: f.endswith(".js") and not f.endswith(".d.js"),
        "append-dts": lambda f: f.endswith(".d.ts"),
    }[mode]
    return next((f for f in files if wanted(f)), None)


def make_control(ts_dir, chosen, rest, work):
    """Corrupt what up to CONTROL_CASES cases read; returns (cases, ref dir, cases dir, what was done, tests).

    `chosen` is the slice, `rest` the other cases of the runner (a slice without a baseline of some kind still gets
    that control from them). `tests` is the number of tests the corruption makes fail on a correct engine, at least.
    """
    ref_src = os.path.join(ts_dir, "tests", "baselines", "reference")
    ref, cases = os.path.join(work, "ref"), os.path.join(work, "cases")
    os.makedirs(ref)
    picked, done, expect = [], [], 0
    for mode in MODES[:-1]:
        for c in list(chosen) + list(rest):
            stem = c.base.rsplit(".", 1)[0]
            src = project_dir(ref_src, stem)
            if c in picked or not os.path.isdir(src):
                continue
            files = files_of(src)
            victim = pick_file(files, mode)
            if victim is None:
                continue
            dest = project_dir(ref, stem)
            shutil.copytree(src, dest)
            path = os.path.join(dest, victim)
            if mode == "remove-js":
                os.remove(path)
            else:
                with open(path, "w" if mode == "replace-errors" else "a") as f:
                    f.write("\ncorrupted reference\n")
            picked.append(c)
            done.append(f"{stem}/{victim}: {mode}")
            expect += 1
            break
    edited = next(
        (
            c
            for c in list(chosen) + list(rest)
            if c not in picked
            and os.path.isdir(project_dir(ref_src, c.base.rsplit(".", 1)[0]))
        ),
        None,
    )
    if edited is not None:
        stem = edited.base.rsplit(".", 1)[0]
        shutil.copytree(project_dir(ref_src, stem), project_dir(ref, stem))
        picked.append(edited)
        done.append(f"{edited.base}: edit-case")
        expect += 2
    src_cases = os.path.join(ts_dir, "tests", "cases")
    os.makedirs(os.path.join(cases, "project"))
    for name in os.listdir(src_cases):
        if name != "project":
            os.symlink(os.path.join(src_cases, name), os.path.join(cases, name))
    skip = os.path.basename(edited.file) if edited is not None else None
    for name in os.listdir(os.path.join(src_cases, "project")):
        if name != skip:
            os.symlink(
                os.path.join(src_cases, "project", name),
                os.path.join(cases, "project", name),
            )
    if edited is not None:
        with open(os.path.join(ts_dir, edited.file), encoding="utf-8") as f:
            data = json.load(f)
        data["bug"] = "control"
        with open(os.path.join(cases, "project", skip), "w", encoding="utf-8") as f:
            json.dump(data, f, indent=4)
    return picked, ref, cases, done, expect
