#!/usr/bin/env python3
"""Update README.md with latest test262 results."""

import json
import re
import subprocess
import sys
from pathlib import Path


def main():
    readme = Path("README.md")
    if not readme.exists():
        print("README.md not found", file=sys.stderr)
        sys.exit(1)

    result = subprocess.run(
        ["uv", "run", "python", "scripts/run-test262.py"],
        capture_output=True,
        text=True,
        timeout=7200,
    )

    for line in result.stdout.splitlines():
        if line.startswith("JSON: "):
            data = json.loads(line[6:])
            break
    else:
        print("Could not find JSON output from test runner", file=sys.stderr)
        print(result.stdout[-500:], file=sys.stderr)
        sys.exit(1)

    total = f"{data['total']:,}"
    run = f"{data['run']:,}"
    skip = f"{data['skip']:,}"
    passed = f"{data['pass']:,}"
    fail = f"{data['fail']:,}"
    rate = f"{data['percentage']:.2f}%"

    table = (
        f"| Total Tests | Run     | Skipped | Passing | Failing | Pass Rate |\n"
        f"|-------------|---------|---------|---------|---------|-----------|"
        f"\n| {total:<11} | {run:<7} | {skip:<7} | {passed:<7} | {fail:<7} | {rate:<9} |"
    )

    content = readme.read_text()
    content = re.sub(
        r"\| Total Tests.*?\n\|[-| ]+\n\|[^\n]+",
        table,
        content,
    )
    readme.write_text(content)
    print(f"Updated README.md: {passed} passing ({rate})")


if __name__ == "__main__":
    main()
