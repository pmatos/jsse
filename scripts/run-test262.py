#!/usr/bin/env python3
"""Run jsse against test262 tests and report pass/fail/percentage.

Usage:
    ./scripts/run-test262.py [options]

Examples:
    ./scripts/run-test262.py
    ./scripts/run-test262.py -j 8 --timeout 30
    ./scripts/run-test262.py --jsse ./target/debug/jsse
"""

import argparse
import json
import os
import re
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor, as_completed
from pathlib import Path

FRONTMATTER_RE = re.compile(r"^---\s*\n(.*?)\n---\s*\n", re.DOTALL)
NEGATIVE_RE = re.compile(r"^negative:", re.MULTILINE)


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
    return parser.parse_args()


def find_tests(test262_dir: Path) -> list[Path]:
    test_dir = test262_dir / "test"
    tests = []
    for subdir in ("language", "built-ins", "annexB"):
        d = test_dir / subdir
        if d.is_dir():
            tests.extend(d.rglob("*.js"))
    return sorted(tests)


def is_negative_test(test_file: Path) -> bool:
    try:
        with open(test_file, "r", encoding="utf-8", errors="replace") as f:
            head = f.read(4096)
    except OSError:
        return False
    m = FRONTMATTER_RE.match(head)
    if m:
        return bool(NEGATIVE_RE.search(m.group(1)))
    return False


def run_single_test(args: tuple[Path, str, int]) -> tuple[str, bool]:
    """Run a single test. Returns (test_path, passed)."""
    test_file, jsse, timeout = args
    negative = is_negative_test(test_file)

    try:
        result = subprocess.run(
            [jsse, str(test_file)],
            timeout=timeout,
            capture_output=True,
        )
        exit_code = result.returncode
    except subprocess.TimeoutExpired:
        exit_code = -1
    except OSError:
        exit_code = -2

    if negative:
        passed = exit_code != 0
    else:
        passed = exit_code == 0

    return (str(test_file), passed)


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

    tests = find_tests(test262)
    total = len(tests)
    if total == 0:
        print("No tests found.")
        sys.exit(1)

    print(f"Found {total} tests, running with {args.jobs} workers (timeout: {args.timeout}s)...", file=sys.stderr)

    passed = 0
    failed = 0
    done = 0

    work = [(t, str(jsse.resolve()), args.timeout) for t in tests]

    with ProcessPoolExecutor(max_workers=args.jobs) as pool:
        futures = {pool.submit(run_single_test, w): w for w in work}
        for future in as_completed(futures):
            _, test_passed = future.result()
            done += 1
            if test_passed:
                passed += 1
            else:
                failed += 1
            if done % 1000 == 0:
                pct = (passed / done) * 100
                print(f"... {done}/{total} ({pct:.1f}% passing so far)", file=sys.stderr)

    percentage = (passed / total) * 100

    print()
    print("=== test262 Results ===")
    print(f"Total:   {total}")
    print(f"Pass:    {passed}")
    print(f"Fail:    {failed}")
    print(f"Rate:    {percentage:.2f}%")
    print()
    print(f'JSON: {json.dumps({"total": total, "pass": passed, "fail": failed, "percentage": round(percentage, 2)})}')


if __name__ == "__main__":
    main()
