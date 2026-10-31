"""Case lists of the scripted runners in the jsse pass list of the TypeScript suite (issue #1298).

Standard library only: scripts/check_ts_pass_list_ratchet.py (the pre-commit hook and the CI check) imports it.

A mocha runner's bucket is a fixed slice of the Node-pinned universe, so ts-suite/jsse-pass-1296.json can record it
as [tests, digest of the ids] and the ratchet can demand that the record never changes. The scripted runners
(tsc-projects, tsc-watch, hereby) list their cases from fixture files that grow, and a digest cannot show that a
bigger set still contains the recorded one. ts-suite/jsse-pass-cases-1298.json therefore stores the case names of a
scripted bucket, next to its unchanged [tests, digest] entry (which must hash the names). The rule, for scripted
runners only: a recorded bucket may GROW when every previously recorded case is still listed (a removed or renamed
case fails like a shrink); the mocha buckets keep the exact-equality rule of scripts/ts_passlist_store.py.

A bucket recorded before the names existed is a digest only. Its first growth carries `added`, the names that were
not in the old record: the case list minus `added` must hash to the old entry, which proves the old record is
contained without anybody having kept its names. Once the names are on the base side `added` is no longer consulted.
"""

import json
import os

import ts_passlist_store as store

CASES = "ts-suite/jsse-pass-cases-1298.json"
SCHEMA = 1
SCRIPTED_RUNNERS = ("tsc-projects", "tsc-watch", "hereby")


def entry_of(runner, names):
    """The [tests, digest] bucket entry of scripted cases: the ids are `runner :: case`, like scripted.Case.id."""
    return [len(names), store.bucket_digest(f"{runner} :: {n}" for n in names)]


def parse_cases(text):
    """{runner: {bucket: {"cases": [...], "added": [...]}}}; ValueError when it is not a well-formed document."""
    try:
        doc = json.loads(text)
    except ValueError as e:
        raise ValueError(f"{CASES}: not JSON: {e}") from e
    if not isinstance(doc, dict) or doc.get("schema") != SCHEMA:
        raise ValueError(f"{CASES}: schema is not {SCHEMA}")
    out = {}
    for runner, table in (doc.get("buckets") or {}).items():
        if runner not in SCRIPTED_RUNNERS:
            raise ValueError(
                f"{CASES}: {runner} is not a scripted runner; mocha buckets are recorded as digests only"
            )
        if not isinstance(table, dict):
            raise ValueError(f"{CASES}: buckets.{runner} must be an object")
        for name, e in table.items():
            if not (isinstance(e, dict) and set(e) <= {"cases", "added"}):
                raise ValueError(f"{CASES}: {runner}:{name} needs cases and added")
            cases, added = e.get("cases"), e.get("added", [])
            for key, names in (("cases", cases), ("added", added)):
                if not (
                    isinstance(names, list)
                    and all(isinstance(n, str) and n for n in names)
                    and len(set(names)) == len(names)
                ):
                    raise ValueError(
                        f"{CASES}: {runner}:{name}.{key} must be a list of distinct case names"
                    )
            if not set(added) <= set(cases):
                raise ValueError(f"{CASES}: {runner}:{name}.added is not in cases")
            out.setdefault(runner, {})[name] = {"cases": cases, "added": added}
    return out


def dump_cases(cases):
    """Stable text: one case name per line, sorted."""
    buckets = {
        runner: {
            name: {k: sorted(e[k]) for k in ("added", "cases") if k == "cases" or e[k]}
            for name, e in sorted(table.items())
        }
        for runner, table in sorted(cases.items())
    }
    return json.dumps({"schema": SCHEMA, "buckets": buckets}, indent=1) + "\n"


def load_at(ref, cwd=None):
    """The case lists at a revision (`ref` "" = the index); {} when the file does not exist there."""
    text = store.read_at(ref, CASES, cwd=cwd)
    return parse_cases(text) if text is not None else {}


def growth(base, head):
    """(buckets that may differ from the base entry, problems) of a change from `base` to `head`; each side is
    (baseline doc, not-passing dict, case lists) or None for a missing baseline."""
    if head is None:
        return set(), []
    head_doc, _, head_cases = head
    problems, grown = [], set()
    for runner, table in sorted(head_cases.items()):
        for name, e in sorted(table.items()):
            entry = head_doc["buckets"].get(runner, {}).get(name)
            if entry is None:
                problems.append(
                    f"{runner}:{name} has a case list but no recorded bucket"
                )
            elif entry != entry_of(runner, e["cases"]):
                problems.append(
                    f"{runner}:{name}: the case list does not hash to the recorded bucket {entry[:1]}"
                )
    if base is None:
        return grown, problems
    base_doc, _, base_cases = base
    for runner, table in sorted(base_doc["buckets"].items()):
        if runner not in SCRIPTED_RUNNERS:
            continue
        for name, old in sorted(table.items()):
            new = head_doc["buckets"].get(runner, {}).get(name)
            have = base_cases.get(runner, {}).get(name)
            now = head_cases.get(runner, {}).get(name)
            if new is None:
                continue
            if have is not None:
                if now is None:
                    problems.append(f"{runner}:{name}: the case list was removed")
                    continue
                lost = sorted(set(have["cases"]) - set(now["cases"]))
                if lost:
                    problems.append(
                        f"{runner}:{name} lost {len(lost)} recorded case(s), a case may not be removed or renamed: {lost[0]}"
                    )
                    continue
            elif new != old:
                if now is None:
                    continue
                kept = [n for n in now["cases"] if n not in set(now["added"])]
                if entry_of(runner, kept) != old:
                    problems.append(
                        f"{runner}:{name}: the cases that are not in `added` do not hash to the old record "
                        f"({old[0]} tests {old[1][:12]}), so the old record is not shown to be contained"
                    )
                    continue
            if new != old:
                grown.add((runner, name))
    return grown, problems


def load(source, ref, data_dir, root):
    """The case lists next to the baseline the gate loaded: `source` is what scripts/run-ts-pass-list.py's
    load_baseline reported (a git ref, "working tree" or "none")."""
    if source == "none":
        return {}
    if source == "working tree":
        try:
            with open(
                os.path.join(data_dir, os.path.basename(CASES)), encoding="utf-8"
            ) as f:
                return parse_cases(f.read())
        except OSError:
            return {}
    return load_at(source, cwd=root)


def read_legacy(specs):
    """{runner: [case names]} from `runner=FILE` options (one name per line)."""
    out = {}
    for spec in specs or []:
        runner, _, path = spec.partition("=")
        if runner not in SCRIPTED_RUNNERS or not path:
            raise ValueError(
                f"--legacy-cases expects RUNNER=FILE, RUNNER one of {SCRIPTED_RUNNERS}: {spec}"
            )
        with open(path, encoding="utf-8") as f:
            out[runner] = f.read().split()
    return out


def resolved(doc, cases, legacy):
    """{(runner, bucket): set of recorded case names} for the recorded scripted buckets whose names are known: from
    the case lists, or from `legacy` names when they hash to the recorded digest-only entry."""
    out = {}
    for runner in SCRIPTED_RUNNERS:
        for name, entry in doc["buckets"].get(runner, {}).items():
            have = cases.get(runner, {}).get(name)
            if have is not None:
                out[(runner, name)] = set(have["cases"])
            elif entry_of(runner, legacy.get(runner, [])) == entry:
                out[(runner, name)] = set(legacy[runner])
    return out


def bucket_problems(doc, universe, known):
    """Recorded buckets that the checkout cannot honour: a mocha bucket (or a scripted one without names) whose ids
    differ from the universe, a scripted bucket that lost a recorded case. Extra cases are not a problem."""
    problems = []
    for runner, tests in universe.items():
        by = {}
        for t in tests or []:
            by.setdefault(t.bucket, []).append(t)
        for name, e in doc["buckets"].get(runner, {}).items():
            got = by.get(name, [])
            if (runner, name) in known:
                lost = sorted(known[(runner, name)] - {t.full for t in got})
                if lost:
                    problems.append(
                        f"recorded bucket {runner}:{name} lists {len(lost)} case(s) the checkout lacks, first: {lost[0]}"
                    )
            elif [len(got), store.bucket_digest(t.id for t in got)] != e:
                problems.append(
                    f"recorded bucket {runner}:{name} has {e[0]} tests, the checkout lists {len(got)} (or other ids)"
                )
    return problems


def held(doc, known):
    """Predicate: is the test of a recorded bucket (a scripted one by case name) part of the pass list?"""

    def is_held(t):
        names = known.get((t.runner, t.bucket))
        if names is not None:
            return t.full in names
        return t.bucket in doc["buckets"].get(t.runner, {})

    return is_held


def record_scripted(doc, cases, run, complete, known, jsse_rev):
    """Record the case lists of the complete scripted buckets of a clean run, and let a recorded bucket grow when all
    of its cases pass and none of the recorded ones is missing: (doc, cases, notes)."""
    doc = json.loads(json.dumps(doc))
    cases = json.loads(json.dumps(cases))
    notes = []
    for (runner, name), tests in sorted(complete.items()):
        if runner not in SCRIPTED_RUNNERS:
            continue
        names = sorted(t.full for t in tests)
        entry = entry_of(runner, names)
        table = doc["buckets"].setdefault(runner, {})
        cur = table.get(name)
        have = cases.get(runner, {}).get(name)
        old = known.get((runner, name))
        if cur is None:
            old = set(names)
        elif old is None and cur == entry:
            old = set(names)
        if old is None:
            notes.append(
                f"{runner}:{name}: not grown, the recorded bucket has no case list (pass --legacy-cases {runner}=FILE)"
            )
            continue
        bad = sorted(t.full for t in tests if t.id not in run["passing_ids"])
        if bad:
            notes.append(
                f"{runner}:{name}: not recorded, {len(bad)} case(s) fail, first: {bad[0]}"
            )
            continue
        if not old <= set(names):
            notes.append(
                f"{runner}:{name}: not grown, recorded case(s) are gone: {sorted(old - set(names))[0]}"
            )
            continue
        added = set(have["added"] if have else []) | (
            set(names) - old if cur else set()
        )
        if cur == entry and have and have["cases"] == names:
            continue
        table[name] = entry
        cases.setdefault(runner, {})[name] = {"cases": names, "added": sorted(added)}
        doc["jsse"] = jsse_rev
        notes.append(
            f"{runner}:{name}: recorded {len(names)} cases ({len(set(names) - old)} new)"
        )
    return doc, cases, notes
