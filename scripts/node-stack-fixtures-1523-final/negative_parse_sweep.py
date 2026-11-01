"""Differential sweep of test262's negative parse tests under `--node` (#1523).

`extract TEST262 OUT` writes OUT/sources.tsv (one `goal<TAB>JSON source` line per
test of test/language whose frontmatter says `phase: parse, type: SyntaxError`) and
OUT/paths.txt (the test of each line). A test runs once, as written: `onlyStrict`
sources get a `"use strict";` line, `module` ones run as modules.

`compare OUT` reads OUT/node.txt and OUT/jsse.txt, the output of
scripts/node-stack-fixtures-1523-tail/syntax-tail.mjs over sources.tsv on Node and on
`jsse --node`, and prints how many tests differ, how many of those Node accepts, and
the classes of the rest (Node's text, jsse's text, a count and one test each).
"""

import collections
import json
import re
import sys
from pathlib import Path

NEGATIVE = re.compile(r"negative:\s*\n\s*phase:\s*parse\s*\n\s*type:\s*SyntaxError")


def extract(test262: Path, out: Path) -> int:
    rows = []
    for path in sorted((test262 / "test" / "language").rglob("*.js")):
        if "_FIXTURE" in path.name:
            continue
        text = path.read_text(encoding="utf8")
        match = re.search(r"/\*---(.*?)---\*/", text, re.S)
        if not match or not NEGATIVE.search(match.group(1)):
            continue
        flags_match = re.search(r"flags:\s*\[(.*?)\]", match.group(1))
        flags = (
            [f.strip() for f in flags_match.group(1).split(",")] if flags_match else []
        )
        if "module" in flags:
            rows.append((path, "module", text))
        elif "onlyStrict" in flags:
            rows.append((path, "script", '"use strict";\n' + text))
        else:
            rows.append((path, "script", text))
    out.mkdir(parents=True, exist_ok=True)
    with (out / "sources.tsv").open("w") as tsv, (out / "paths.txt").open("w") as paths:
        for path, goal, source in rows:
            tsv.write(f"{goal}\t{json.dumps(source)}\n")
            paths.write(f"{path}\n")
    return len(rows)


def outcome(line: str) -> str:
    return line.rsplit('" ', 1)[1]


def compare(out: Path) -> None:
    node = (out / "node.txt").read_text().split("\n")[:-1]
    jsse = (out / "jsse.txt").read_text().split("\n")[:-1]
    paths = (out / "paths.txt").read_text().split("\n")[:-1]
    classes = collections.defaultdict(list)
    accepted = 0
    for want, got, path in zip(node, jsse, paths, strict=True):
        if want == got:
            continue
        if outcome(want) in ("parses", "loads"):
            accepted += 1
        classes[(outcome(want), outcome(got))].append(path)
    differing = sum(len(paths) for paths in classes.values())
    print(f"{differing} differ, {accepted} of them accepted by Node")
    for (want, got), tests in sorted(classes.items(), key=lambda kv: -len(kv[1])):
        print(f"{len(tests)} | {want} | {got} | {tests[0]}")


if __name__ == "__main__":
    mode, *args = sys.argv[1:]
    if mode == "extract":
        print(extract(Path(args[0]), Path(args[1])))
    else:
        compare(Path(args[0]))
