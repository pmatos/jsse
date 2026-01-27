#!/usr/bin/env python3
"""Run custom JS tests from the tests/ directory.

Each .js file is executed by jsse. A test passes if exit code is 0.
Tests can use throw to signal failure.
"""

import argparse
import subprocess
import sys
from pathlib import Path


def find_tests(test_dir: Path) -> list[Path]:
    return sorted(test_dir.rglob("*.js"))


def main():
    parser = argparse.ArgumentParser(description="Run custom JSSE tests")
    parser.add_argument(
        "--jsse",
        default="./target/release/jsse",
        help="Path to jsse binary",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=10,
        help="Timeout per test in seconds",
    )
    parser.add_argument(
        "paths",
        nargs="*",
        help="Specific test files or directories to run",
    )
    args = parser.parse_args()

    jsse = Path(args.jsse)
    if not jsse.is_file():
        print(f"Error: jsse binary not found at {jsse}", file=sys.stderr)
        sys.exit(2)

    if args.paths:
        tests = []
        for p in args.paths:
            p = Path(p)
            if p.is_file():
                tests.append(p)
            elif p.is_dir():
                tests.extend(sorted(p.rglob("*.js")))
    else:
        tests = find_tests(Path("tests"))

    if not tests:
        print("No tests found.")
        sys.exit(0)

    passed = 0
    failed = 0
    errors = []

    for test in tests:
        try:
            result = subprocess.run(
                [str(jsse), str(test)],
                timeout=args.timeout,
                capture_output=True,
            )
            if result.returncode == 0:
                passed += 1
            else:
                failed += 1
                stderr = result.stderr.decode("utf-8", errors="replace").strip()
                errors.append((str(test), stderr))
        except subprocess.TimeoutExpired:
            failed += 1
            errors.append((str(test), "TIMEOUT"))

    total = passed + failed
    print(f"\n=== Custom Test Results ===")
    print(f"Total: {total}")
    print(f"Pass:  {passed}")
    print(f"Fail:  {failed}")

    if errors:
        print(f"\nFailed tests:")
        for path, err in errors:
            print(f"  FAIL {path}")
            if err:
                for line in err.split("\n")[:2]:
                    print(f"       {line}")

    sys.exit(0 if failed == 0 else 1)


if __name__ == "__main__":
    main()
