#!/usr/bin/env python3
"""Update or validate the managed test262 progress table in README.md."""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path


README_PATH = Path("README.md")
RUNNER_COMMAND = ["uv", "run", "python", "scripts/run-test262.py"]
TABLE_HEADER = "| Test Files | Scenarios | Passing | Failing | Pass Rate |"
SECTION_RE = re.compile(
    r"(?P<prefix>## Test262 Progress\s*\n\s*\n)"
    r"(?P<table>\|[^\n]+\|\n\|[^\n]+\|\n\|[^\n]+\|)",
    re.MULTILINE,
)
DATA_ROW_RE = re.compile(
    r"^\|\s*(?P<files>[\d,]+)\s*\|"
    r"\s*(?P<scenarios>[\d,]+)\s*\|"
    r"\s*(?P<pass>[\d,]+)\s*\|"
    r"\s*(?P<fail>[\d,]+)\s*\|"
    r"\s*(?P<percentage>[\d.]+%)\s*\|$"
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--check",
        action="store_true",
        help="Validate README.md instead of rewriting it.",
    )
    parser.add_argument(
        "--from-readme",
        action="store_true",
        help="Use the current README table values as the data source. Useful for CI format checks.",
    )
    parser.add_argument(
        "runner_args",
        nargs=argparse.REMAINDER,
        help="Optional args passed through to scripts/run-test262.py after '--'.",
    )
    return parser.parse_args()


def fail(message: str) -> "NoReturn":
    print(message, file=sys.stderr)
    raise SystemExit(1)


def load_readme() -> str:
    if not README_PATH.exists():
        fail("README.md not found")
    return README_PATH.read_text()


def find_managed_table(content: str) -> tuple[re.Match[str], str]:
    match = SECTION_RE.search(content)
    if not match:
        fail("Could not find the managed '## Test262 Progress' table in README.md")
    table = match.group("table")
    lines = table.splitlines()
    if len(lines) != 3 or lines[0].strip() != TABLE_HEADER:
        fail("README.md Test262 Progress table does not match the expected format")
    return match, table


def parse_readme_table(content: str) -> dict[str, int | float]:
    _, table = find_managed_table(content)
    data_line = table.splitlines()[2]
    match = DATA_ROW_RE.match(data_line)
    if not match:
        fail("README.md Test262 Progress data row does not match the expected format")
    percentage = match.group("percentage").rstrip("%")
    return {
        "files": int(match.group("files").replace(",", "")),
        "scenarios": int(match.group("scenarios").replace(",", "")),
        "pass": int(match.group("pass").replace(",", "")),
        "fail": int(match.group("fail").replace(",", "")),
        "percentage": float(percentage),
    }


def run_test262(args: list[str]) -> dict[str, int | float]:
    command = RUNNER_COMMAND + args
    result = subprocess.run(
        command,
        capture_output=True,
        text=True,
        timeout=7200,
    )
    if result.returncode != 0:
        if result.stdout:
            print(result.stdout, file=sys.stderr, end="")
        if result.stderr:
            print(result.stderr, file=sys.stderr, end="")
        fail("test262 runner failed")

    for line in result.stdout.splitlines():
        if line.startswith("JSON: "):
            data = json.loads(line[6:])
            break
    else:
        fail("Could not find JSON output from the test262 runner")

    return {
        "files": data["files"],
        "scenarios": data["scenarios"],
        "pass": data["pass"],
        "fail": data["fail"],
        "percentage": data["percentage"],
    }


def render_table(data: dict[str, int | float]) -> str:
    files = f"{data['files']:,}"
    scenarios = f"{data['scenarios']:,}"
    passed = f"{data['pass']:,}"
    failed = f"{data['fail']:,}"
    percentage = f"{float(data['percentage']):.2f}%"
    return "\n".join(
        [
            TABLE_HEADER,
            "|------------|-----------|---------|---------|-----------|",
            f"| {files:<10} | {scenarios:<9} | {passed:<7} | {failed:<7} | {percentage:<9} |",
        ]
    )


def update_content(content: str, table: str) -> str:
    match, current = find_managed_table(content)
    if current == table:
        return content
    return content[: match.start("table")] + table + content[match.end("table") :]


def main() -> None:
    args = parse_args()
    readme = load_readme()
    if args.from_readme:
        data = parse_readme_table(readme)
    else:
        runner_args = args.runner_args
        if runner_args and runner_args[0] == "--":
            runner_args = runner_args[1:]
        data = run_test262(runner_args)

    expected_content = update_content(readme, render_table(data))

    if args.check:
        if expected_content != readme:
            fail("README.md Test262 Progress table is out of date")
        print("README.md Test262 Progress table is up to date")
        return

    README_PATH.write_text(expected_content)
    print(
        "Updated README.md: "
        f"{data['pass']:,} passing ({float(data['percentage']):.2f}%)"
    )


if __name__ == "__main__":
    main()
