"""Fail when the jsse pass list of the TypeScript suite loses entries (issue #1296).

    python scripts/check_ts_pass_list_ratchet.py                  # pre-commit: the index against HEAD
    python scripts/check_ts_pass_list_ratchet.py --base A --head B  # CI: B against the merge base of A and B

Compares ts-suite/jsse-pass-1296.json and ts-suite/pass-list-1296-not-passing.txt by git blob only (no TypeScript
checkout, no engine): a recorded bucket may not disappear or change, and the not-passing set may only shrink
(scripts/ts_passlist_store.py), except that a scripted bucket (tsc-projects, tsc-watch, hereby) may grow while every
recorded case is still listed (scripts/ts_passlist_cases_1298.py). A file that does not exist on the old side is new
and always fine.
"""

import argparse
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ts_passlist_cases_1298 as cases  # noqa: E402
import ts_passlist_store as store  # noqa: E402


def load(ref):
    """(baseline doc, not-passing dict, scripted case lists) at a revision, None when the baseline does not exist there."""
    text = store.read_at(ref, store.BASELINE)
    if text is None:
        return None
    doc = store.parse_baseline(text)
    np_text = store.read_at(ref, store.NOT_PASSING)
    np = store.parse_not_passing(np_text) if np_text is not None else {}
    return doc, np, cases.load_at(ref)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--base", help="CI: compare the merge base of this commit and head")
    ap.add_argument("--head", help="CI: the commit to check (default HEAD)")
    args = ap.parse_args(argv)
    if args.head and not args.base:
        ap.error("--head requires --base")
    if args.base:
        head_rev = args.head or "HEAD"
        base_rev = store.git("merge-base", args.base, head_rev)
        if base_rev is None:
            print(f"no merge base of {args.base} and {head_rev}", file=sys.stderr)
            return 1
        base_rev = base_rev.strip()
    else:
        base_rev, head_rev = "HEAD", ""
    try:
        base, head = load(base_rev), load(head_rev)
    except ValueError as e:
        print(f"ts pass list: malformed file: {e}", file=sys.stderr)
        return 1
    grown, extra = cases.growth(base, head)
    problems = (
        store.ratchet_problems(base and base[:2], head and head[:2], grown) + extra
    )
    if not problems:
        return 0
    print(
        "The TypeScript suite pass list may only gain entries (scripts/run-ts-pass-list.py):",
        file=sys.stderr,
    )
    for p in problems:
        print(f"  {p}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
