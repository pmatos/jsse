"""Storage format and ratchet of the jsse pass list of TypeScript's own test suite (issue #1296).

Standard library only: scripts/check_ts_pass_list_ratchet.py (the pre-commit hook and the CI check) imports it.

The pass list is not a list of 106k ids (21.8 MB; the token and large-file hooks forbid that). A test id is the id of
ts-suite/oracle.json (`mocha parent full title :: test title`, whitespace normalised), the ids of a runner are the tests
Node passes (ts-suite/oracle.json pins their sha256), and the ids are grouped in buckets: the first letter of the file
name for compiler, fourslash and fourslash-server, the directory for conformance, the unit-test suite, the file-name
family for project, `all` for transpile. `ts-suite/jsse-pass-1296.json` records, per bucket, the number of tests and the
first 128 bits of the sha256 of its sorted ids, as a gate run on jsse found them. The jsse pass list of a recorded bucket is every id of that
bucket (regenerated from the checkout, checked against the digest) except the ids of
`ts-suite/pass-list-1296-not-passing.txt` (`runner TAB bucket TAB id TAB issue`, tests Node passes and jsse fails).
"""

import hashlib
import json
import re
import subprocess

BASELINE = "ts-suite/jsse-pass-1296.json"
NOT_PASSING = "ts-suite/pass-list-1296-not-passing.txt"
SCHEMA = 1
DIGEST_LEN = 32
HEX_DIGEST = re.compile(rf"^[0-9a-f]{{{DIGEST_LEN}}}$")
NP_HEAD = (
    "# Tests that pass on Node and fail on jsse --node inside a recorded bucket of ts-suite/jsse-pass-1296.json\n"
    "# (scripts/run-ts-pass-list.py): runner TAB bucket TAB id TAB issue. Entries may only be removed.\n"
)
_FIELD_ESCAPES = {"\\": "\\\\", "\n": "\\n", "\r": "\\r", "\t": "\\t"}
_FIELD_UNESCAPES = {"\\": "\\", "n": "\n", "r": "\r", "t": "\t"}


def sha256_lines(ids):
    """sha256 of the sorted, escaped ids, one per line: the digest scheme of ts-suite/oracle.json."""
    lines = sorted(
        i.replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r") for i in ids
    )
    return hashlib.sha256("".join(line + "\n" for line in lines).encode()).hexdigest()


def escape_field(text):
    return "".join(_FIELD_ESCAPES.get(c, c) for c in text)


def unescape_field(text):
    out, i = [], 0
    while i < len(text):
        if text[i] == "\\" and i + 1 < len(text) and text[i + 1] in _FIELD_UNESCAPES:
            out.append(_FIELD_UNESCAPES[text[i + 1]])
            i += 2
        else:
            out.append(text[i])
            i += 1
    return "".join(out)


def bucket_digest(ids):
    """The digest recorded for a bucket: the first 128 bits of the sha256 of its ids."""
    return sha256_lines(ids)[:DIGEST_LEN]


def empty_baseline(pin):
    return {
        "schema": SCHEMA,
        "typescript": {"tag": pin["tag"], "commit": pin["commit"]},
        "jsse": "",
        "buckets": {},
    }


def parse_baseline(text):
    """The baseline document of `text`; ValueError when it is not a well-formed one."""
    try:
        doc = json.loads(text)
    except ValueError as e:
        raise ValueError(f"not JSON: {e}") from e
    if not isinstance(doc, dict) or doc.get("schema") != SCHEMA:
        raise ValueError(f"schema is not {SCHEMA}")
    ts = doc.get("typescript")
    if not (
        isinstance(ts, dict)
        and isinstance(ts.get("tag"), str)
        and isinstance(ts.get("commit"), str)
    ):
        raise ValueError("typescript.tag and typescript.commit are required")
    if not isinstance(doc.get("jsse"), str):
        raise ValueError("jsse (the commit of the last recording) is required")
    buckets = doc.get("buckets")
    if not isinstance(buckets, dict):
        raise ValueError("buckets must be an object")
    for runner, table in buckets.items():
        if not isinstance(table, dict):
            raise ValueError(f"buckets.{runner} must be an object")
        for name, e in table.items():
            ok = (
                isinstance(e, list)
                and len(e) == 2
                and isinstance(e[0], int)
                and not isinstance(e[0], bool)
                and e[0] >= 0
                and isinstance(e[1], str)
                and HEX_DIGEST.match(e[1])
            )
            if not ok:
                raise ValueError(
                    f"buckets.{runner}.{name} must be [tests (int), digest ({DIGEST_LEN} hex digits)]"
                )
    return doc


def dump_baseline(doc):
    """Stable text, one bucket per line: the file stays small (a bucket is [tests, digest])."""
    runners = []
    for runner, table in sorted(doc["buckets"].items()):
        rows = ",\n".join(
            f"   {json.dumps(name)}: {json.dumps(e)}"
            for name, e in sorted(table.items())
        )
        runners.append(f"  {json.dumps(runner)}: {{\n{rows}\n  }}")
    body = ",\n".join(runners)
    buckets = f"{{\n{body}\n }}" if runners else "{}"
    ts = json.dumps(doc["typescript"], sort_keys=True)
    return (
        f'{{\n "buckets": {buckets},\n "jsse": {json.dumps(doc["jsse"])},\n'
        f' "schema": {SCHEMA},\n "typescript": {ts}\n}}\n'
    )


def parse_not_passing(text):
    """{(runner, bucket, id): issue} of the not-passing file; ValueError on a malformed line."""
    out = {}
    for n, line in enumerate(text.split("\n"), 1):
        if not line or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) != 4 or not all(parts):
            raise ValueError(
                f"line {n}: expected runner, bucket, id and issue separated by tabs"
            )
        runner, bucket, ident, issue = (unescape_field(p) for p in parts)
        key = (runner, bucket, ident)
        if key in out:
            raise ValueError(f"line {n}: duplicate entry")
        out[key] = issue
    return out


def dump_not_passing(entries):
    rows = sorted(entries.items())
    return NP_HEAD + "".join(
        "\t".join(escape_field(x) for x in (*key, issue)) + "\n" for key, issue in rows
    )


def git(*args, cwd=None):
    """Stdout of a git command run in `cwd` (default: the current directory), or None when it fails."""
    r = subprocess.run(["git", *args], capture_output=True, cwd=cwd, check=False)
    return r.stdout.decode("utf-8") if r.returncode == 0 else None


def read_at(ref, path, cwd=None):
    """Text of `path` at a revision (`ref` ""  = the index), or None when it does not exist there."""
    return git("show", f"{ref}:{path}", cwd=cwd)


def ratchet_problems(base, head, grown=()):
    """What a change from `base` to `head` loses. Each side is (baseline doc, not-passing dict) or None
    for a file that does not exist: new files are always fine; recorded buckets stay with the same size and
    digest, except the scripted buckets in `grown` (scripts/ts_passlist_cases_1298.py has proved that every
    recorded case is still there); the not-passing set only shrinks, except for the entries of a bucket recorded
    by this change."""
    if head is None:
        return (
            ["the pass list was deleted; it may only gain entries"]
            if base is not None
            else []
        )
    head_doc, head_np = head
    problems = []
    for runner, name, ident in sorted(head_np):
        if name not in head_doc["buckets"].get(runner, {}):
            problems.append(
                f"not-passing entry of the unrecorded bucket {runner}:{name}: {ident[:120]}"
            )
    if base is None:
        return problems
    base_doc, base_np = base
    if base_doc["buckets"] and base_doc["typescript"] != head_doc["typescript"]:
        problems.append(
            f"the TypeScript pin changed from {base_doc['typescript']} to {head_doc['typescript']}; "
            "a pin change needs a re-recorded baseline and a human ruling"
        )
    for runner, table in sorted(base_doc["buckets"].items()):
        for name, e in sorted(table.items()):
            h = head_doc["buckets"].get(runner, {}).get(name)
            if h is None:
                problems.append(f"bucket {runner}:{name} was removed")
            elif h != e and (runner, name) not in grown:
                problems.append(
                    f"bucket {runner}:{name} changed ({e[0]} tests {e[1][:12]} -> {h[0]} tests {h[1][:12]})"
                )
    for runner, name, ident in sorted(set(head_np) - set(base_np)):
        if name in base_doc["buckets"].get(runner, {}):
            problems.append(
                f"{runner}:{name} gained a not-passing entry (the pass list may only gain): {ident[:120]}"
            )
    return problems
