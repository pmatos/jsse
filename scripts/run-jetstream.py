#!/usr/bin/env python3
"""
JetStream 3 benchmark runner for jsse.

Runs the pure-JS workloads from JetStream 3 on jsse (or any JS engine).
Wasm and Worker-dependent benchmarks are skipped.

Usage:
    uv run python scripts/run-jetstream.py [options]

Options:
    --engine PATH       Path to JS engine binary (default: target/release/jsse)
    --jetstream PATH    Path to JetStream checkout (default: /tmp/JetStream)
    --iterations N      Override iteration count (default: use benchmark default)
    --test NAME         Run a specific test (comma-separated for multiple)
    --timeout SECS      Per-benchmark timeout in seconds (default: 300)
    --json FILE         Write JSON results to file
    --compare FILE      Compare results against a previous JSON run
    --list              List available benchmarks and exit
    -j N                Number of parallel workers (default: 1, benchmarks run sequentially)
    -v, --verbose       Verbose output
"""

import argparse
import json
import math
import os
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ProcessPoolExecutor, as_completed
from pathlib import Path

# JS-only benchmarks extracted from JetStreamDriver.js BENCHMARKS array.
# Each entry: (name, type, files, preload_dict_or_None, default_iterations, deterministic_random, worst_case_count)
BENCHMARKS = [
    # ARES-6
    ("Air", "sync", ["ARES-6/Air/symbols.js", "ARES-6/Air/tmp_base.js", "ARES-6/Air/arg.js", "ARES-6/Air/basic_block.js", "ARES-6/Air/code.js", "ARES-6/Air/frequented_block.js", "ARES-6/Air/inst.js", "ARES-6/Air/opcode.js", "ARES-6/Air/reg.js", "ARES-6/Air/stack_slot.js", "ARES-6/Air/tmp.js", "ARES-6/Air/util.js", "ARES-6/Air/custom.js", "ARES-6/Air/liveness.js", "ARES-6/Air/insertion_set.js", "ARES-6/Air/allocate_stack.js", "ARES-6/Air/payload-gbemu-executeIteration.js", "ARES-6/Air/payload-imaging-gaussian-blur-gaussianBlur.js", "ARES-6/Air/payload-airjs-ACLj8C.js", "ARES-6/Air/payload-typescript-scanIdentifier.js", "ARES-6/Air/benchmark.js"], None, 120, False, 4),
    ("Basic", "sync", ["ARES-6/Basic/ast.js", "ARES-6/Basic/basic.js", "ARES-6/Basic/caseless_map.js", "ARES-6/Basic/lexer.js", "ARES-6/Basic/number.js", "ARES-6/Basic/parser.js", "ARES-6/Basic/random.js", "ARES-6/Basic/state.js", "ARES-6/Basic/benchmark.js"], None, 120, False, 4),
    ("ML", "sync", ["ARES-6/ml/index.js", "ARES-6/ml/benchmark.js"], None, 60, False, 4),
    ("Babylon", "async", ["ARES-6/Babylon/index.js", "ARES-6/Babylon/benchmark.js"], {"airBlob": "ARES-6/Babylon/air-blob.js", "basicBlob": "ARES-6/Babylon/basic-blob.js", "inspectorBlob": "ARES-6/Babylon/inspector-blob.js", "babylonBlob": "ARES-6/Babylon/babylon-blob.js"}, 120, False, 4),
    # cdjs
    ("cdjs", "sync", ["cdjs/constants.js", "cdjs/util.js", "cdjs/red_black_tree.js", "cdjs/call_sign.js", "cdjs/vector_2d.js", "cdjs/vector_3d.js", "cdjs/motion.js", "cdjs/reduce_collision_set.js", "cdjs/simulator.js", "cdjs/collision.js", "cdjs/collision_detector.js", "cdjs/benchmark.js"], None, 60, False, 3),
    # CodeLoad
    ("first-inspector-code-load", "async", ["code-load/code-first-load.js"], {"inspectorPayloadBlob": "code-load/inspector-payload-minified.js"}, 120, False, 4),
    ("multi-inspector-code-load", "async", ["code-load/code-multi-load.js"], {"inspectorPayloadBlob": "code-load/inspector-payload-minified.js"}, 120, False, 4),
    # Octane
    ("Box2D", "sync", ["Octane/box2d.js"], None, 120, True, 4),
    ("octane-code-load", "sync", ["Octane/code-first-load.js"], None, 120, True, 4),
    ("crypto", "sync", ["Octane/crypto.js"], None, 120, True, 4),
    ("delta-blue", "sync", ["Octane/deltablue.js"], None, 120, True, 4),
    ("earley-boyer", "sync", ["Octane/earley-boyer.js"], None, 120, True, 4),
    ("gbemu", "sync", ["Octane/gbemu-part1.js", "Octane/gbemu-part2.js"], None, 120, True, 4),
    ("mandreel", "sync", ["Octane/mandreel.js"], None, 80, True, 4),
    ("navier-stokes", "sync", ["Octane/navier-stokes.js"], None, 120, True, 4),
    ("pdfjs", "sync", ["Octane/pdfjs.js"], None, 120, True, 4),
    ("raytrace", "sync", ["Octane/raytrace.js"], None, 120, False, 4),
    ("regexp-octane", "sync", ["Octane/regexp.js"], None, 120, True, 4),
    ("richards", "sync", ["Octane/richards.js"], None, 120, True, 4),
    ("splay", "sync", ["Octane/splay.js"], None, 120, True, 4),
    ("typescript-octane", "sync", ["Octane/typescript-compiler.js", "Octane/typescript-input.js", "Octane/typescript.js"], None, 15, True, 2),
    # RexBench
    ("FlightPlanner", "sync", ["RexBench/FlightPlanner/airways.js", "RexBench/FlightPlanner/waypoints.js", "RexBench/FlightPlanner/flight_planner.js", "RexBench/FlightPlanner/expectations.js", "RexBench/FlightPlanner/benchmark.js"], None, 120, False, 4),
    ("OfflineAssembler", "sync", ["RexBench/OfflineAssembler/registers.js", "RexBench/OfflineAssembler/instructions.js", "RexBench/OfflineAssembler/ast.js", "RexBench/OfflineAssembler/parser.js", "RexBench/OfflineAssembler/file.js", "RexBench/OfflineAssembler/LowLevelInterpreter.js", "RexBench/OfflineAssembler/LowLevelInterpreter32_64.js", "RexBench/OfflineAssembler/LowLevelInterpreter64.js", "RexBench/OfflineAssembler/InitBytecodes.js", "RexBench/OfflineAssembler/expected.js", "RexBench/OfflineAssembler/benchmark.js"], None, 80, False, 4),
    ("UniPoker", "sync", ["RexBench/UniPoker/poker.js", "RexBench/UniPoker/expected.js", "RexBench/UniPoker/benchmark.js"], None, 120, True, 4),
    # validatorjs
    ("validatorjs", "sync", ["validatorjs/dist/bundle.es6.js", "validatorjs/dist/bundle.es6.min.js", "validatorjs/benchmark.js"], None, 120, False, 4),
    # Simple
    ("hash-map", "sync", ["simple/hash-map.js"], None, 120, False, 4),
    ("doxbee-promise", "async", ["simple/doxbee-promise.js"], None, 120, False, 4),
    ("doxbee-async", "async", ["simple/doxbee-async.js"], None, 120, False, 4),
    # SeaMonster
    ("ai-astar", "sync", ["SeaMonster/ai-astar.js"], None, 120, False, 4),
    ("gaussian-blur", "sync", ["SeaMonster/gaussian-blur.js"], None, 120, False, 4),
    ("stanford-crypto-aes", "sync", ["SeaMonster/sjlc.js", "SeaMonster/stanford-crypto-aes.js"], None, 120, False, 4),
    ("stanford-crypto-pbkdf2", "sync", ["SeaMonster/sjlc.js", "SeaMonster/stanford-crypto-pbkdf2.js"], None, 120, False, 4),
    ("stanford-crypto-sha256", "sync", ["SeaMonster/sjlc.js", "SeaMonster/stanford-crypto-sha256.js"], None, 120, False, 4),
    ("json-stringify-inspector", "sync", ["SeaMonster/inspector-json-payload.js", "SeaMonster/json-stringify-inspector.js"], None, 20, False, 2),
    ("json-parse-inspector", "sync", ["SeaMonster/inspector-json-payload.js", "SeaMonster/json-parse-inspector.js"], None, 20, False, 2),
    # BigInt
    ("bigint-noble-bls12-381", "async", ["bigint/web-crypto-sham.js", "bigint/noble-bls12-381-bundle.js", "bigint/noble-benchmark.js"], None, 4, True, 1),
    ("bigint-noble-secp256k1", "async", ["bigint/web-crypto-sham.js", "bigint/noble-secp256k1-bundle.js", "bigint/noble-benchmark.js"], None, 120, True, 4),
    ("bigint-noble-ed25519", "async", ["bigint/web-crypto-sham.js", "bigint/noble-ed25519-bundle.js", "bigint/noble-benchmark.js"], None, 30, True, 4),
    ("bigint-paillier", "sync", ["bigint/web-crypto-sham.js", "bigint/paillier-bundle.js", "bigint/paillier-benchmark.js"], None, 10, True, 2),
    ("bigint-bigdenary", "sync", ["bigint/bigdenary-bundle.js", "bigint/bigdenary-benchmark.js"], None, 160, False, 16),
    # Proxy
    ("proxy-mobx", "async", ["proxy/common.js", "proxy/mobx-bundle.js", "proxy/mobx-benchmark.js"], None, 120, False, 4),
    ("proxy-vue", "async", ["proxy/common.js", "proxy/vue-bundle.js", "proxy/vue-benchmark.js"], None, 120, False, 4),
    # Class fields
    ("raytrace-public-class-fields", "sync", ["class-fields/raytrace-public-class-fields.js"], None, 120, False, 4),
    ("raytrace-private-class-fields", "sync", ["class-fields/raytrace-private-class-fields.js"], None, 120, False, 4),
    # Generators
    ("async-fs", "async", ["generators/async-file-system.js"], None, 80, True, 6),
    ("sync-fs", "sync", ["generators/sync-file-system.js"], None, 80, True, 6),
    ("lazy-collections", "sync", ["generators/lazy-collections.js"], None, 120, False, 4),
    ("js-tokens", "sync", ["generators/js-tokens.js"], None, 120, False, 4),
    # Startup benchmarks (require loadString/eval for BUNDLE preloads — skip for now)
    # ("mobx-startup", "async", ...), ("jsdom-d3-startup", "async", ...),
    # ("web-ssr", "async", ...), ("typescript-lib", "async", ...),
    # ("babylonjs-startup-*", "async", ...), ("babylonjs-scene-*", "async", ...),
    # Worker benchmarks (require Web Workers — skip for now)
    # ("bomb-workers", "async", ...), ("segmentation", "async", ...),
    # threejs (uses readFile global)
    # ("threejs", "sync", ["threejs/three.js", "threejs/benchmark.js"], None, 120, True, 4),
]

# Benchmarks that need special handling or are skipped
SKIPPED_BENCHMARKS = [
    "mobx-startup", "jsdom-d3-startup", "web-ssr", "typescript-lib",
    "babylonjs-startup-es5", "babylonjs-startup-es6", "babylonjs-scene-es5",
    "babylonjs-scene-es6", "bomb-workers", "segmentation", "threejs",
]


def build_polyfill_preamble():
    """Polyfills for globals that jsse may not have but JetStream expects."""
    return """
// --- Polyfills ---
if (typeof print === "undefined") {
    var print = function(...args) { console.log(...args); };
}
if (typeof printErr === "undefined") {
    var printErr = function(...args) { console.error(...args); };
}
if (typeof performance === "undefined") {
    var performance = {};
}
if (typeof performance.now !== "function") {
    const __perfStart = Date.now();
    performance.now = function() { return Date.now() - __perfStart; };
}
if (typeof performance.mark !== "function") {
    performance.mark = function() {};
}
if (typeof performance.measure !== "function") {
    performance.measure = function() {};
}
"""


def build_deterministic_random_code():
    return """
(() => {
    const initialSeed = 49734321;
    let seed = initialSeed;
    Math.random = () => {
        seed = ((seed + 0x7ed55d16) + (seed << 12))  & 0xffff_ffff;
        seed = ((seed ^ 0xc761c23c) ^ (seed >>> 19)) & 0xffff_ffff;
        seed = ((seed + 0x165667b1) + (seed << 5))   & 0xffff_ffff;
        seed = ((seed + 0xd3a2646c) ^ (seed << 9))   & 0xffff_ffff;
        seed = ((seed + 0xfd7046c5) + (seed << 3))   & 0xffff_ffff;
        seed = ((seed ^ 0xb55a4f09) ^ (seed >>> 16)) & 0xffff_ffff;
        return (seed >>> 0) / 0x1_0000_0000;
    };
    Math.random.__resetSeed = () => { seed = initialSeed; };
})();
"""


def build_sync_harness(iterations, deterministic_random, worst_case_count):
    reset_code = "Math.random.__resetSeed();" if deterministic_random else ""
    return f"""
// --- JetStream harness ---
const __iterations = {iterations};
const __results = [];
const benchmark = new Benchmark();
if (benchmark.init) benchmark.init();
for (let i = 0; i < __iterations; i++) {{
    if (benchmark.prepareForNextIteration) benchmark.prepareForNextIteration();
    {reset_code}
    const start = performance.now();
    benchmark.runIteration(i);
    const end = performance.now();
    __results.push(Math.max(1, end - start));
}}
if (benchmark.validate) benchmark.validate(__iterations);

// Output results as JSON
print(JSON.stringify({{
    results: __results,
    iterations: __iterations,
    worstCaseCount: {worst_case_count}
}}));
"""


def build_async_harness(iterations, deterministic_random, worst_case_count):
    reset_code = "Math.random.__resetSeed();" if deterministic_random else ""
    return f"""
// --- JetStream async harness ---
(async () => {{
    const __iterations = {iterations};
    const __results = [];
    const benchmark = new Benchmark();
    if (benchmark.init) await benchmark.init();
    for (let i = 0; i < __iterations; i++) {{
        if (benchmark.prepareForNextIteration) await benchmark.prepareForNextIteration();
        {reset_code}
        const start = performance.now();
        await benchmark.runIteration(i);
        const end = performance.now();
        __results.push(Math.max(1, end - start));
    }}
    if (benchmark.validate) benchmark.validate(__iterations);

    print(JSON.stringify({{
        results: __results,
        iterations: __iterations,
        worstCaseCount: {worst_case_count}
    }}));
}})();
"""


def build_preload_code(preloads, jetstream_dir):
    """Build code that injects preloaded file contents as globals."""
    code = ""
    for var_name, file_path in preloads.items():
        full_path = os.path.join(jetstream_dir, file_path)
        if not os.path.exists(full_path):
            # Try with .z extension removed (decompressed)
            alt_path = full_path.replace('.js', '.js')
            if not os.path.exists(alt_path):
                return None  # Can't preload
        content = Path(full_path).read_text(encoding='utf-8', errors='replace')
        # Escape for embedding in a JS string
        escaped = content.replace('\\', '\\\\').replace('`', '\\`').replace('${', '\\${')
        code += f"globalThis.{var_name} = `{escaped}`;\n"
    return code


def to_score(time_ms):
    return 5000.0 / max(time_ms, 1.0)


def compute_scores(results, worst_case_count):
    if not results:
        return None
    first_time = results[0]
    first_score = to_score(first_time)

    rest = sorted(results[1:], reverse=True)

    worst_time = None
    worst_score = None
    if worst_case_count and len(rest) >= worst_case_count:
        worst_times = rest[:worst_case_count]
        worst_time = sum(worst_times) / len(worst_times)
        worst_score = to_score(worst_time)

    avg_time = sum(rest) / len(rest) if rest else first_time
    avg_score = to_score(avg_time)

    scores = [first_score, avg_score]
    if worst_score is not None:
        scores.append(worst_score)

    # Overall score is geometric mean of sub-scores
    overall = math.exp(sum(math.log(s) for s in scores) / len(scores))

    return {
        "first_time": first_time,
        "first_score": first_score,
        "worst_time": worst_time,
        "worst_score": worst_score,
        "average_time": avg_time,
        "average_score": avg_score,
        "overall_score": overall,
    }


def run_benchmark(name, btype, files, preloads, iterations, det_random, worst_case,
                  engine, jetstream_dir, timeout, verbose, iteration_override):
    if iteration_override is not None:
        iterations = iteration_override

    # Build the concatenated script
    parts = []

    # Polyfills for missing globals
    parts.append(build_polyfill_preamble())

    # Deterministic random if needed
    if det_random:
        parts.append(build_deterministic_random_code())

    # Preloads
    if preloads:
        preload_code = build_preload_code(preloads, jetstream_dir)
        if preload_code is None:
            return {"name": name, "status": "skipped", "reason": "preload files not found"}
        parts.append(preload_code)

    # Source files
    for f in files:
        fpath = os.path.join(jetstream_dir, f)
        if not os.path.exists(fpath):
            return {"name": name, "status": "skipped", "reason": f"file not found: {f}"}
        parts.append(Path(fpath).read_text(encoding='utf-8', errors='replace'))

    # Harness
    if btype == "async":
        parts.append(build_async_harness(iterations, det_random, worst_case))
    else:
        parts.append(build_sync_harness(iterations, det_random, worst_case))

    script = "\n".join(parts)

    # Write to temp file
    with tempfile.NamedTemporaryFile(mode='w', suffix='.js', delete=False, dir='/tmp') as f:
        f.write(script)
        tmp_path = f.name

    try:
        start = time.time()
        result = subprocess.run(
            [engine, tmp_path],
            capture_output=True, text=True,
            timeout=timeout,
            cwd=jetstream_dir,
        )
        elapsed = time.time() - start

        if result.returncode != 0:
            stderr_preview = result.stderr[:500] if result.stderr else ""
            stdout_preview = result.stdout[:500] if result.stdout else ""
            if verbose:
                print(f"  FAIL: exit code {result.returncode}", file=sys.stderr)
                if stderr_preview:
                    print(f"  stderr: {stderr_preview}", file=sys.stderr)
                if stdout_preview:
                    print(f"  stdout: {stdout_preview}", file=sys.stderr)
            return {
                "name": name,
                "status": "error",
                "reason": f"exit code {result.returncode}",
                "stderr": stderr_preview,
                "elapsed": elapsed,
            }

        # Parse JSON output from last line of stdout
        output_lines = result.stdout.strip().split('\n')
        json_line = None
        for line in reversed(output_lines):
            line = line.strip()
            if line.startswith('{'):
                json_line = line
                break

        if not json_line:
            return {
                "name": name,
                "status": "error",
                "reason": "no JSON output",
                "stdout": result.stdout[:500],
                "elapsed": elapsed,
            }

        data = json.loads(json_line)
        scores = compute_scores(data["results"], data.get("worstCaseCount", worst_case))

        return {
            "name": name,
            "status": "pass",
            "elapsed": elapsed,
            "iterations": data["iterations"],
            "scores": scores,
            "raw_times": data["results"],
        }

    except subprocess.TimeoutExpired:
        return {"name": name, "status": "timeout", "reason": f"exceeded {timeout}s"}
    except json.JSONDecodeError as e:
        return {"name": name, "status": "error", "reason": f"JSON parse error: {e}"}
    finally:
        os.unlink(tmp_path)


def geometric_mean(values):
    if not values:
        return 0
    return math.exp(sum(math.log(v) for v in values) / len(values))


def main():
    parser = argparse.ArgumentParser(description="JetStream 3 benchmark runner for jsse")
    parser.add_argument("--engine", default="target/release/jsse", help="Path to JS engine")
    parser.add_argument("--jetstream", default="/tmp/JetStream", help="Path to JetStream checkout")
    parser.add_argument("--iterations", type=int, default=None, help="Override iteration count")
    parser.add_argument("--test", default=None, help="Run specific test(s), comma-separated")
    parser.add_argument("--timeout", type=int, default=300, help="Per-benchmark timeout (seconds)")
    parser.add_argument("--json", default=None, help="Write JSON results to file")
    parser.add_argument("--compare", default=None, help="Compare against previous JSON results")
    parser.add_argument("--list", action="store_true", help="List available benchmarks")
    parser.add_argument("-j", type=int, default=1, help="Parallel workers")
    parser.add_argument("-v", "--verbose", action="store_true", help="Verbose output")
    args = parser.parse_args()

    if args.list:
        print(f"Available JS benchmarks ({len(BENCHMARKS)}):")
        for name, btype, *_ in BENCHMARKS:
            print(f"  {name} ({btype})")
        print(f"\nSkipped benchmarks ({len(SKIPPED_BENCHMARKS)}):")
        for name in SKIPPED_BENCHMARKS:
            print(f"  {name}")
        return

    # Validate paths
    engine = os.path.abspath(args.engine)
    if not os.path.exists(engine):
        print(f"Error: engine not found at {engine}", file=sys.stderr)
        print("Build with: cargo build --release", file=sys.stderr)
        sys.exit(1)

    jetstream_dir = os.path.abspath(args.jetstream)
    if not os.path.exists(os.path.join(jetstream_dir, "JetStreamDriver.js")):
        print(f"Error: JetStream not found at {jetstream_dir}", file=sys.stderr)
        print("Clone with: gh repo clone WebKit/JetStream /tmp/JetStream -- --depth 1", file=sys.stderr)
        sys.exit(1)

    # Filter benchmarks
    benchmarks = BENCHMARKS
    if args.test:
        selected = set(args.test.split(","))
        benchmarks = [b for b in benchmarks if b[0] in selected]
        if not benchmarks:
            print(f"Error: no matching benchmarks for: {args.test}", file=sys.stderr)
            sys.exit(1)

    # Load comparison data
    compare_data = {}
    if args.compare:
        with open(args.compare) as f:
            prev = json.load(f)
        for r in prev.get("results", []):
            if r.get("status") == "pass" and r.get("scores"):
                compare_data[r["name"]] = r["scores"]["overall_score"]

    print(f"JetStream 3 — {len(benchmarks)} JS benchmarks")
    print(f"Engine: {engine}")
    print(f"Timeout: {args.timeout}s per benchmark")
    if args.iterations:
        print(f"Iterations: {args.iterations} (override)")
    print()

    all_results = []
    passed_scores = []
    errors = []
    skipped = []
    timeouts = []

    def process_result(result):
        all_results.append(result)
        name = result["name"]
        status = result["status"]

        if status == "pass":
            scores = result["scores"]
            overall = scores["overall_score"]
            passed_scores.append(overall)
            avg_ms = scores["average_time"]

            compare_str = ""
            if name in compare_data:
                prev_score = compare_data[name]
                ratio = overall / prev_score
                arrow = "^" if ratio > 1.01 else ("v" if ratio < 0.99 else "=")
                compare_str = f"  [{arrow} {ratio:.2f}x vs baseline]"

            print(f"  PASS  {name:40s}  score: {overall:8.1f}  avg: {avg_ms:8.1f}ms{compare_str}")
        elif status == "timeout":
            timeouts.append(name)
            print(f"  TIME  {name:40s}  (exceeded {args.timeout}s)")
        elif status == "skipped":
            skipped.append(name)
            print(f"  SKIP  {name:40s}  ({result.get('reason', '')})")
        else:
            errors.append(name)
            print(f"  FAIL  {name:40s}  ({result.get('reason', '')})")

    if args.j > 1:
        with ProcessPoolExecutor(max_workers=args.j) as executor:
            futures = {}
            for name, btype, files, preloads, iters, det_rand, worst in benchmarks:
                future = executor.submit(
                    run_benchmark, name, btype, files, preloads, iters,
                    det_rand, worst, engine, jetstream_dir, args.timeout,
                    args.verbose, args.iterations
                )
                futures[future] = name
            for future in as_completed(futures):
                process_result(future.result())
    else:
        for name, btype, files, preloads, iters, det_rand, worst in benchmarks:
            result = run_benchmark(
                name, btype, files, preloads, iters, det_rand, worst,
                engine, jetstream_dir, args.timeout, args.verbose, args.iterations
            )
            process_result(result)

    # Summary
    print()
    print("=" * 70)
    geo_mean = geometric_mean(passed_scores) if passed_scores else 0
    print(f"Overall score (geometric mean): {geo_mean:.1f}")
    print(f"Passed: {len(passed_scores)}/{len(benchmarks)}  "
          f"Errors: {len(errors)}  Timeouts: {len(timeouts)}  Skipped: {len(skipped)}")

    if compare_data and passed_scores:
        prev_scores = [compare_data[r["name"]] for r in all_results
                       if r["status"] == "pass" and r["name"] in compare_data]
        if prev_scores:
            prev_geo = geometric_mean(prev_scores)
            curr_matched = [r["scores"]["overall_score"] for r in all_results
                           if r["status"] == "pass" and r["name"] in compare_data]
            curr_geo = geometric_mean(curr_matched)
            ratio = curr_geo / prev_geo if prev_geo else 0
            print(f"vs baseline: {ratio:.2f}x (geo mean of matched benchmarks)")

    if errors:
        print(f"\nFailed: {', '.join(errors)}")
    if timeouts:
        print(f"Timed out: {', '.join(timeouts)}")

    # Write JSON
    output = {
        "engine": engine,
        "jetstream_version": "3.0",
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "overall_score": geo_mean,
        "passed": len(passed_scores),
        "total": len(benchmarks),
        "results": all_results,
    }

    if args.json:
        with open(args.json, 'w') as f:
            json.dump(output, f, indent=2)
        print(f"\nResults written to {args.json}")

    # Always write to a default location too
    default_path = "jetstream-results.json"
    with open(default_path, 'w') as f:
        json.dump(output, f, indent=2)


if __name__ == "__main__":
    main()
