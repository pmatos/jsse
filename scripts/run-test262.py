#!/usr/bin/env python3
"""Run test262 tests against JavaScript engines and report pass/fail/percentage.

Usage:
    uv run python scripts/run-test262.py [options] [path...]

Examples:
    uv run python scripts/run-test262.py
    uv run python scripts/run-test262.py -j 8 --timeout 30
    uv run python scripts/run-test262.py test262/test/language/types/
    uv run python scripts/run-test262.py --engine node test262/test/built-ins/Array/
    uv run python scripts/run-test262.py --engine boa --binary /usr/local/bin/boa
"""

import argparse
import ctypes
import json
import os
import re
import resource
import signal
import subprocess
import sys
import tempfile
from abc import ABC, abstractmethod
from concurrent.futures import ProcessPoolExecutor, as_completed
from pathlib import Path

_main_pgid = None


def _set_pdeathsig():
    """Set PR_SET_PDEATHSIG so this process dies when its parent dies (Linux only)."""
    try:
        libc = ctypes.CDLL("libc.so.6", use_errno=True)
        libc.prctl(1, signal.SIGTERM)  # PR_SET_PDEATHSIG = 1
    except Exception:
        pass


def _worker_init():
    """Initializer for pool worker processes."""
    _set_pdeathsig()
    signal.signal(signal.SIGTERM, signal.SIG_DFL)
    signal.signal(signal.SIGINT, signal.SIG_DFL)


def _cleanup_handler(signum, frame):
    """Kill entire process group on termination signals."""
    if _main_pgid is not None:
        try:
            os.killpg(_main_pgid, signal.SIGTERM)
        except OSError:
            pass
    os._exit(128 + signum)


FRONTMATTER_RE = re.compile(r"/\*---\s*\n(.*?)\n---\*/", re.DOTALL)

FLAGS_RE = re.compile(r"^flags:\s*\[([^\]]*)\]", re.MULTILINE)
INCLUDES_RE = re.compile(r"^includes:\s*\[([^\]]*)\]", re.MULTILINE)
NEGATIVE_PHASE_RE = re.compile(
    r"^negative:\s*\n\s+phase:\s*(\S+)\s*\n\s+type:\s*(\S+)", re.MULTILINE
)
NEGATIVE_SIMPLE_RE = re.compile(r"^negative:", re.MULTILINE)
FEATURES_RE = re.compile(r"^features:\s*\[([^\]]*)\]", re.MULTILINE)


# ---------------------------------------------------------------------------
# Engine adapters
# ---------------------------------------------------------------------------

class EngineAdapter(ABC):
    """Abstract base for engine-specific behavior."""

    def __init__(self, binary: str):
        self.binary = binary

    @abstractmethod
    def build_command(
        self, test_file: Path, tmp_path: str | None, harness_files: list[Path],
        is_module: bool,
    ) -> list[str]:
        """Return the command list to execute."""

    @abstractmethod
    def needs_harness_in_source(self, is_module: bool) -> bool:
        """Whether harness must be concatenated into the source file."""

    @abstractmethod
    def is_parse_error(self, exit_code: int, stderr: str) -> bool:
        """Whether the result indicates a parse-phase error."""

    @abstractmethod
    def skip_module(self) -> bool:
        """Whether module tests should be skipped for this engine."""

    def setup_preexec(self):
        """Pre-exec function for subprocess: memory limit + pdeathsig."""
        mem_limit = 512 * 1024 * 1024  # 512 MB
        resource.setrlimit(resource.RLIMIT_AS, (mem_limit, mem_limit))
        _set_pdeathsig()


class JsseAdapter(EngineAdapter):
    def build_command(self, test_file, tmp_path, harness_files, is_module):
        if is_module:
            cmd = [self.binary]
            for hf in harness_files:
                cmd.extend(["--prelude", str(hf)])
            cmd.append("--module")
            cmd.append(str(test_file))
            return cmd
        return [self.binary, tmp_path]

    def needs_harness_in_source(self, is_module):
        return not is_module

    def is_parse_error(self, exit_code, stderr):
        return exit_code == 2

    def skip_module(self):
        return False


class NodeAdapter(EngineAdapter):
    def build_command(self, test_file, tmp_path, harness_files, is_module):
        return [self.binary, tmp_path]

    def needs_harness_in_source(self, is_module):
        return True

    def is_parse_error(self, exit_code, stderr):
        return "SyntaxError" in stderr

    def skip_module(self):
        return True


class BoaAdapter(EngineAdapter):
    def build_command(self, test_file, tmp_path, harness_files, is_module):
        if is_module:
            return [self.binary, "--module", tmp_path]
        return [self.binary, tmp_path]

    def needs_harness_in_source(self, is_module):
        return True

    def is_parse_error(self, exit_code, stderr):
        return "SyntaxError" in stderr

    def skip_module(self):
        return False


_ADAPTER_CLASSES = {
    "jsse": JsseAdapter,
    "node": NodeAdapter,
    "boa": BoaAdapter,
}

_DEFAULT_BINARIES = {
    "jsse": "./target/release/jsse",
    "node": "node",
    "boa": "boa",
}


def make_adapter(engine_name: str, binary: str | None = None) -> EngineAdapter:
    cls = _ADAPTER_CLASSES.get(engine_name)
    if cls is None:
        raise ValueError(f"Unknown engine: {engine_name}")
    if binary is None:
        binary = _DEFAULT_BINARIES[engine_name]
    return cls(binary)


# ---------------------------------------------------------------------------
# Frontmatter parsing
# ---------------------------------------------------------------------------

def parse_frontmatter(text: str) -> dict:
    m = FRONTMATTER_RE.search(text)
    if not m:
        return {}

    fm = m.group(1)
    result: dict = {}

    flags_m = FLAGS_RE.search(fm)
    if flags_m:
        result["flags"] = [f.strip() for f in flags_m.group(1).split(",") if f.strip()]

    includes_m = INCLUDES_RE.search(fm)
    if includes_m:
        result["includes"] = [
            i.strip() for i in includes_m.group(1).split(",") if i.strip()
        ]

    neg_m = NEGATIVE_PHASE_RE.search(fm)
    if neg_m:
        result["negative"] = {"phase": neg_m.group(1), "type": neg_m.group(2)}
    elif NEGATIVE_SIMPLE_RE.search(fm):
        result["negative"] = {"phase": "runtime", "type": ""}

    features_m = FEATURES_RE.search(fm)
    if features_m:
        result["features"] = [
            f.strip() for f in features_m.group(1).split(",") if f.strip()
        ]

    return result


# ---------------------------------------------------------------------------
# Scenario computation (dual strict/non-strict per spec)
# ---------------------------------------------------------------------------

def compute_scenarios(test_file: str, metadata: dict) -> list[tuple[str, str]]:
    """Return list of (scenario_id, mode) for a test file.

    Per INTERPRETING.md, tests without noStrict/onlyStrict/module/raw must run
    twice: once default and once with "use strict"; prepended.
    """
    flags = metadata.get("flags", [])

    if "module" in flags:
        return [(test_file, "module")]
    if "raw" in flags:
        return [(test_file, "default")]
    if "onlyStrict" in flags:
        return [(test_file, "strict")]
    if "noStrict" in flags:
        return [(test_file, "default")]

    # Dual mode: run both default and strict
    return [(test_file, "default"), (test_file + ":strict", "strict")]


# ---------------------------------------------------------------------------
# Harness / source building
# ---------------------------------------------------------------------------

def read_harness_file(test262_dir: Path, name: str) -> str:
    path = test262_dir / "harness" / name
    return path.read_text(encoding="utf-8", errors="replace")


def get_harness_files(
    metadata: dict, test262_dir: Path, is_module: bool, is_async: bool
) -> list[Path]:
    """Get list of harness files needed for a test."""
    flags = metadata.get("flags", [])
    harness_files = []

    if "raw" not in flags:
        harness_files.append(test262_dir / "harness" / "assert.js")
        harness_files.append(test262_dir / "harness" / "sta.js")

        if is_async:
            harness_files.append(test262_dir / "harness" / "doneprintHandle.js")

        for inc in metadata.get("includes", []):
            harness_files.append(test262_dir / "harness" / inc)

    return harness_files


def build_test_source(
    test_file: Path, metadata: dict, test262_dir: Path, mode: str,
) -> str:
    """Build the full source to feed to the engine, prepending harness files.

    mode is one of "default", "strict", "module".
    For modules with jsse, harness is loaded via --prelude instead.
    """
    flags = metadata.get("flags", [])
    is_async = "async" in flags

    if mode == "module":
        return test_file.read_text(encoding="utf-8", errors="replace")

    parts: list[str] = []
    source = test_file.read_text(encoding="utf-8", errors="replace")

    if mode == "strict":
        parts.append('"use strict";\n')

    if "raw" not in flags:
        parts.append(read_harness_file(test262_dir, "assert.js"))
        parts.append(read_harness_file(test262_dir, "sta.js"))

        if is_async:
            parts.append(read_harness_file(test262_dir, "doneprintHandle.js"))

        for inc in metadata.get("includes", []):
            parts.append(read_harness_file(test262_dir, inc))

    parts.append(source)
    return "\n".join(parts)


# ---------------------------------------------------------------------------
# Test execution
# ---------------------------------------------------------------------------

def run_single_test(
    args: tuple[str, str, str, int, str, str, str],
) -> tuple[str, bool, str]:
    """Run a single test scenario.

    Args tuple: (scenario_id, test_file_str, mode, timeout, test262_dir_str,
                  engine_name, engine_binary)
    Returns: (scenario_id, passed, skip_reason)
    """
    scenario_id, test_file_str, mode, timeout, test262_dir_str, engine_name, engine_binary = args
    test_file = Path(test_file_str)
    test262_dir = Path(test262_dir_str)

    adapter = make_adapter(engine_name, engine_binary)

    try:
        head = test_file.read_text(encoding="utf-8", errors="replace")[:8192]
    except OSError:
        return (scenario_id, False, "read_error")

    metadata = parse_frontmatter(head)
    flags = metadata.get("flags", [])

    is_module = mode == "module"
    is_async = "async" in flags
    negative = metadata.get("negative")

    if is_module and adapter.skip_module():
        return (scenario_id, False, "skip_module")

    harness_files = get_harness_files(metadata, test262_dir, is_module, is_async)
    concat_harness = adapter.needs_harness_in_source(is_module)

    tmp_path = None
    if concat_harness or not is_module:
        try:
            combined = build_test_source(test_file, metadata, test262_dir, mode)
        except OSError:
            return (scenario_id, False, "harness_error")

        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".js", delete=False, encoding="utf-8",
            dir=str(test_file.parent)
        ) as tmp:
            tmp.write(combined)
            tmp_path = tmp.name

    cmd = adapter.build_command(test_file, tmp_path, harness_files, is_module)

    try:
        result = subprocess.run(
            cmd,
            timeout=timeout,
            capture_output=True,
            preexec_fn=adapter.setup_preexec,
        )
        exit_code = result.returncode
    except subprocess.TimeoutExpired:
        if tmp_path:
            try:
                os.unlink(tmp_path)
            except OSError:
                pass
        return (scenario_id, False, "timeout")
    except OSError:
        if tmp_path:
            try:
                os.unlink(tmp_path)
            except OSError:
                pass
        return (scenario_id, False, "exec_error")
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

    return (scenario_id, passed, "")


# ---------------------------------------------------------------------------
# CLI and main
# ---------------------------------------------------------------------------

def parse_args():
    parser = argparse.ArgumentParser(
        description="Run test262 suite against JavaScript engines",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""\
examples:
  %(prog)s                                    # full jsse run
  %(prog)s test262/test/language/types/        # subset
  %(prog)s --engine node                       # test node
  %(prog)s --engine boa --binary ./boa_build   # custom boa binary
""",
    )
    parser.add_argument(
        "--engine",
        choices=["jsse", "node", "boa"],
        default="jsse",
        help="Engine to test (default: jsse)",
    )
    parser.add_argument(
        "--binary",
        default=None,
        help="Path to engine binary (default: auto per engine)",
    )
    parser.add_argument(
        "--jsse",
        default=None,
        dest="jsse_compat",
        help=argparse.SUPPRESS,  # hidden backward compat
    )
    parser.add_argument(
        "--test262",
        default="./test262",
        help="Path to test262 directory (default: ./test262)",
    )
    parser.add_argument(
        "-j",
        "--jobs",
        type=int,
        default=max((os.cpu_count() or 2) // 2, 1),
        help="Number of parallel jobs (default: half of nproc)",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=120,
        help="Timeout per test in seconds (default: 120)",
    )
    parser.add_argument(
        "paths",
        nargs="*",
        help="Specific test files or directories to run (default: all tests)",
    )
    return parser.parse_args()


def find_tests(test262_dir: Path, paths: list[str] | None) -> list[Path]:
    if paths:
        tests = []
        for p in paths:
            path = Path(p)
            if path.is_file() and path.suffix == ".js":
                tests.append(path)
            elif path.is_dir():
                tests.extend(sorted(path.rglob("*.js")))
        return sorted(tests)

    test_dir = test262_dir / "test"
    tests = []
    for subdir in ("language", "built-ins", "annexB"):
        d = test_dir / subdir
        if d.is_dir():
            tests.extend(d.rglob("*.js"))
    return sorted(tests)


def main():
    global _main_pgid

    try:
        os.setpgrp()
    except OSError:
        pass
    _main_pgid = os.getpid()

    signal.signal(signal.SIGTERM, _cleanup_handler)
    signal.signal(signal.SIGINT, _cleanup_handler)

    args = parse_args()

    engine_name = args.engine
    binary = args.binary
    if binary is None and args.jsse_compat is not None and engine_name == "jsse":
        binary = args.jsse_compat
    if binary is None:
        binary = _DEFAULT_BINARIES[engine_name]

    binary_path = Path(binary)
    if engine_name == "jsse" and not binary_path.is_file():
        print(f"Error: jsse binary not found at {binary_path}", file=sys.stderr)
        print("Build it first: cargo build --release", file=sys.stderr)
        sys.exit(2)

    test262 = Path(args.test262)
    if not (test262 / "test").is_dir():
        print(f"Error: test262 directory not found at {test262}", file=sys.stderr)
        sys.exit(2)

    tests = find_tests(test262, args.paths if args.paths else None)
    num_files = len(tests)
    if num_files == 0:
        print("No tests found.")
        sys.exit(1)

    # Expand files into scenarios (dual strict/non-strict per spec)
    scenarios = []
    for t in tests:
        try:
            head = t.read_text(encoding="utf-8", errors="replace")[:8192]
        except OSError:
            scenarios.append((str(t), "default"))
            continue
        metadata = parse_frontmatter(head)
        scenarios.extend(compute_scenarios(str(t), metadata))

    total = len(scenarios)
    resolved_binary = str(binary_path.resolve())
    resolved_test262 = str(test262.resolve())

    print(
        f"Found {num_files} test files, {total} scenarios, "
        f"running with {args.jobs} workers (timeout: {args.timeout}s)...",
        file=sys.stderr,
    )

    passed = 0
    failed = 0
    skipped = 0
    done = 0
    pass_list: list[str] = []
    fail_list: list[str] = []

    work = [
        (
            scenario_id,
            scenario_id.removesuffix(":strict"),  # test_file_str
            mode,
            args.timeout,
            resolved_test262,
            engine_name,
            resolved_binary,
        )
        for scenario_id, mode in scenarios
    ]

    with ProcessPoolExecutor(max_workers=args.jobs, initializer=_worker_init) as pool:
        futures = {pool.submit(run_single_test, w): w for w in work}
        for future in as_completed(futures):
            scenario_id, test_passed, skip_reason = future.result()
            done += 1
            if skip_reason.startswith("skip_"):
                skipped += 1
            elif test_passed:
                passed += 1
                pass_list.append(scenario_id)
            else:
                failed += 1
                fail_list.append(scenario_id)
            if done % 1000 == 0:
                run = passed + failed
                pct = (passed / run * 100) if run else 0
                print(
                    f"... {done}/{total} ({pct:.1f}% passing of {run} run)",
                    file=sys.stderr,
                )

    run_total = passed + failed
    percentage = (passed / run_total * 100) if run_total else 0

    # Pass/fail list filenames
    if engine_name == "jsse":
        baseline_file = Path("test262-pass.txt")
        fail_file = Path("/tmp/test262-fail.txt")
    else:
        baseline_file = Path(f"test262-pass-{engine_name}.txt")
        fail_file = Path(f"/tmp/test262-fail-{engine_name}.txt")

    # Regression detection
    regressions: list[str] = []
    new_passes: list[str] = []
    ran_tests = set(pass_list + fail_list)
    is_full_run = not args.paths
    if baseline_file.exists():
        baseline = set(baseline_file.read_text().strip().split("\n"))
        current = set(pass_list)
        regressions = sorted((baseline & ran_tests) - current)
        new_passes = sorted(current - baseline)

    print()
    print("=== test262 Results ===")
    print(f"Engine:  {engine_name} ({binary})")
    print(f"Files:   {num_files}")
    print(f"Scenarios: {total}")
    print(f"Run:     {run_total}")
    print(f"Skip:    {skipped}")
    print(f"Pass:    {passed}")
    print(f"Fail:    {failed}")
    print(f"Rate:    {percentage:.2f}%")

    if regressions:
        print(f"\n!!! REGRESSIONS: {len(regressions)} tests that previously passed now fail:")
        for r in regressions[:20]:
            print(f"  REGRESSED: {r}")
        if len(regressions) > 20:
            print(f"  ... and {len(regressions) - 20} more")

    if new_passes:
        print(f"\nNew passes: {len(new_passes)}")

    if is_full_run:
        pass_list.sort()
        baseline_file.write_text("\n".join(pass_list) + "\n")

    fail_list.sort()
    fail_file.write_text("\n".join(fail_list) + "\n")

    print()
    json_obj = {
        'engine': engine_name,
        'files': num_files,
        'scenarios': total,
        'run': run_total,
        'skip': skipped,
        'pass': passed,
        'fail': failed,
        'percentage': round(percentage, 2),
        'regressions': len(regressions),
        'new_passes': len(new_passes),
    }
    print(f"JSON: {json.dumps(json_obj)}")


if __name__ == "__main__":
    main()
