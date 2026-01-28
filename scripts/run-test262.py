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
import json
import os
import re
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor, as_completed
from pathlib import Path

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
        default=os.cpu_count() or 1,
        help="Number of parallel jobs (default: nproc)",
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


def build_test_source(
    test_file: Path, metadata: dict, test262_dir: Path
) -> str:
    """Build the full source to feed to the engine, prepending harness files."""
    parts: list[str] = []

    flags = metadata.get("flags", [])

    if "raw" not in flags:
        parts.append(read_harness_file(test262_dir, "assert.js"))
        parts.append(read_harness_file(test262_dir, "sta.js"))

        for inc in metadata.get("includes", []):
            parts.append(read_harness_file(test262_dir, inc))

    source = test_file.read_text(encoding="utf-8", errors="replace")

    if "onlyStrict" in flags:
        source = '"use strict";\n' + source

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

    # Skip module tests for now (engine doesn't support modules yet)
    if "module" in flags:
        return (test_file_str, False, "skip_module")

    # Skip async tests for now
    if "async" in flags:
        return (test_file_str, False, "skip_async")

    negative = metadata.get("negative")

    # Build combined source with harness
    try:
        combined = build_test_source(test_file, metadata, test262_dir)
    except OSError:
        return (test_file_str, False, "harness_error")

    # Write combined source to temp file and run
    import tempfile

    with tempfile.NamedTemporaryFile(
        mode="w", suffix=".js", delete=False, encoding="utf-8"
    ) as tmp:
        tmp.write(combined)
        tmp_path = tmp.name

    try:
        result = subprocess.run(
            [jsse, tmp_path],
            timeout=timeout,
            capture_output=True,
        )
        exit_code = result.returncode
    except subprocess.TimeoutExpired:
        return (test_file_str, False, "timeout")
    except OSError:
        return (test_file_str, False, "exec_error")
    finally:
        os.unlink(tmp_path)

    if negative:
        phase = negative.get("phase", "runtime")
        if phase == "parse":
            passed = exit_code == 2
        else:
            passed = exit_code != 0
    else:
        passed = exit_code == 0

    return (test_file_str, passed, "")


def main():
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

    with ProcessPoolExecutor(max_workers=args.jobs) as pool:
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
