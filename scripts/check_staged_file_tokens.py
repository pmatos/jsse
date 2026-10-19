"""Enforce the file token budget on staged blobs or a committed change range."""

import argparse
import os
import subprocess
import sys


MAX_BYTES = 5_000 * 4
IGNORE_ATTRIBUTE = "token-limit-ignore"


def git(*args: str, input_bytes: bytes | None = None) -> bytes:
    return subprocess.run(
        ["git", *args], input=input_bytes, stdout=subprocess.PIPE, check=True
    ).stdout


def base_size(base_rev: str, path: bytes) -> int | None:
    """Size of `path` in `base_rev`, or None when it did not exist there."""
    result = subprocess.run(
        ["git", "cat-file", "-s", f"{base_rev}:{os.fsdecode(path)}"],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return int(result.stdout) if result.returncode == 0 else None


def find_violations(
    candidates: list[tuple[bytes, bytes]], base_rev: str
) -> list[tuple[bytes, int, int | None]]:
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
        # Ratchet: a file that was already over the limit may change as long
        # as it does not grow, so oversized files shrink as they are touched.
        before = base_size(base_rev, path)
        if before is not None and before > MAX_BYTES and byte_count <= before:
            continue
        # Match Git's usual binary heuristic; assets have no text token budget.
        if b"\0" in git("cat-file", "blob", oid.decode("ascii"))[:8000]:
            continue
        violations.append((path, byte_count, before))
    return violations


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--base", help="CI: compare the merge base of this commit and head"
    )
    parser.add_argument("--head", help="CI: inspect this commit (defaults to HEAD)")
    args = parser.parse_args()
    if args.base == "" or args.head == "":
        parser.error("commit references must not be empty")
    if args.head and not args.base:
        parser.error("--head requires --base")
    attribute_source = ["--cached"]
    revisions = ["--cached"]
    base_rev = "HEAD"
    if args.base:
        head = (
            git(
                "rev-parse",
                "--verify",
                "--end-of-options",
                f"{args.head or 'HEAD'}^{{commit}}",
            )
            .decode()
            .strip()
        )
        base = (
            git("rev-parse", "--verify", "--end-of-options", f"{args.base}^{{commit}}")
            .decode()
            .strip()
        )
        base = git("merge-base", base, head).decode().strip()
        revisions = [base, head]
        base_rev = base
        attribute_source = [f"--source={head}"]
    # --no-renames treats a rename as deletion + addition, checking its new path.
    # Raw, NUL-delimited records preserve unusual names and give us blob IDs/modes.
    records = git(
        "diff",
        "--raw",
        "-z",
        "--no-abbrev",
        "--no-renames",
        "--no-relative",
        "--diff-filter=ACMT",
        *revisions,
        "--",
    ).split(b"\0")
    staged = []
    for metadata, path in zip(records[::2], records[1::2]):
        _, mode, _, oid, _ = metadata.split()
        if mode in (b"100644", b"100755"):
            staged.append((path, oid))
    if not staged:
        return 0

    # Exemptions must come from the same snapshot as the inspected blobs.
    attributes = git(
        "check-attr",
        *attribute_source,
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

    violations = find_violations(candidates, base_rev)
    if not violations:
        return 0

    print(
        f"{'CI check failed: changed' if args.base else 'Commit blocked: staged'} "
        "authored files exceed 5,000 tokens "
        "(bytes/4; maximum 20,000 bytes per file).",
        file=sys.stderr,
    )
    for path, byte_count, before in violations:
        was = f" (was {before:,})" if before is not None and before > MAX_BYTES else ""
        print(
            f"  {os.fsdecode(path)!r}: {byte_count:,} bytes{was} = "
            f"{byte_count / 4:,.2f} tokens",
            file=sys.stderr,
        )
    print(
        "\nNew files must stay at or below the limit, and a file that is already "
        "over it may change\nonly if it does not grow. Split each listed file "
        "before committing:\n"
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
