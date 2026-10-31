#!/usr/bin/env python3
"""Nightly TypeScript suite on jsse --node: one shard of the pass list gate, and the merge of the shards (issue #1298).

    TS_SRC_DIR=<built TypeScript v6.0.3 checkout> [TSC_DIR=<typescript 6.0.3 package>] \\
        uv run python scripts/run-ts-suite-nightly-1298.py shard --shard I --shards N --out DIR [options]
    uv run python scripts/run-ts-suite-nightly-1298.py merge --results DIR --out DIR --shards N [--blocking]
    uv run python scripts/run-ts-suite-nightly-1298.py plan --shards N      # the matrix of shard numbers, as JSON

`shard` runs scripts/run-ts-pass-list.py (#1296, loaded as a module and left unchanged) on the suites that
ts_nightly_1298.partition assigns to shard I of N and writes DIR/summary.json, timings.json and the sorted id lists
pass-<runner>.txt, fail-<runner>.txt (failed, not in the pass list) and regression-<runner>.txt (in the pass list,
failed). It exits 0 whenever it ran the gate and wrote its results (the verdict is the merge's); 2 when it did not.
Options: --jsse, --jobs (default 2), --batch-size, --test-timeout, --timeout, --sample/--seed (a smoke run),
--baseline-ref (default HEAD: the pass list of the checked-out commit), runner[:bucket] specs.

`merge` folds the shard directories found under --results (one per downloaded artifact) into --out: the merged id
lists, timings.json (every mocha batch and scripted run with its shard), summary.json and summary.md. Exit 1 on an
integrity problem (a shard missing or run twice, a universe that differs between shards or is not covered exactly
once, a bucket or universe that differs from the pin) and, with --blocking, on a baseline regression or a crashed
batch; without it those are `warning:` lines. --sampled skips the exactly-once coverage check; --runners limits the
runners that must be present.
"""

import argparse
import importlib.util
import json
import os
import sys
import time
from unittest import mock

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, SCRIPTS)
import ts_nightly_1298 as nightly  # noqa: E402
import ts_passlist as pl  # noqa: E402


def load_gate():
    spec = importlib.util.spec_from_file_location(
        "run_ts_pass_list", os.path.join(SCRIPTS, "run-ts-pass-list.py")
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


class Recorder:
    """What the gate computes and does not return: the universe, the outcome rows and the time of every run."""

    def __init__(self, shard, shards):
        self.shard, self.shards = shard, shards
        self.universe, self.selected, self.rows = {}, {}, {}
        self.batch_errors, self.integrity, self.timings = [], [], []
        self.reported = False

    def patches(self, gate):
        real_select, real_batches, real_report = (
            pl.select,
            gate.run_batches,
            gate.report,
        )
        real_mocha, real_cases = gate.compiler.mocha_run, gate.scripted.run_cases

        def select(universe, specs, sample=None, seed=None):
            self.universe = {r: len(t or []) for r, t in universe.items()}
            chosen = nightly.partition(
                real_select(universe, specs, sample, seed), self.shard, self.shards
            )
            self.selected = {r: sum(len(ts) for _, ts in s) for r, s in chosen.items()}
            return chosen

        def run_batches(args, work, ts_dir, exe, chosen):
            tables, errors = real_batches(args, work, ts_dir, exe, chosen)
            self.batch_errors = list(errors)
            return tables, errors

        def report(rows, args, problems):
            self.reported = True
            self.rows = rows
            self.integrity = [p for p in problems if p not in self.batch_errors]
            return real_report(rows, args, problems)

        def mocha_run(engine, exe, cwd, grep, test_timeout, timeout, dry=False):
            res = real_mocha(engine, exe, cwd, grep, test_timeout, timeout, dry)
            tests = (res["report"] or {}).get("tests", [])
            tag = os.path.basename(cwd)
            self.timings.append(
                nightly.timing_record(
                    tag.removeprefix("list-") if dry else tag.rsplit("-", 1)[0],
                    tag,
                    len(tests),
                    res["secs"],
                    res["exit"],
                    dry,
                )
            )
            return res

        def run_cases(runner, cases, python, jsse, jobs, timeout, env):
            start = time.monotonic()
            out = real_cases(runner, cases, python, jsse, jobs, timeout, env)
            self.timings.append(
                nightly.timing_record(
                    runner, runner, len(cases), time.monotonic() - start, 0
                )
            )
            return out

        return [
            mock.patch.object(pl, "select", select),
            mock.patch.object(gate, "run_batches", run_batches),
            mock.patch.object(gate, "report", report),
            mock.patch.object(gate.compiler, "mocha_run", mocha_run),
            mock.patch.object(gate.scripted, "run_cases", run_cases),
        ]

    def summary(self, code, secs):
        runners = {
            r: {
                "held": row["held"],
                "pass": len(row["pass"]),
                "fail": len(row["fail"]),
                "new_pass": len(row["new_pass"]),
                "regression": len(row["regression"]),
            }
            for r, row in self.rows.items()
        }
        return {
            "schema": nightly.SCHEMA,
            "shard": self.shard,
            "shards": self.shards,
            "ran": self.reported,
            "exit": code,
            "secs": round(secs, 1),
            "universe": self.universe,
            "selected": self.selected,
            "runners": runners,
            "batch_errors": self.batch_errors,
            "integrity": self.integrity,
        }

    def ids(self, key):
        return {r: [t.id for t in row[key]] for r, row in self.rows.items()}


def shard_main(args, gate_argv):
    gate = load_gate()
    rec = Recorder(args.shard, args.shards)
    patches = rec.patches(gate)
    start = time.monotonic()
    code = 2
    try:
        for p in patches:
            p.start()
        code = gate.main(gate_argv)
    finally:
        mock.patch.stopall()
    summary = rec.summary(code, time.monotonic() - start)
    nightly.write_shard(
        args.out,
        summary,
        rec.ids("pass"),
        rec.ids("fail"),
        rec.ids("regression"),
        rec.timings,
    )
    print(f"shard {args.shard}/{args.shards}: gate exit {code}; results in {args.out}")
    return 0 if rec.reported else 2


def build_gate_argv(args):
    argv = ["--require", "--jobs", str(args.jobs), "--batch-size", str(args.batch_size)]
    argv += ["--test-timeout", str(args.test_timeout), "--timeout", str(args.timeout)]
    argv += ["--baseline-ref", args.baseline_ref]
    for flag, value in (
        ("--jsse", args.jsse),
        ("--data-dir", args.data_dir),
        ("--oracle-json", args.oracle_json),
    ):
        if value:
            argv += [flag, value]
    if args.sample is not None:
        argv += ["--sample", str(args.sample), "--seed", str(args.seed or 0)]
    return argv + (list(args.specs) or list(nightly.DEFAULT_RUNNERS))


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    sh = sub.add_parser("shard")
    sh.add_argument("--shard", type=int, required=True)
    sh.add_argument("--shards", type=int, required=True)
    sh.add_argument("--out", required=True)
    sh.add_argument("--jsse")
    sh.add_argument("--jobs", type=int, default=2)
    sh.add_argument("--batch-size", type=int, default=240)
    sh.add_argument("--test-timeout", type=int, default=600000)
    sh.add_argument("--timeout", type=int, default=7200)
    sh.add_argument("--sample", type=float)
    sh.add_argument("--seed", type=int)
    sh.add_argument("--baseline-ref", default="HEAD")
    sh.add_argument("--data-dir", help="pass list files (the unit tests use another)")
    sh.add_argument(
        "--oracle-json", help="the universe pin (the unit tests use another)"
    )
    sh.add_argument("specs", nargs="*", metavar="runner[:bucket,...]")
    mg = sub.add_parser("merge")
    mg.add_argument("--results", required=True)
    mg.add_argument("--out", required=True)
    mg.add_argument("--shards", type=int, required=True)
    mg.add_argument("--blocking", action="store_true")
    mg.add_argument("--sampled", action="store_true")
    mg.add_argument("--runners", default=",".join(nightly.DEFAULT_RUNNERS))
    pn = sub.add_parser("plan")
    pn.add_argument("--shards", type=int, required=True)
    args = ap.parse_args(argv)
    if args.cmd == "plan":
        print(json.dumps(list(range(args.shards))))
        return 0
    if args.cmd == "shard":
        try:
            nightly.partition({}, args.shard, args.shards)
        except ValueError as e:
            ap.error(str(e))
        return shard_main(args, build_gate_argv(args))
    code, messages = nightly.merge(
        args.results,
        args.out,
        args.shards,
        [r for r in args.runners.split(",") if r],
        args.blocking,
        args.sampled,
    )
    print("\n".join(messages))
    print(f"nightly TypeScript suite: {'FAIL' if code else 'PASS'}")
    return code


if __name__ == "__main__":
    sys.exit(main())
