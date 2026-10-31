#!/usr/bin/env python3
"""Check that every Markdown link in AGENTS.md and docs/agents/ resolves (issue #1299).

    python3 scripts/check_agent_docs_links.py [--root DIR] [--strict] [--list-orphans]

A link `[text](target)` is checked unless it is external (`http:`, `https:`, `mailto:`): the file
must exist (relative to the linking file) and, when the target has a `#fragment` into a Markdown
file, a heading of that file must produce the fragment (GitHub's slug rules). Links inside fenced
code blocks and inline code are ignored. Exit 1 on a broken link.

An orphan is a page of docs/agents/ that no chain of links from AGENTS.md reaches. Orphans are
printed as warnings; `--strict` makes them an error, `--list-orphans` prints only their paths.
Standard library only; never writes anything.
"""

import argparse
import os
import re
import sys

FENCE = re.compile(r"^\s*(```|~~~)")
INLINE_CODE = re.compile(r"`+[^`\n]*`+")
LINK = re.compile(
    r"(?<!\!)\[(?:[^\[\]]|\[[^\]]*\])*\]\(\s*<?([^)\s>]+)>?(?:\s+\"[^\"]*\")?\s*\)"
)
HEADING = re.compile(r"^\s{0,3}(#{1,6})\s+(.*?)\s*#*\s*$")
EXTERNAL = re.compile(r"^(?:[a-zA-Z][a-zA-Z0-9+.-]*:|//)")


def slug(text):
    text = re.sub(r"`", "", text)
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = text.strip().lower()
    text = re.sub(r"[^\w\- ]", "", text, flags=re.UNICODE)
    return text.replace(" ", "-")


def prose_lines(text):
    """Yield (line number, line) outside fenced code blocks, inline code removed."""
    fence = None
    for number, line in enumerate(text.splitlines(), 1):
        m = FENCE.match(line)
        if m:
            if fence is None:
                fence = m.group(1)
            elif m.group(1) == fence:
                fence = None
            continue
        if fence is None:
            yield number, INLINE_CODE.sub("", line)


def anchors(text):
    seen = {}
    found = set()
    for _, line in prose_lines(text):
        m = HEADING.match(line)
        if not m:
            continue
        base = slug(m.group(2))
        count = seen.get(base, 0)
        seen[base] = count + 1
        found.add(base if count == 0 else "%s-%d" % (base, count))
    return found


def links(text):
    for number, line in prose_lines(text):
        for m in LINK.finditer(line):
            yield number, m.group(1)


def markdown_files(root):
    out = []
    agents = os.path.join(root, "AGENTS.md")
    if os.path.isfile(agents):
        out.append(agents)
    for base, _, names in sorted(os.walk(os.path.join(root, "docs", "agents"))):
        out.extend(os.path.join(base, n) for n in sorted(names) if n.endswith(".md"))
    return out


def check(root):
    """Return (errors, edges): broken links as strings, and {file: set(linked Markdown files)}."""
    errors, edges, anchor_cache = [], {}, {}

    def anchors_of(path):
        if path not in anchor_cache:
            with open(path, encoding="utf-8") as f:
                anchor_cache[path] = anchors(f.read())
        return anchor_cache[path]

    for path in markdown_files(root):
        rel = os.path.relpath(path, root)
        with open(path, encoding="utf-8") as f:
            text = f.read()
        edges[path] = set()
        for number, target in links(text):
            if EXTERNAL.match(target):
                continue
            file_part, _, fragment = target.partition("#")
            dest = (
                path
                if file_part == ""
                else os.path.normpath(os.path.join(os.path.dirname(path), file_part))
            )
            if file_part != "" and not os.path.exists(dest):
                errors.append("%s:%d: %s does not exist" % (rel, number, target))
                continue
            if dest.endswith(".md") and os.path.isfile(dest):
                edges[path].add(dest)
                if fragment and fragment not in anchors_of(dest):
                    errors.append(
                        "%s:%d: %s has no heading #%s"
                        % (rel, number, os.path.relpath(dest, root), fragment)
                    )
            elif fragment and file_part == "":
                errors.append("%s:%d: no heading #%s" % (rel, number, fragment))
    return errors, edges


def orphans(root, edges):
    start = os.path.join(root, "AGENTS.md")
    reached, todo = set(), [start]
    while todo:
        current = todo.pop()
        if current in reached:
            continue
        reached.add(current)
        todo.extend(edges.get(current, ()))
    agents_dir = os.path.join(root, "docs", "agents") + os.sep
    return sorted(p for p in edges if p.startswith(agents_dir) and p not in reached)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument(
        "--root", default=os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    )
    parser.add_argument(
        "--strict", action="store_true", help="orphan pages are an error"
    )
    parser.add_argument(
        "--list-orphans", action="store_true", help="print only the orphan paths"
    )
    args = parser.parse_args(argv)
    root = os.path.abspath(args.root)
    errors, edges = check(root)
    lost = [os.path.relpath(p, root) for p in orphans(root, edges)]
    if args.list_orphans:
        print("\n".join(lost))
        return 0
    for line in errors:
        print("error: " + line)
    for rel in lost:
        print(
            "%s: %s is not reachable from AGENTS.md"
            % ("error" if args.strict else "warning", rel)
        )
    pages = len(edges)
    print("%d pages, %d broken links, %d orphans" % (pages, len(errors), len(lost)))
    return 1 if errors or (args.strict and lost) else 0


if __name__ == "__main__":
    sys.exit(main())
