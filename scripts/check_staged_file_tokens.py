"""Enforce the file token budget against Git's staged blobs only."""

import os
import subprocess
import sys


MAX_BYTES = 5_000 * 4
IGNORE_ATTRIBUTE = "token-limit-ignore"


def git(*args: str, input_bytes: bytes | None = None) -> bytes:
    return subprocess.run(
        ["git", *args], input=input_bytes, stdout=subprocess.PIPE, check=True
    ).stdout


def main() -> int:
    # --no-renames treats a rename as deletion + addition, checking its new path.
    # Raw, NUL-delimited records preserve unusual names and give us blob IDs/modes.
    records = git(
        "diff",
        "--cached",
        "--raw",
        "-z",
        "--no-abbrev",
        "--no-renames",
        "--diff-filter=ACMT",
    ).split(b"\0")
    staged = []
    for metadata, path in zip(records[::2], records[1::2]):
        _, mode, _, oid, _ = metadata.split()
        if mode in (b"100644", b"100755"):
            staged.append((path, oid))
    if not staged:
        return 0

    # Read exemptions from the index too: unstaged attributes cannot bypass it.
    attributes = git(
        "check-attr",
        "--cached",
        "-z",
        "--stdin",
        IGNORE_ATTRIBUTE,
        input_bytes=b"\0".join(path for path, _ in staged) + b"\0",
    ).split(b"\0")
    ignored = {
        path
        for path, _, value in zip(attributes[::3], attributes[1::3], attributes[2::3])
        if value in (b"set", b"true")
    }
    candidates = [(path, oid) for path, oid in staged if path not in ignored]
    if not candidates:
        return 0

    sizes = git(
        "cat-file",
        "--batch-check=%(objectsize)",
        input_bytes=b"\n".join(oid for _, oid in candidates) + b"\n",
    ).splitlines()
    violations = []
    for (path, oid), size in zip(candidates, sizes):
        byte_count = int(size)
        if byte_count <= MAX_BYTES:
            continue
        # Match Git's usual binary heuristic; assets have no text token budget.
        if b"\0" in git("cat-file", "blob", oid.decode("ascii"))[:8000]:
            continue
        violations.append((path, byte_count))
    if not violations:
        return 0

    print(
        "Commit blocked: staged authored files exceed 5,000 tokens "
        "(bytes/4; maximum 20,000 bytes per file).",
        file=sys.stderr,
    )
    for path, byte_count in violations:
        print(
            f"  {os.fsdecode(path)!r}: {byte_count:,} bytes = "
            f"{byte_count / 4:,.2f} tokens",
            file=sys.stderr,
        )
    print(
        "\nSplit each listed file before committing:\n"
        "  1. Group related functions, types, tests, or documentation by "
        "responsibility.\n"
        "  2. Move cohesive groups into focused sibling files, keeping EVERY "
        "file at or below 20,000 bytes.\n"
        "     Rust: extract child modules (e.g. parser/expressions.rs), declare "
        "them with mod, and update use paths/visibility.\n"
        "     Tests: move related cases into separate test files. "
        "Docs: extract sections into linked documents.\n"
        "  3. Keep a small entry module/API, update imports and callers, "
        "and run the relevant checks to preserve behavior.\n"
        "  4. git add the original and extracted files (including deletions), "
        "then retry the commit.\n"
        "Only generated or vendored files belong in .gitattributes with "
        "token-limit-ignore; do not exempt authored code.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (subprocess.CalledProcessError, ValueError) as error:
        print(f"Staged-file token check failed: {error}", file=sys.stderr)
        sys.exit(1)
