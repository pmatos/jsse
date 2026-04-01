#!/usr/bin/env python3
# /// script
# dependencies = []
# ///
"""Run JS benchmark suites (SunSpider, Kraken, Octane) on multiple engines.

Each benchmark runs as a single process invocation so the timing measures
actual execution, not startup overhead.

Usage:
    uv run scripts/run-benchmarks.py [options]

Examples:
    uv run scripts/run-benchmarks.py --suite sunspider
    uv run scripts/run-benchmarks.py --suite octane --engines jsse node boa
    uv run scripts/run-benchmarks.py --timeout 120
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

SCRIPT_DIR = Path(__file__).parent
PROJECT_DIR = SCRIPT_DIR.parent

DEFAULT_ENGINES = {
    "jsse": str(PROJECT_DIR / "target" / "release" / "jsse"),
    "node": "node",
    "boa": "/tmp/boa/target/release/boa",
    "engine262": "/tmp/engine262/lib/node/bin.mjs",
}

ENGINE_COMMANDS = {
    "jsse": lambda binary, script: [binary, script],
    "node": lambda binary, script: [binary, script],
    "boa": lambda binary, script: [binary, script],
    "engine262": lambda binary, script: ["node", binary, "--features=all", "--no-inspector", script],
}

# ---------------------------------------------------------------------------
# SunSpider
# ---------------------------------------------------------------------------

SUNSPIDER_DIR = PROJECT_DIR / "benchmarks" / "sunspider" / "tests" / "sunspider-1.0.2"

SUNSPIDER_TESTS = [
    "3d-cube", "3d-morph", "3d-raytrace",
    "access-binary-trees", "access-fannkuch", "access-nbody", "access-nsieve",
    "bitops-3bit-bits-in-byte", "bitops-bits-in-byte", "bitops-bitwise-and", "bitops-nsieve-bits",
    "controlflow-recursive",
    "crypto-aes", "crypto-md5", "crypto-sha1",
    "date-format-tofte", "date-format-xparb",
    "math-cordic", "math-partial-sums", "math-spectral-norm",
    "regexp-dna",
    "string-base64", "string-fasta", "string-tagcloud", "string-unpack-code", "string-validate-input",
]


def sunspider_tests():
    """Return list of (name, script_path) for SunSpider."""
    tests = []
    for name in SUNSPIDER_TESTS:
        path = SUNSPIDER_DIR / f"{name}.js"
        if path.exists():
            tests.append((name, str(path)))
    return tests


# ---------------------------------------------------------------------------
# Kraken 1.1
# ---------------------------------------------------------------------------

KRAKEN_DIR = PROJECT_DIR / "benchmarks" / "kraken" / "tests" / "kraken-1.1"

KRAKEN_TESTS = [
    "ai-astar",
    "audio-beat-detection", "audio-dft", "audio-fft", "audio-oscillator",
    "imaging-darkroom", "imaging-desaturate", "imaging-gaussian-blur",
    "json-parse-financial", "json-stringify-tinderbox",
    "stanford-crypto-aes", "stanford-crypto-ccm",
    "stanford-crypto-pbkdf2", "stanford-crypto-sha256-iterative",
]


def kraken_tests():
    """Return list of (name, tmp_script_path). Each test concatenates data+code."""
    tests = []
    for name in KRAKEN_TESTS:
        code = KRAKEN_DIR / f"{name}.js"
        data = KRAKEN_DIR / f"{name}-data.js"
        if not code.exists():
            continue

        tmpfile = tempfile.NamedTemporaryFile(
            mode="w", suffix=".js", delete=False, prefix=f"kraken-{name}-"
        )
        if data.exists():
            tmpfile.write(data.read_text())
            tmpfile.write("\n")
        tmpfile.write(code.read_text())
        tmpfile.close()
        tests.append((name, tmpfile.name))
    return tests


# ---------------------------------------------------------------------------
# Octane 2.0
# ---------------------------------------------------------------------------

OCTANE_DIR = PROJECT_DIR / "benchmarks" / "octane"

OCTANE_TESTS = {
    "richards": ["richards.js"],
    "deltablue": ["deltablue.js"],
    "crypto": ["crypto.js"],
    "raytrace": ["raytrace.js"],
    "earley-boyer": ["earley-boyer.js"],
    "regexp": ["regexp.js"],
    "splay": ["splay.js"],
    "navier-stokes": ["navier-stokes.js"],
    "pdfjs": ["pdfjs.js"],
    "code-load": ["code-load.js"],
    "box2d": ["box2d.js"],
    "gbemu": ["gbemu-part1.js", "gbemu-part2.js"],
    "zlib": ["zlib-data.js", "zlib.js"],
    "typescript": ["typescript-input.js", "typescript-compiler.js", "typescript.js"],
}

OCTANE_HARNESS = """
BenchmarkSuite.RunSuites({
  NotifyResult: function(name, result) { print(name + ": " + result); },
  NotifyError: function(name, error) { print("ERROR " + name + ": " + error); },
  NotifyScore: function(score) { print("Score: " + score); }
});
"""

# For Node.js, print() is not defined
OCTANE_HARNESS_NODE = """
if (typeof print === 'undefined') { var print = console.log; }
BenchmarkSuite.RunSuites({
  NotifyResult: function(name, result) { print(name + ": " + result); },
  NotifyError: function(name, error) { print("ERROR " + name + ": " + error); },
  NotifyScore: function(score) { print("Score: " + score); }
});
"""


def octane_tests(engine_name):
    """Return list of (name, tmp_script_path) for Octane."""
    base = (OCTANE_DIR / "base.js").read_text()
    harness = OCTANE_HARNESS_NODE if engine_name == "node" else OCTANE_HARNESS

    tests = []
    for name, files in OCTANE_TESTS.items():
        parts = [base]
        for fname in files:
            path = OCTANE_DIR / fname
            if not path.exists():
                break
            parts.append(path.read_text())
        else:
            parts.append(harness)
            tmpfile = tempfile.NamedTemporaryFile(
                mode="w", suffix=".js", delete=False, prefix=f"octane-{name}-"
            )
            tmpfile.write("\n".join(parts))
            tmpfile.close()
            tests.append((name, tmpfile.name))
    return tests


# ---------------------------------------------------------------------------
# Runner
# ---------------------------------------------------------------------------

def run_test(engine_name, binary, name, script_path, timeout):
    """Run a single benchmark. Returns (name, passed, duration_ms, score, error)."""
    cmd_fn = ENGINE_COMMANDS[engine_name]
    cmd = cmd_fn(binary, script_path)

    t0 = time.perf_counter()
    try:
        result = subprocess.run(
            cmd, timeout=timeout, capture_output=True
        )
        duration = (time.perf_counter() - t0) * 1000  # ms
        exit_code = result.returncode
    except subprocess.TimeoutExpired:
        duration = (time.perf_counter() - t0) * 1000
        return (name, False, duration, None, "timeout")
    except OSError as e:
        duration = (time.perf_counter() - t0) * 1000
        return (name, False, duration, None, str(e))

    stdout = result.stdout.decode("utf-8", errors="replace")
    stderr = result.stderr.decode("utf-8", errors="replace")

    # Try to extract Octane score
    score = None
    for line in stdout.split("\n"):
        if line.startswith("Score:"):
            try:
                score = float(line.split(":")[1].strip())
            except (ValueError, IndexError):
                pass

    if exit_code != 0:
        err_msg = stderr.strip().split("\n")[0] if stderr.strip() else f"exit {exit_code}"
        return (name, False, duration, score, err_msg)

    return (name, True, duration, score, None)


def run_suite(suite_name, engine_name, binary, timeout, repetitions):
    """Run an entire suite on an engine. Returns list of result dicts."""
    if suite_name == "sunspider":
        tests = sunspider_tests()
    elif suite_name == "kraken":
        tests = kraken_tests()
    elif suite_name == "octane":
        tests = octane_tests(engine_name)
    else:
        print(f"Unknown suite: {suite_name}")
        return []

    results = []
    for name, script_path in tests:
        durations = []
        last_result = None
        for rep in range(repetitions):
            last_result = run_test(engine_name, binary, name, script_path, timeout)
            _, passed, duration, score, error = last_result
            if not passed:
                break
            durations.append(duration)

        if durations:
            avg_ms = sum(durations) / len(durations)
            min_ms = min(durations)
            results.append({
                "name": name,
                "passed": True,
                "avg_ms": round(avg_ms, 2),
                "min_ms": round(min_ms, 2),
                "runs": len(durations),
                "score": last_result[3],
                "error": None,
            })
        else:
            _, _, duration, score, error = last_result
            results.append({
                "name": name,
                "passed": False,
                "avg_ms": round(duration, 2),
                "min_ms": round(duration, 2),
                "runs": 0,
                "score": score,
                "error": error,
            })

    # Clean up temp files for kraken/octane
    if suite_name in ("kraken", "octane"):
        for _, script_path in tests:
            try:
                os.unlink(script_path)
            except OSError:
                pass

    return results


def print_results(suite_name, all_results, engines):
    """Print comparison table."""
    # Collect all test names
    all_tests = []
    for engine in engines:
        for r in all_results.get(engine, []):
            if r["name"] not in all_tests:
                all_tests.append(r["name"])

    print(f"\n{'='*80}")
    print(f"  {suite_name.upper()} Results")
    print(f"{'='*80}")

    # Header
    header = f"  {'Test':<35s}"
    for engine in engines:
        header += f" {engine:>12s}"
    print(header)
    print(f"  {'-'*35}" + f" {'-'*12}" * len(engines))

    # Results by engine, keyed by test name
    by_engine = {}
    for engine in engines:
        by_engine[engine] = {r["name"]: r for r in all_results.get(engine, [])}

    for test_name in all_tests:
        row = f"  {test_name:<35s}"
        for engine in engines:
            r = by_engine[engine].get(test_name)
            if r is None:
                row += f" {'N/A':>12s}"
            elif not r["passed"]:
                reason = r["error"] or "fail"
                if reason == "timeout":
                    row += f" {'TIMEOUT':>12s}"
                else:
                    row += f" {'FAIL':>12s}"
            else:
                row += f" {r['min_ms']:>10.1f}ms"
        print(row)

    # Summary
    print()
    for engine in engines:
        results = all_results.get(engine, [])
        passed = sum(1 for r in results if r["passed"])
        total_ms = sum(r["min_ms"] for r in results if r["passed"])
        print(f"  {engine}: {passed}/{len(results)} passed, {total_ms:.0f}ms total (passing only)")


def main():
    parser = argparse.ArgumentParser(description="Run JS benchmarks on multiple engines")
    parser.add_argument("--suite", nargs="+",
                        default=["sunspider", "kraken", "octane"],
                        choices=["sunspider", "kraken", "octane"],
                        help="Suites to run")
    parser.add_argument("--engines", nargs="+",
                        default=["jsse", "node", "boa"],
                        help="Engines to test")
    parser.add_argument("--timeout", type=int, default=120,
                        help="Per-test timeout in seconds")
    parser.add_argument("--repetitions", type=int, default=3,
                        help="Number of repetitions per test (reports min)")
    parser.add_argument("--jsse-binary", default=DEFAULT_ENGINES["jsse"])
    parser.add_argument("--boa-binary", default=DEFAULT_ENGINES["boa"])
    parser.add_argument("--engine262-binary", default=DEFAULT_ENGINES["engine262"])
    parser.add_argument("--node-binary", default=DEFAULT_ENGINES["node"])
    parser.add_argument("--output", default=None, help="Output JSON file")
    args = parser.parse_args()

    binaries = {
        "jsse": args.jsse_binary,
        "node": args.node_binary,
        "boa": args.boa_binary,
        "engine262": args.engine262_binary,
    }

    # Check engines exist
    for engine in args.engines:
        binary = binaries[engine]
        # For engine262, check node + the mjs file
        if engine == "engine262":
            if not Path(binary).exists():
                print(f"WARNING: {engine} binary not found at {binary}")
        elif engine != "node" and not Path(binary).exists():
            print(f"WARNING: {engine} binary not found at {binary}")

    full_output = {}
    for suite in args.suite:
        print(f"\n{'#'*80}")
        print(f"#  Suite: {suite.upper()}")
        print(f"{'#'*80}")

        all_results = {}
        for engine in args.engines:
            binary = binaries[engine]
            print(f"\n  Running {suite} on {engine}...")
            results = run_suite(suite, engine, binary, args.timeout, args.repetitions)
            all_results[engine] = results

            passed = sum(1 for r in results if r["passed"])
            print(f"  {engine}: {passed}/{len(results)} passed")

        print_results(suite, all_results, args.engines)
        full_output[suite] = all_results

    if args.output:
        with open(args.output, "w") as f:
            json.dump(full_output, f, indent=2)
        print(f"\nResults written to {args.output}")


if __name__ == "__main__":
    main()
