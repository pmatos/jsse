# Plan: issue #559 — find_tests silently drops tests in directories it cannot read

## 1. Problem restated

`find_tests` in `scripts/run-test262.py` collects test files with `Path.rglob("*.js")`
(and the `.mjs`-guard helper `_uncollected_mjs` collects with `Path.rglob("*.mjs")`).
`Path.rglob` silently omits any subtree it cannot `os.scandir` — a directory with mode
`0o000`, or one that becomes unreadable mid-walk — and raises nothing. The three
affected call sites are `_uncollected_mjs` (line 735), the selected-directory branch of
`find_tests` (lines 862–867), and the default corpus walk over
`language/built-ins/annexB/intl402` (lines 874–879). Any unreadable subtree under
`test262/test` therefore shrinks the collected test set with no diagnostic: the run
reports a smaller denominator and a pass rate computed over it, exactly the
silent-denominator corruption #546 fixed for the scratch-file sweep but left
unaddressed for collection itself. #546 already established the fix shape for this
exact failure mode (`sweep_scratch_files`'s `_scan`, `SweepResult.unreadable`): walk with
`os.walk(..., onerror=...)` instead of `rglob`, collect what could not be read, and fail
loudly instead of returning a quietly-smaller list.

## 2. Spec basis

N/A: no JavaScript behavior change. This is test-runner tooling under `scripts/` — it
does not touch the parser, interpreter, or any built-in; it only changes how the Python
test harness enumerates `.js`/`.mjs` files on disk before invoking the engine.

## 3. Files to touch

- `scripts/run-test262.py` — add a shared directory-walk helper used by all three
  affected call sites; wire each call site to fail loudly on an unreadable subtree.
- `scripts/test_run_test262.py` — new tests covering all three call sites.

No `src/` changes, no `docs/adr/` entry (this reproduces an already-accepted pattern
from #546, not a new architectural decision), no `CONTEXT.md` change (no new
vocabulary — `unreadable` and the walk-with-`onerror` shape are already established by
`SweepResult`/`sweep_scratch_files`).

## 4. Design

Add two small helpers near `_uncollected_mjs` (around line 730), reusing the exact
`os.walk(..., onerror=...)` shape already proven in `sweep_scratch_files`'s inner `_scan`
(lines 806–819), but generalized to any suffix and returning the unreadable list instead
of appending it to a closure:

```python
def _walk_matching(directory: Path, suffix: str) -> tuple[list[Path], list[tuple[Path, OSError]]]:
    """Collect files ending in `suffix` under directory.

    `Path.rglob` swallows a directory it cannot read and silently omits that
    subtree; `os.walk`'s `onerror` callback at least surfaces it, mirroring
    `sweep_scratch_files`'s walk.
    """
    unreadable: list[tuple[Path, OSError]] = []
    found: list[Path] = []
    for dirpath, _dirnames, filenames in os.walk(
        directory, onerror=lambda e: unreadable.append((Path(e.filename), e))
    ):
        found.extend(Path(dirpath) / name for name in filenames if name.endswith(suffix))
    return found, unreadable


def _raise_if_unreadable(unreadable: list[tuple[Path, OSError]]) -> None:
    if not unreadable:
        return
    formatted = "\n  ".join(f"{path}: {error}" for path, error in unreadable)
    noun = "directory" if len(unreadable) == 1 else "directories"
    raise TestCollectionError(
        f"could not scan the following {noun} while collecting tests:\n  {formatted}"
    )
```

`TestCollectionError` (already defined at line 730, "Raised when a selected path
contains a test the runner would omit") is reused rather than adding a new exception
type — an unreadable subtree is exactly that: a test the runner would silently omit.
Reusing it means `main()`'s existing `except TestCollectionError` block around the
`find_tests` call (lines 975–979) needs **no changes** — it already prints
`f"Error: {error}"` to stderr and exits 2, which is the correct "fail loudly" behavior
the issue asks for.

Call site changes:

1. **`_uncollected_mjs`** (line 734–736): replace `path.rglob("*.mjs")` with
   `_walk_matching(path, ".mjs")` for the directory case, call `_raise_if_unreadable`
   before filtering.
   ```python
   def _uncollected_mjs(path: Path) -> list[Path]:
       if path.is_file():
           candidates = [path]
       elif path.is_dir():
           candidates, unreadable = _walk_matching(path, ".mjs")
           _raise_if_unreadable(unreadable)
       else:
           candidates = []
       return [f for f in candidates if f.suffix == ".mjs" and not _is_fixture(f)]
   ```
   The `elif path.is_dir()` guard matters: today `Path("typo").rglob(...)` on a
   nonexistent path silently returns `[]` (pathlib bails when the parent isn't a
   directory), and `_walk_matching`'s `os.walk` would instead call `onerror` with
   `FileNotFoundError` — a permission-flavored "could not scan" message for what is
   actually a typo, which is a behavior change this issue doesn't ask for. Guarding on
   `is_dir()` keeps the nonexistent-path case exactly as it behaves today.

2. **`find_tests` selected-directory branch** (lines 862–867): replace
   `path.rglob("*.js")` with `_walk_matching(path, ".js")` + `_raise_if_unreadable`.
   ```python
   elif path.is_dir():
       found, unreadable = _walk_matching(path, ".js")
       _raise_if_unreadable(unreadable)
       tests.extend(f for f in found if not _is_fixture(f) and not _is_scratch(f))
   ```

3. **`find_tests` default corpus walk** (lines 874–879): accumulate unreadable
   directories across all four subdirs and raise once after the loop, so one bad
   subtree under `built-ins` doesn't stop `language`/`annexB`/`intl402` from being
   checked too (a single combined error is more useful than exiting on the first hit).
   ```python
   test_dir = test262_dir / "test"
   tests = []
   unreadable: list[tuple[Path, OSError]] = []
   for subdir in ("language", "built-ins", "annexB", "intl402"):
       d = test_dir / subdir
       if d.is_dir():
           found, sub_unreadable = _walk_matching(d, ".js")
           unreadable.extend(sub_unreadable)
           tests.extend(f for f in found if not _is_fixture(f) and not _is_scratch(f))
   _raise_if_unreadable(unreadable)
   return sorted(tests)
   ```

Note on ordering: in the selected-directory branch, `_uncollected_mjs` (via
`_raise_for_uncollected_mjs`, called at line 853, before the branch at line 862) walks
the same tree first. An unreadable subtree under a *selected* directory is therefore
observed via call site 1 before call site 2 is ever reached — both call sites still get
their own independent fix (each walks its own suffix), but a black-box test against a
selected directory cannot distinguish which one fired. The plan's test slices account
for this by testing call site 1 in isolation via a direct call, not only through
`find_tests`.

## 5. TDD slices

All new tests go in `scripts/test_run_test262.py`. Each is written first (red against
current `rglob`-based code), then made to pass by the corresponding piece of section 4.
Follow the existing `locked.chmod(0o000)` / `try...finally: locked.chmod(mode)` pattern
already used in `test_clean_scratch_exits_nonzero_when_a_directory_cannot_be_scanned`
(line 294), including the `if os.geteuid() == 0: self.skipTest(...)` guard — root
ignores directory permission bits, so the test must skip rather than false-negative
under a root test runner (e.g. inside a container).

1. **Call site 1 in isolation — `_uncollected_mjs`.**
   New test (e.g. `test_uncollected_mjs_reports_unreadable_directory`) in
   `RunTest262ExitStatusTests`: create `test262-extra/locked/` with a throwaway file
   inside, `chmod(0o000)` the `locked` dir, call `runner._uncollected_mjs(self.root / "test262-extra")`
   directly (no subprocess), assert it raises `runner.TestCollectionError` with
   "could not scan" in the message, restore the mode in `finally`.
   Red: today `_uncollected_mjs` returns `[]` (silently drops the unreadable subtree).
   Green: after wiring `_walk_matching`/`_raise_if_unreadable` into `_uncollected_mjs`.

2. **Call site 2 — `find_tests` selected-directory branch.**
   Per the ordering note in section 4, `_uncollected_mjs` runs before the `.js` walk on
   the same tree, so a naive test here would go green the moment slice 1 lands even if
   lines 862–867 were never touched — it would not pin call site 2 at all. Neutralize
   the guard with `unittest.mock.patch.object(runner, "_uncollected_mjs", return_value=[])`
   (`runner` is a plain module object from `_load_runner()`, so `patch.object` works;
   add `from unittest import mock` or `import unittest.mock` to the test file's
   imports). New test (e.g.
   `test_find_tests_selected_directory_reports_unreadable_directory`): same
   `test262-extra/locked/` fixture, and inside the `mock.patch.object(...)` context call
   `runner.find_tests(self.root / "test262", [str(self.root / "test262-extra")])`
   directly, assert it raises `TestCollectionError`.
   Red: today returns a `.js` list that silently excludes anything under `locked/`.
   Green: after wiring the selected-directory branch (lines 862–867) specifically.

3. **Call site 3 — `find_tests` default corpus walk.**
   New test (e.g. `test_find_tests_default_corpus_reports_unreadable_directory`):
   create `test262/test/language/locked/` (mirroring the corpus shape the real
   submodule has), `chmod(0o000)`, call `runner.find_tests(self.root / "test262", None)`
   directly, assert it raises `TestCollectionError`.
   Red: today returns a `.js` list silently missing `language/locked/`'s contents.
   Green: after wiring the default-corpus loop.

4. **End-to-end CLI confirmation.**
   New test (e.g. `test_run_reports_unreadable_directory_and_exits_nonzero`) using the
   existing `run_runner` subprocess helper with `paths=()` (so `find_tests` takes the
   default-corpus branch) and the same locked-subdir fixture as slice 3. Assert
   `result.returncode == 2` and `"could not scan"` in `result.stderr` — this is the
   user-visible contract the issue actually asks for ("fail loudly rather than quietly
   running a subset"), exercised through the real CLI entry point and the existing
   `except TestCollectionError` handler in `main()`, not just the internal function.

Run the suite with `uv run python -m unittest discover -s scripts -p 'test_*.py'`
(the same invocation `.github/workflows/ci.yml` uses) after each slice.

## 6. Test surface

No `test262/test/...` directories are relevant — this changes no engine behavior, only
Python test-collection tooling. No `test262-extra/` or `tests/` additions either, for
the same reason (those exercise *engine* spec compliance, not the harness). The gate for
this change is `uv run python -m unittest discover -s scripts -p 'test_*.py'`
(`scripts/test_run_test262.py`), matching `.github/workflows/ci.yml`'s `unittest`
job. No `cargo build`/`cargo test`/test262 run is needed since no Rust source changes.

## 7. Regression risk

Low, and confined to the Python harness:

- **Behavioral risk**: `_walk_matching` must return exactly the same file set as the
  `rglob` it replaces when every directory *is* readable — same suffix filter, same
  recursion into all subdirectories. `os.walk` and `Path.rglob` both do a full recursive
  traversal by default, so this is a like-for-like swap with an added error channel, not
  a change to which files get selected in the readable case. The existing
  `ScratchFileTests`/`RunTest262ExitStatusTests` tests around `find_tests` (fixture
  filtering, scratch-file filtering, `.mjs` guard, sample/glob selection) must keep
  passing unchanged — they're the regression net for "still collects the same tests
  when nothing is unreadable."
- **No engine involvement**: nothing in `src/` changes, so `test262-pass.txt` cannot
  move and none of the tree-walker/property-MOP/GC/bytecode/library-harness machinery
  named in the planning brief is touched.
- **CI environment**: GitHub Actions runners are non-root, so the new `chmod(0o000)`
  tests will actually exercise the unreadable path there; the `os.geteuid() == 0` guard
  protects any local/containerized run as root from a false failure, matching the
  existing sibling test's precedent exactly.

## 8. Out of scope

- Unifying `sweep_scratch_files`'s bespoke inner `_scan` (lines 806–819) with the new
  `_walk_matching` helper. They now share the same `os.walk(..., onerror=...)` shape,
  and a follow-up could de-duplicate them, but `_scan` also filters on `SCRATCH_PREFIX`
  plus fixed `.js` suffix and folds `unreadable` into a different result type
  (`SweepResult` vs raising `TestCollectionError`); collapsing them is a refactor with
  its own risk surface, not needed to close this issue, and is exactly the kind of
  bundled cleanup the constraints ask to leave out.
- Any change to how `main()` reports `TestCollectionError` (formatting, exit code,
  wording beyond what's already there) — the existing handler already does the right
  thing and needs no change.
- Rolling `test262-pass.txt` forward — not applicable regardless, since this PR changes
  no engine behavior.
- Failing loudly on a selected path that doesn't exist at all (typo'd path silently
  collecting zero tests). Same silent-denominator family as #559, but a distinct
  failure mode (missing path vs. unreadable path) with its own precedent worth
  following deliberately (`--clean-scratch`'s explicit `missing = [r for r in roots if
  not r.exists()]` check in `main()`, lines 928–932) rather than folding in here.
