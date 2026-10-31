# Library Tests: a scripted bucket of the TypeScript pass list may grow (#1298)

Split from [library-tests-typescript-pass-list-1296.md](library-tests-typescript-pass-list-1296.md) to keep each file under the 20,000-byte budget.

**The problem.** `ts-suite/jsse-pass-1296.json` records each bucket as `[tests, digest of the ids]` and the ratchet (`scripts/check_ts_pass_list_ratchet.py`) forbids any change of a recorded bucket. That is right for the mocha runners: their bucket is a slice of the universe that Node pins in `ts-suite/oracle.json`. The scripted runners (`tsc-projects`, `tsc-watch`, `hereby`; one bucket `all` each) list their cases from fixture files that gain cases: #1488 took `scripts/tsc-fixtures` from 367 to 413 cases, the recorded `tsc-projects:all` could not follow, the gate reported "recorded bucket tsc-projects:all has 367 tests, the checkout lists 413", and the nightly left the runner out.

**The rule** (a ruling, in the spirit of "a baseline may only gain entries"): a recorded *scripted* bucket may grow when every case recorded before still exists. A removed case (a shrink) or a renamed one fails like any loss. The mocha buckets keep the exact-equality rule.

## Files

| file | what |
|---|---|
| `ts-suite/jsse-pass-cases-1298.json` | the case names of scripted buckets: `{"schema": 1, "buckets": {runner: {bucket: {"cases": [...], "added": [...]}}}}`, one name per line, sorted. Only scripted runners are allowed in it (a mocha runner makes the file malformed). The bucket's `[tests, digest]` entry in `jsse-pass-1296.json` stays the source of truth and must hash these names (ids are `runner :: case`, as `scripts/ts_passlist_scripted.Case.id`) |
| `scripts/ts_passlist_cases_1298.py` | standard library only: parse and dump the file, `growth` (the ratchet rule), and the gate side `resolved`, `bucket_problems`, `held`, `record_scripted` |
| `scripts/test_ts_passlist_cases_1298.py` | grow, shrink, rename, digest-only transition, tampered `added`, a mocha bucket that grows, the hook on temporary repositories, the gate side, and the checked-in file against the current fixtures |

## The ratchet

`growth(base, head)` returns the buckets whose `[tests, digest]` entry may differ from the base and the problems it found; `ts_passlist_store.ratchet_problems(base, head, grown)` skips its "bucket changed" problem for those buckets only. A scripted bucket whose entry changed is accepted when:

1. the head's case list hashes to the head's entry (otherwise: "the case list does not hash to the recorded bucket"), and
2. the base had a case list for the bucket and every base case is still in the head's list (otherwise: "lost N recorded case(s)"), or
3. the base recorded only a digest (the situation of `tsc-projects`, `tsc-watch`, `hereby` when this landed): the head's cases that are **not** in its `added` list must hash to the base entry. That proves the old record is contained without anybody having kept its names. `added` is the list of names not in the old record; once the base has names it is no longer consulted.

Removing the case list of a bucket that had one, or changing a digest-only entry without a case list, fails. A grown bucket still may not gain a not-passing entry (the "may only gain" rule of the not-passing file is unchanged).

## The gate

`scripts/run-ts-pass-list.py` reads the case file next to the baseline (same `--baseline-ref`, or the working tree). For a scripted bucket with names, a recorded case that the checkout no longer lists is a bucket problem (a baseline for another test set); extra cases are no problem and show as `new-pass` until recorded; only the named cases are *held*, so a fixture added tomorrow is never a regression. A scripted bucket recorded as a digest only keeps the old exact check.

`--update-baseline` records a complete scripted bucket when all its cases pass and none of the recorded ones is missing, writing the three files (`jsse-pass-1296.json`, `pass-list-1296-not-passing.txt`, `jsse-pass-cases-1298.json`). For a bucket that is still a digest only, `--legacy-cases RUNNER=FILE` supplies the old names (one per line; they count only if they hash to the recorded entry); a bucket whose digest the universe still matches adopts the names without the option. Re-running the recording is idempotent.

## Recording of `tsc-projects` (413 cases)

The 367 old names were recovered from the checkout before #1488 (`git archive 61e741bf~1 scripts`, `run-tsc-projects-oracle.py --list`: 367 names, digest `39320e33...`, the recorded one; all 367 are in today's 413). Then, on a release build of `main` (`TSC_DIR` an installed typescript 6.0.3, `TS_SRC_DIR` the built checkout):

    uv run python scripts/run-ts-pass-list.py tsc-projects --update-baseline --issue 1298 \
      --legacy-cases tsc-projects=<the 367 names>

`tsc-projects` joined `DEFAULT_RUNNERS` of `scripts/ts_nightly_1298.py`. `tsc-watch` (38) and `hereby` (32) are still digest-only; their next recording adopts their names (pass both runners, no option needed).

## Not covered

The eslint rule tests (`hereby run-eslint-rules-tests`, 79) are in no gate runner and no nightly job: they exist only as the `eslint-*` cases of `scripts/run-ts-mocha-oracle.py` (`scripts/ts_eslint_cases.py`), which compares jsse with Node on demand.
