#!/usr/bin/env python3
"""Run jsse against test262 tests and report pass/fail/percentage.

Usage:
    uv run python scripts/run-test262.py [options] [path...]

Examples:
    uv run python scripts/run-test262.py
    uv run python scripts/run-test262.py -j 8 --timeout 30
    uv run python scripts/run-test262.py test262/test/language/types/
    uv run python scripts/run-test262.py test262/test/language/types/undefined/S8.1_A1_T1.js
"""

import argparse
import ctypes
import json
import os
import re
import signal
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor, as_completed
from pathlib import Path

_main_pgid = None


def _set_pdeathsig():
    """Set PR_SET_PDEATHSIG so this process dies when its parent dies (Linux only)."""
    try:
        libc = ctypes.CDLL("libc.so.6", use_errno=True)
        libc.prctl(1, signal.SIGTERM)  # PR_SET_PDEATHSIG = 1
    except Exception:
        pass


def _worker_init():
    """Initializer for pool worker processes."""
    _set_pdeathsig()
    signal.signal(signal.SIGTERM, signal.SIG_DFL)
    signal.signal(signal.SIGINT, signal.SIG_DFL)


def _cleanup_handler(signum, frame):
    """Kill entire process group on termination signals."""
    if _main_pgid is not None:
        try:
            os.killpg(_main_pgid, signal.SIGTERM)
        except OSError:
            pass
    os._exit(128 + signum)

FRONTMATTER_RE = re.compile(r"/\*---\s*\n(.*?)\n---\*/", re.DOTALL)

# Simple YAML-enough parsing for test262 frontmatter
FLAGS_RE = re.compile(r"^flags:\s*\[([^\]]*)\]", re.MULTILINE)
INCLUDES_RE = re.compile(r"^includes:\s*\[([^\]]*)\]", re.MULTILINE)
NEGATIVE_PHASE_RE = re.compile(
    r"^negative:\s*\n\s+phase:\s*(\S+)\s*\n\s+type:\s*(\S+)", re.MULTILINE
)
NEGATIVE_SIMPLE_RE = re.compile(r"^negative:", re.MULTILINE)
FEATURES_RE = re.compile(r"^features:\s*\[([^\]]*)\]", re.MULTILINE)


def parse_frontmatter(text: str) -> dict:
    m = FRONTMATTER_RE.search(text)
    if not m:
        return {}

    fm = m.group(1)
    result: dict = {}

    flags_m = FLAGS_RE.search(fm)
    if flags_m:
        result["flags"] = [f.strip() for f in flags_m.group(1).split(",") if f.strip()]

    includes_m = INCLUDES_RE.search(fm)
    if includes_m:
        result["includes"] = [
            i.strip() for i in includes_m.group(1).split(",") if i.strip()
        ]

    neg_m = NEGATIVE_PHASE_RE.search(fm)
    if neg_m:
        result["negative"] = {"phase": neg_m.group(1), "type": neg_m.group(2)}
    elif NEGATIVE_SIMPLE_RE.search(fm):
        result["negative"] = {"phase": "runtime", "type": ""}

    features_m = FEATURES_RE.search(fm)
    if features_m:
        result["features"] = [
            f.strip() for f in features_m.group(1).split(",") if f.strip()
        ]

    return result


def read_harness_file(test262_dir: Path, name: str) -> str:
    path = test262_dir / "harness" / name
    return path.read_text(encoding="utf-8", errors="replace")


def parse_args():
    parser = argparse.ArgumentParser(description="Run test262 suite against jsse")
    parser.add_argument(
        "--jsse",
        default="./target/release/jsse",
        help="Path to jsse binary (default: ./target/release/jsse)",
    )
    parser.add_argument(
        "--test262",
        default="./test262",
        help="Path to test262 directory (default: ./test262)",
    )
    parser.add_argument(
        "-j",
        "--jobs",
        type=int,
        default=max((os.cpu_count() or 2) // 2, 1),
        help="Number of parallel jobs (default: half of nproc)",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=60,
        help="Timeout per test in seconds (default: 60)",
    )
    parser.add_argument(
        "paths",
        nargs="*",
        help="Specific test files or directories to run (default: all tests)",
    )
    return parser.parse_args()


def find_tests(test262_dir: Path, paths: list[str] | None) -> list[Path]:
    if paths:
        tests = []
        for p in paths:
            path = Path(p)
            if path.is_file() and path.suffix == ".js":
                tests.append(path)
            elif path.is_dir():
                tests.extend(sorted(path.rglob("*.js")))
        return sorted(tests)

    test_dir = test262_dir / "test"
    tests = []
    for subdir in ("language", "built-ins", "annexB"):
        d = test_dir / subdir
        if d.is_dir():
            tests.extend(d.rglob("*.js"))
    return sorted(tests)


def get_harness_files(
    metadata: dict, test262_dir: Path, is_module: bool, is_async: bool
) -> list[Path]:
    """Get list of harness files needed for a test."""
    flags = metadata.get("flags", [])
    harness_files = []

    if "raw" not in flags:
        harness_files.append(test262_dir / "harness" / "assert.js")
        harness_files.append(test262_dir / "harness" / "sta.js")

        if is_async:
            harness_files.append(test262_dir / "harness" / "doneprintHandle.js")

        for inc in metadata.get("includes", []):
            harness_files.append(test262_dir / "harness" / inc)

    return harness_files


def build_test_source(
    test_file: Path, metadata: dict, test262_dir: Path, is_module: bool
) -> str:
    """Build the full source to feed to the engine, prepending harness files.
    For modules, we don't concatenate - harness is loaded via --prelude instead."""
    flags = metadata.get("flags", [])
    is_async = "async" in flags

    # For modules, just return the test source (harness loaded via --prelude)
    if is_module:
        return test_file.read_text(encoding="utf-8", errors="replace")

    # For scripts, concatenate harness + test
    parts: list[str] = []
    source = test_file.read_text(encoding="utf-8", errors="replace")

    if "onlyStrict" in flags:
        parts.append('"use strict";\n')

    if "raw" not in flags:
        parts.append(read_harness_file(test262_dir, "assert.js"))
        parts.append(read_harness_file(test262_dir, "sta.js"))

        if is_async:
            parts.append(read_harness_file(test262_dir, "doneprintHandle.js"))

        for inc in metadata.get("includes", []):
            parts.append(read_harness_file(test262_dir, inc))

    parts.append(source)
    return "\n".join(parts)


def run_single_test(
    args: tuple[str, str, int, str],
) -> tuple[str, bool, str]:
    """Run a single test. Returns (test_path, passed, skip_reason)."""
    test_file_str, jsse, timeout, test262_dir_str = args
    test_file = Path(test_file_str)
    test262_dir = Path(test262_dir_str)

    try:
        head = test_file.read_text(encoding="utf-8", errors="replace")[:8192]
    except OSError:
        return (test_file_str, False, "read_error")

    metadata = parse_frontmatter(head)
    flags = metadata.get("flags", [])

    is_module = "module" in flags
    is_async = "async" in flags

    negative = metadata.get("negative")

    def limit_memory():
        import resource
        mem_limit = 512 * 1024 * 1024  # 512 MB
        resource.setrlimit(resource.RLIMIT_AS, (mem_limit, mem_limit))
        _set_pdeathsig()

    # For modules, use --prelude to load harness and run original file
    # For scripts, use combined source in temp file
    import tempfile

    if is_module:
        harness_files = get_harness_files(metadata, test262_dir, is_module, is_async)
        cmd = [jsse]
        for hf in harness_files:
            cmd.extend(["--prelude", str(hf)])
        cmd.append("--module")
        cmd.append(str(test_file))
        tmp_path = None
    else:
        # Build combined source with harness for scripts
        try:
            combined = build_test_source(test_file, metadata, test262_dir, is_module)
        except OSError:
            return (test_file_str, False, "harness_error")

        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".js", delete=False, encoding="utf-8"
        ) as tmp:
            tmp.write(combined)
            tmp_path = tmp.name

        cmd = [jsse, tmp_path]

    try:
        result = subprocess.run(
            cmd,
            timeout=timeout,
            capture_output=True,
            preexec_fn=limit_memory,
        )
        exit_code = result.returncode
    except subprocess.TimeoutExpired:
        if tmp_path:
            os.unlink(tmp_path)
        return (test_file_str, False, "timeout")
    except OSError:
        if tmp_path:
            os.unlink(tmp_path)
        return (test_file_str, False, "exec_error")
    finally:
        if tmp_path:
            try:
                os.unlink(tmp_path)
            except OSError:
                pass

    if negative:
        phase = negative.get("phase", "runtime")
        if phase == "parse":
            passed = exit_code == 2
        else:
            passed = exit_code != 0
    elif is_async:
        stdout = result.stdout.decode("utf-8", errors="replace")
        if "Test262:AsyncTestComplete" in stdout:
            passed = exit_code == 0
        elif "Test262:AsyncTestFailure" in stdout:
            passed = False
        else:
            passed = exit_code == 0
    else:
        passed = exit_code == 0

    return (test_file_str, passed, "")


def main():
    global _main_pgid

    # Create a new process group so we can kill all children at once
    try:
        os.setpgrp()
    except OSError:
        pass
    _main_pgid = os.getpid()

    signal.signal(signal.SIGTERM, _cleanup_handler)
    signal.signal(signal.SIGINT, _cleanup_handler)

    args = parse_args()

    jsse = Path(args.jsse)
    if not jsse.is_file():
        print(f"Error: jsse binary not found at {jsse}", file=sys.stderr)
        print("Build it first: cargo build --release", file=sys.stderr)
        sys.exit(2)

    test262 = Path(args.test262)
    if not (test262 / "test").is_dir():
        print(f"Error: test262 directory not found at {test262}", file=sys.stderr)
        sys.exit(2)

    tests = find_tests(test262, args.paths if args.paths else None)
    total = len(tests)
    if total == 0:
        print("No tests found.")
        sys.exit(1)

    print(
        f"Found {total} tests, running with {args.jobs} workers "
        f"(timeout: {args.timeout}s)...",
        file=sys.stderr,
    )

    passed = 0
    failed = 0
    skipped = 0
    done = 0
    pass_list: list[str] = []
    fail_list: list[str] = []

    work = [
        (str(t), str(jsse.resolve()), args.timeout, str(test262.resolve()))
        for t in tests
    ]

    with ProcessPoolExecutor(max_workers=args.jobs, initializer=_worker_init) as pool:
        futures = {pool.submit(run_single_test, w): w for w in work}
        for future in as_completed(futures):
            test_path, test_passed, skip_reason = future.result()
            done += 1
            if skip_reason.startswith("skip_"):
                skipped += 1
            elif test_passed:
                passed += 1
                pass_list.append(test_path)
            else:
                failed += 1
                fail_list.append(test_path)
            if done % 1000 == 0:
                run = passed + failed
                pct = (passed / run * 100) if run else 0
                print(
                    f"... {done}/{total} ({pct:.1f}% passing of {run} run)",
                    file=sys.stderr,
                )

    run_total = passed + failed
    percentage = (passed / run_total * 100) if run_total else 0

    # Regression detection
    baseline_file = Path("test262-pass.txt")
    regressions: list[str] = []
    new_passes: list[str] = []
    ran_tests = set(pass_list + fail_list)
    is_full_run = not args.paths
    if baseline_file.exists():
        baseline = set(baseline_file.read_text().strip().split("\n"))
        current = set(pass_list)
        # Only check regressions among tests that were actually run
        regressions = sorted((baseline & ran_tests) - current)
        new_passes = sorted(current - baseline)

    print()
    print("=== test262 Results ===")
    print(f"Total:   {total}")
    print(f"Run:     {run_total}")
    print(f"Skip:    {skipped}")
    print(f"Pass:    {passed}")
    print(f"Fail:    {failed}")
    print(f"Rate:    {percentage:.2f}%")

    if regressions:
        print(f"\n!!! REGRESSIONS: {len(regressions)} tests that previously passed now fail:")
        for r in regressions[:20]:
            print(f"  REGRESSED: {r}")
        if len(regressions) > 20:
            print(f"  ... and {len(regressions) - 20} more")

    if new_passes:
        print(f"\nNew passes: {len(new_passes)}")

    # Save current pass list as new baseline (only on full runs)
    if is_full_run:
        pass_list.sort()
        baseline_file.write_text("\n".join(pass_list) + "\n")

    print()
    print(
        f"JSON: {json.dumps({
            'total': total,
            'run': run_total,
            'skip': skipped,
            'pass': passed,
            'fail': failed,
            'percentage': round(percentage, 2),
            'regressions': len(regressions),
            'new_passes': len(new_passes),
        })}"
    )


if __name__ == "__main__":
    main()
