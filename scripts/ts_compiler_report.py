"""Result bookkeeping of scripts/run-ts-compiler-oracle.py: reports, comparison, known failures (issue #1268)."""

import collections
import hashlib
import os

RECORDABLE = ("passes on Node, fails on jsse (not in", "known-failures file lists")


def recordable(problems):
    """The problems `--record` does not resolve by rewriting the known-failures file."""
    return [p for p in problems if not p.startswith(RECORDABLE)]


def outcomes_of(report):
    """({(fullTitle, kind): n}, {fullTitle: first error line + diff}) of a JSON-reporter document."""
    outcomes, errors = collections.Counter(), {}
    for kind, key in (
        ("passing", "passes"),
        ("failing", "failures"),
        ("pending", "pending"),
    ):
        for t in (report or {}).get(key, []):
            title = t.get("fullTitle", "")
            outcomes[(title, kind)] += 1
            if kind == "failing":
                errors.setdefault(
                    title, ((t.get("err") or {}).get("message") or "").strip()
                )
    return outcomes, errors


def merge(results):
    """One result out of a list (batches of the same engine)."""
    out = {
        "secs": sum(r["secs"] for r in results),
        "local": {},
        "bad": [],
        "outcomes": collections.Counter(),
        "errors": {},
    }
    for r in results:
        if r["report"] is None:
            out["bad"].append(
                f"exit {r['exit']}, no report; output tail:\n" + r["tail"]
            )
        o, e = outcomes_of(r["report"])
        out["outcomes"].update(o)
        out["errors"].update(e)
        out["local"].update(r["local"])
    return out


def counts(res):
    c = collections.Counter()
    for (_, kind), n in res["outcomes"].items():
        c[kind] += n
    return {k: c.get(k, 0) for k in ("passing", "failing", "pending")}


def classify(node, jsse, known):
    """Problems of one comparison (list of lines): Node failures, unlisted jsse failures, stale known failures,
    tests run a different number of times, differing messages and differing local baselines."""
    node_pass, node_fail = collections.Counter(), set()
    for (t, kind), n in node["outcomes"].items():
        if kind == "passing":
            node_pass[t] += n
        elif kind == "failing":
            node_fail.add(t)
    jsse_fail = {t for (t, kind) in jsse["outcomes"] if kind == "failing"}
    lines = [f"fails on Node itself: {t}" for t in sorted(node_fail)]
    for t in sorted(jsse_fail - node_fail):
        if t not in known:
            lines.append(
                f"passes on Node, fails on jsse (not in the baseline): {t}\n    {jsse['errors'].get(t, '')[:600]}"
            )
    lines += [
        f"known-failures file lists a test that no longer fails on jsse: {t}"
        for t in sorted(known)
        if t not in jsse_fail
    ]
    ok = lambda r: {k: v for k, v in r["outcomes"].items() if k[1] != "failing"}  # noqa: E731
    a, b = ok(node), ok(jsse)
    diff = (
        {
            t
            for (t, kind) in set(a) | set(b)
            if a.get((t, kind), 0) != b.get((t, kind), 0)
        }
        - jsse_fail
        - node_fail
    )
    lines += [
        f"ran a different number of times (or not at all) on jsse: {t}"
        for t in sorted(diff)
    ]
    for t in sorted(node_fail & jsse_fail):
        if node["errors"].get(t) != jsse["errors"].get(t):
            lines.append(
                f"fails on both with different messages: {t}\n--- node\n{node['errors'].get(t)}\n--- jsse\n{jsse['errors'].get(t)}"
            )
    return lines


def local_problems(node, jsse, ref_dir):
    lines = []
    for rel in sorted(set(node["local"]) | set(jsse["local"])):
        a, b = node["local"].get(rel), jsse["local"].get(rel)
        if a is None or b is None:
            lines.append(
                f"local baseline {rel} only written by {'jsse' if a is None else 'Node'}"
            )
        elif a != b:
            lines.append(
                f"local baseline {rel} differs: node sha256 {hashlib.sha256(a).hexdigest()[:12]}, jsse {hashlib.sha256(b).hexdigest()[:12]} ({len(a)} vs {len(b)} bytes)"
            )
        ref = os.path.join(ref_dir, rel)
        if os.path.isfile(ref):
            with open(ref, "rb") as f:
                if f.read() == a:
                    lines.append(
                        f"local baseline {rel} equals its reference (written without a diff)"
                    )
    return lines


def load_known(path):
    known = {}
    if os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            for line in f:
                line = line.rstrip("\n")
                if line and not line.startswith("#"):
                    title, _, why = line.partition("\t")
                    known[title] = why
    return known


def write_known(path, known):
    head = (
        "# Tests that pass on Node and fail on jsse --node (run-ts-compiler-oracle.py): full test title, TAB, issue.\n"
        "# Maintained with --record; a listed test that passes makes the oracle fail.\n"
    )
    with open(path, "w", encoding="utf-8") as f:
        f.write(head + "".join(f"{t}\t{why}\n" for t, why in sorted(known.items())))
