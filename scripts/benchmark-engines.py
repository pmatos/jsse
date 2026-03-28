#!/usr/bin/env python3
"""Benchmark multiple JS engines on test262, focusing on performance.

Runs test262 on all specified engines, finds the intersection of passing tests,
and produces a JSON report with per-test and per-category timing data.

Usage:
    uv run python scripts/benchmark-engines.py [options]

Examples:
    uv run python scripts/benchmark-engines.py --sample 0.05
    uv run python scripts/benchmark-engines.py --categories language/expressions language/statements
    uv run python scripts/benchmark-engines.py --tests-from common-pass.txt
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
import resource
import signal
import ctypes
import re
from pathlib import Path
from concurrent.futures import ProcessPoolExecutor, as_completed

# Reuse frontmatter/scenario logic from run-test262.py
sys.path.insert(0, str(Path(__file__).parent))
from importlib import import_module

# Import pieces from run-test262
_rt = import_module("run-test262")
parse_frontmatter = _rt.parse_frontmatter
compute_scenarios = _rt.compute_scenarios
build_test_source = _rt.build_test_source
make_adapter = _rt.make_adapter
find_tests = _rt.find_tests
_set_pdeathsig = _rt._set_pdeathsig
_worker_init = _rt._worker_init
get_harness_files = _rt.get_harness_files

# ---------------------------------------------------------------------------
# Engine config
# ---------------------------------------------------------------------------

DEFAULT_ENGINES = {
    "jsse": "./target/release/jsse",
    "node": "node",
    "boa": "/tmp/boa/target/release/boa",
    "engine262": "/tmp/engine262/lib/node/bin.mjs",
}


def run_single_test_timed(args):
    """Run a single test scenario and return detailed timing.

    Args tuple: (scenario_id, test_file_str, mode, timeout, test262_dir_str,
                  engine_name, engine_binary)
    Returns: (scenario_id, passed, skip_reason, duration_secs)
    """
    scenario_id, test_file_str, mode, timeout, test262_dir_str, engine_name, engine_binary = args
    test_file = Path(test_file_str)
    test262_dir = Path(test262_dir_str)

    adapter = make_adapter(engine_name, engine_binary)

    try:
        with open(test_file, encoding="utf-8", errors="replace", newline="") as f:
            head = f.read(8192)
    except OSError:
        return (scenario_id, False, "read_error", 0.0)

    metadata = parse_frontmatter(head)
    flags = metadata.get("flags", [])

    is_module = mode == "module"
    is_async = "async" in flags
    negative = metadata.get("negative")

    if is_module and adapter.skip_module():
        return (scenario_id, False, "skip_module", 0.0)

    harness_files = get_harness_files(metadata, test262_dir, is_module, is_async)
    concat_harness = adapter.needs_harness_in_source(is_module)

    tmp_path = None
    if concat_harness or not is_module:
        try:
            combined = build_test_source(test_file, metadata, test262_dir, mode)
        except OSError:
            return (scenario_id, False, "harness_error", 0.0)

        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".js", delete=False, encoding="utf-8",
            newline="", dir=str(test_file.parent)
        ) as tmp:
            tmp.write(combined)
            tmp_path = tmp.name

    cmd = adapter.build_command(test_file, tmp_path, harness_files, is_module, flags)

    t0 = time.perf_counter()
    try:
        result = subprocess.run(
            cmd,
            timeout=timeout,
            capture_output=True,
            preexec_fn=adapter.setup_preexec,
        )
        duration = time.perf_counter() - t0
        exit_code = result.returncode
    except subprocess.TimeoutExpired:
        duration = time.perf_counter() - t0
        if tmp_path:
            try:
                os.unlink(tmp_path)
            except OSError:
                pass
        return (scenario_id, False, "timeout", duration)
    except OSError:
        duration = time.perf_counter() - t0
        if tmp_path:
            try:
                os.unlink(tmp_path)
            except OSError:
                pass
        return (scenario_id, False, "exec_error", duration)
    finally:
        if tmp_path:
            try:
                os.unlink(tmp_path)
            except OSError:
                pass

    stderr_text = result.stderr.decode("utf-8", errors="replace")

    if negative:
        phase = negative.get("phase", "runtime")
        if phase == "parse":
            passed = adapter.is_parse_error(exit_code, stderr_text)
        else:
            passed = exit_code != 0
    elif is_async:
        stdout = result.stdout.decode("utf-8", errors="replace")
        if "Test262:AsyncTestComplete" in stdout:
            passed = exit_code == 0
        elif "Test262:AsyncTestFailure" in stdout:
            passed = False
        else:
            passed = exit_code == 0
    else:
        passed = exit_code == 0

    return (scenario_id, passed, "", duration)


def get_category(scenario_id, test262_dir):
    """Extract a meaningful category from a test path."""
    rel = os.path.relpath(scenario_id.split(":")[0], os.path.join(test262_dir, "test"))
    parts = rel.split(os.sep)
    if len(parts) >= 2:
        return f"{parts[0]}/{parts[1]}"
    return parts[0]


def run_engine(engine_name, binary, scenarios, test262_dir, timeout, jobs):
    """Run all scenarios on a single engine, return {scenario_id: (passed, duration)}."""
    tasks = [
        (sid, tfile, mode, timeout, test262_dir, engine_name, binary)
        for sid, tfile, mode in scenarios
    ]

    results = {}
    total = len(tasks)
    done = 0
    passed_count = 0

    print(f"\n{'='*60}")
    print(f"  Running {engine_name} ({total} scenarios)")
    print(f"{'='*60}")

    with ProcessPoolExecutor(max_workers=jobs, initializer=_worker_init) as executor:
        futures = {executor.submit(run_single_test_timed, t): t[0] for t in tasks}
        for future in as_completed(futures):
            scenario_id, passed, skip_reason, duration = future.result()
            results[scenario_id] = {"passed": passed, "duration": duration, "skip": skip_reason}
            done += 1
            if passed:
                passed_count += 1
            if done % 500 == 0 or done == total:
                print(f"  [{engine_name}] {done}/{total} — {passed_count} passed")

    return results


def collect_scenarios(test262_dir, paths, sample_rate=None, seed=42):
    """Collect all test scenarios from test262."""
    import random

    test262_path = Path(test262_dir)
    tests = find_tests(test262_path, paths)

    if sample_rate and sample_rate < 1.0:
        rng = random.Random(seed)
        tests = rng.sample(tests, max(1, int(len(tests) * sample_rate)))
        tests.sort()

    scenarios = []
    for test_file in tests:
        try:
            with open(test_file, encoding="utf-8", errors="replace", newline="") as f:
                head = f.read(8192)
        except OSError:
            continue
        metadata = parse_frontmatter(head)
        for scenario_id, mode in compute_scenarios(str(test_file), metadata):
            scenarios.append((scenario_id, str(test_file), mode))

    return scenarios


def main():
    parser = argparse.ArgumentParser(description="Benchmark JS engines on test262")
    parser.add_argument("--test262", default="./test262", help="test262 directory")
    parser.add_argument("-j", "--jobs", type=int,
                        default=max((os.cpu_count() or 2) // 2, 1))
    parser.add_argument("--timeout", type=int, default=30,
                        help="Per-test timeout in seconds (default: 30)")
    parser.add_argument("--sample", type=float, default=None,
                        help="Sample fraction of tests (e.g. 0.05 for 5%%)")
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--engines", nargs="+",
                        default=["jsse", "node", "boa", "engine262"])
    parser.add_argument("--jsse-binary", default=DEFAULT_ENGINES["jsse"])
    parser.add_argument("--boa-binary", default=DEFAULT_ENGINES["boa"])
    parser.add_argument("--engine262-binary", default=DEFAULT_ENGINES["engine262"])
    parser.add_argument("--node-binary", default=DEFAULT_ENGINES["node"])
    parser.add_argument("--output", default="benchmark-results.json",
                        help="Output JSON file")
    parser.add_argument("--common-only-output", default="benchmark-common.json",
                        help="Output JSON with only common-passing tests")
    parser.add_argument("paths", nargs="*",
                        help="Specific test directories/files")
    args = parser.parse_args()

    binaries = {
        "jsse": args.jsse_binary,
        "node": args.node_binary,
        "boa": args.boa_binary,
        "engine262": args.engine262_binary,
    }

    print(f"Collecting test scenarios...")
    scenarios = collect_scenarios(args.test262, args.paths or None, args.sample, args.seed)
    print(f"  {len(scenarios)} scenarios to run per engine")

    # Run each engine
    all_results = {}
    for engine in args.engines:
        binary = binaries[engine]
        all_results[engine] = run_engine(
            engine, binary, scenarios, args.test262, args.timeout, args.jobs
        )

    # Find common passing tests
    scenario_ids = set(scenarios[0][0] for scenarios[0] in [scenarios]) if not scenarios else {s[0] for s in scenarios}
    common_pass = None
    for engine in args.engines:
        engine_pass = {sid for sid, data in all_results[engine].items() if data["passed"]}
        if common_pass is None:
            common_pass = engine_pass
        else:
            common_pass &= engine_pass
    common_pass = common_pass or set()

    print(f"\n{'='*60}")
    print(f"  Results Summary")
    print(f"{'='*60}")
    for engine in args.engines:
        engine_pass = sum(1 for d in all_results[engine].values() if d["passed"])
        print(f"  {engine:12s}: {engine_pass}/{len(scenarios)} passed")
    print(f"  {'common':12s}: {len(common_pass)} tests pass in ALL engines")

    # Build per-category timing for common-passing tests
    category_timing = {engine: {} for engine in args.engines}
    for sid in sorted(common_pass):
        cat = get_category(sid, args.test262)
        for engine in args.engines:
            data = all_results[engine][sid]
            if cat not in category_timing[engine]:
                category_timing[engine][cat] = {"total_time": 0.0, "count": 0, "tests": []}
            category_timing[engine][cat]["total_time"] += data["duration"]
            category_timing[engine][cat]["count"] += 1
            category_timing[engine][cat]["tests"].append({
                "id": sid, "duration": data["duration"]
            })

    # Compute per-category average
    category_summary = {}
    all_cats = set()
    for engine in args.engines:
        for cat in category_timing[engine]:
            all_cats.add(cat)

    for cat in sorted(all_cats):
        category_summary[cat] = {}
        for engine in args.engines:
            ct = category_timing[engine].get(cat, {"total_time": 0, "count": 0})
            category_summary[cat][engine] = {
                "total_time": ct["total_time"],
                "count": ct["count"],
                "avg_time": ct["total_time"] / ct["count"] if ct["count"] > 0 else 0,
            }

    # Find where JSSE is furthest behind Boa
    jsse_vs_boa = []
    if "jsse" in args.engines and "boa" in args.engines:
        for cat in sorted(all_cats):
            jsse_t = category_summary[cat].get("jsse", {}).get("total_time", 0)
            boa_t = category_summary[cat].get("boa", {}).get("total_time", 0)
            count = category_summary[cat].get("jsse", {}).get("count", 0)
            if boa_t > 0 and count >= 5:
                ratio = jsse_t / boa_t
                jsse_vs_boa.append({
                    "category": cat,
                    "jsse_total": jsse_t,
                    "boa_total": boa_t,
                    "ratio": ratio,
                    "count": count,
                })
        jsse_vs_boa.sort(key=lambda x: -x["ratio"])

    # Save full results
    output = {
        "engines": args.engines,
        "binaries": {e: binaries[e] for e in args.engines},
        "total_scenarios": len(scenarios),
        "common_passing": len(common_pass),
        "per_engine_pass": {
            engine: sum(1 for d in all_results[engine].values() if d["passed"])
            for engine in args.engines
        },
        "category_summary": category_summary,
        "jsse_vs_boa": jsse_vs_boa[:30] if jsse_vs_boa else [],
    }

    with open(args.output, "w") as f:
        json.dump(output, f, indent=2)
    print(f"\nFull results written to {args.output}")

    # Save common-only detailed timing
    common_detail = {}
    for sid in sorted(common_pass):
        common_detail[sid] = {}
        for engine in args.engines:
            common_detail[sid][engine] = all_results[engine][sid]["duration"]

    common_output = {
        "engines": args.engines,
        "common_count": len(common_pass),
        "tests": common_detail,
        "category_summary": category_summary,
    }
    with open(args.common_only_output, "w") as f:
        json.dump(common_output, f, indent=2)
    print(f"Common-passing timing written to {args.common_only_output}")

    # Print top categories where JSSE is behind Boa
    if jsse_vs_boa:
        print(f"\n{'='*60}")
        print(f"  Top categories where JSSE is slowest vs Boa")
        print(f"{'='*60}")
        print(f"  {'Category':<40s} {'JSSE':>8s} {'Boa':>8s} {'Ratio':>8s} {'Tests':>6s}")
        print(f"  {'-'*40} {'-'*8} {'-'*8} {'-'*8} {'-'*6}")
        for entry in jsse_vs_boa[:15]:
            print(f"  {entry['category']:<40s} {entry['jsse_total']:>7.1f}s {entry['boa_total']:>7.1f}s {entry['ratio']:>7.1f}x {entry['count']:>6d}")


if __name__ == "__main__":
    main()
