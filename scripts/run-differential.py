#!/usr/bin/env python3
"""Differential check: run the same JS through jsse and a reference engine.

Each corpus file is run as a classic script by jsse and by every reference
engine (node always, Boa when a binary is given or found on PATH). The
observable result is stdout plus an outcome (`ok`, `throw:<ErrorClass>`,
`crash`, `timeout`). jsse must match node; Boa is reported as a second
opinion so a triager can tell an engine bug from a node/host quirk.

Known, accepted differences live in tests/differential/approved.toml. An
approval is pinned to a fingerprint of both results, so if the divergence
changes shape it is reported again instead of staying hidden.

Exit status: 0 when every file matches or is approved, 1 when any file has an
unapproved divergence, 2 on setup errors. `--observe` always exits 0 (the CI
gate runs observe-only until the corpus has settled).

    uv run python scripts/run-differential.py                # default corpus
    uv run python scripts/run-differential.py tests/differential/json.js
    uv run python scripts/run-differential.py --boa ~/bin/boa --json out.json
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
import re
import resource
import shutil
import subprocess
import sys
import tempfile
import tomllib
from dataclasses import asdict, dataclass, field
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CORPUS = REPO_ROOT / "tests" / "differential"
DEFAULT_APPROVED = DEFAULT_CORPUS / "approved.toml"
PRELUDE = REPO_ROOT / "scripts" / "differential-prelude.js"
NODE_RUNNER = REPO_ROOT / "scripts" / "differential-node-runner.js"

# Same limits as scripts/run-test262.py and fuzz/src/lib.rs: V8 needs ~2 GiB
# of address space just to start, so node gets a larger cap than jsse.
AS_LIMITS = {"jsse": 512 << 20, "node": 4 << 30, "boa": 4 << 30}
# jsse exits 101 when the interpreter panics (Rust's default panic exit code).
JSSE_PANIC_EXIT = 101
ERROR_CLASS_RE = re.compile(r"^\s*([A-Za-z0-9]*Error)\b:?")
BOA_REPORT_RE = re.compile(r"^\s*\d+:\s+([A-Za-z0-9]*Error):", re.MULTILINE)
ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")


@dataclass(frozen=True)
class Result:
    outcome: str
    stdout: str

    def key(self) -> str:
        return f"{self.outcome}\n{self.stdout}"


@dataclass
class FileReport:
    file: str
    status: str  # match | diverge | approved | changed
    jsse: Result
    refs: dict[str, Result] = field(default_factory=dict)
    fingerprint: str = ""
    detail: str = ""
    reason: str = ""


def extract_error_class(stderr: str) -> str | None:
    """First `SomeError:`-shaped line. Node prints the source line and a caret
    before the error line, so every line has to be scanned."""
    for line in stderr.splitlines():
        m = ERROR_CLASS_RE.match(line)
        if m:
            return m.group(1)
    return None


def normalize_stdout(text: str) -> str:
    lines = text.replace("\r\n", "\n").split("\n")
    return "\n".join(line.rstrip() for line in lines).rstrip("\n")


def classify(engine: str, code: int | None, stdout: str, stderr: str) -> Result:
    out = normalize_stdout(stdout)
    if code is None:
        return Result("timeout", out)
    if code < 0 or (engine == "jsse" and code == JSSE_PANIC_EXIT):
        if "memory allocation of" in stderr:
            return Result("oom", out)
        return Result("crash", out)
    if code == 0:
        return Result("ok", out)
    if engine == "boa":
        return classify_boa_error(out, stderr)
    cls = extract_error_class(stderr)
    return Result(f"throw:{cls or '?'}", out)


def classify_boa_error(out: str, stderr: str) -> Result:
    """Boa prints an uncaught exception to stdout as `Uncaught Error: <value>`
    plus `    at` frames, and a parse error to stderr as an eyre report
    (`   0: SyntaxError: ...`, ANSI-coloured). Move the uncaught report out of
    stdout so the comparison stays about what the program printed."""
    lines = out.split("\n")
    for i in range(len(lines) - 1, -1, -1):
        if lines[i].startswith("Uncaught "):
            value = lines[i].removeprefix("Uncaught ").removeprefix("Error: ")
            cls = extract_error_class(value)
            return Result(f"throw:{cls or '?'}", "\n".join(lines[:i]))
    m = BOA_REPORT_RE.search(ANSI_RE.sub("", stderr))
    return Result(f"throw:{m.group(1) if m else '?'}", out)


def _preexec(limit: int):
    def fn():
        resource.setrlimit(resource.RLIMIT_AS, (limit, limit))

    return fn


def engine_command(engine: str, binary: str, path: Path) -> list[str]:
    if engine == "node":
        return [binary, "--expose-gc", str(NODE_RUNNER), str(path)]
    return [binary, str(path)]


def with_prelude(path: Path, scratch: Path) -> Path:
    """Every engine runs the same bytes: the `show` prelude, then the file."""
    combined = scratch / path.name
    combined.write_text(
        PRELUDE.read_text(encoding="utf-8")
        + "\n"
        + path.read_text(encoding="utf-8")
        # Boa's CLI prints the script's completion value; make it undefined.
        + "\nvoid 0;\n",
        encoding="utf-8",
    )
    return combined


def run_engine(engine: str, binary: str, path: Path, timeout: float) -> Result:
    try:
        proc = subprocess.run(
            engine_command(engine, binary, path),
            stdin=subprocess.DEVNULL,
            capture_output=True,
            timeout=timeout,
            preexec_fn=_preexec(AS_LIMITS[engine]),
            cwd=path.parent,
        )
    except subprocess.TimeoutExpired as exc:
        partial = exc.stdout.decode("utf-8", "replace") if exc.stdout else ""
        return classify(engine, None, partial, "")
    return classify(
        engine,
        proc.returncode,
        proc.stdout.decode("utf-8", "replace"),
        proc.stderr.decode("utf-8", "replace"),
    )


def fingerprint(jsse: Result, ref: Result) -> str:
    digest = hashlib.sha256(f"{jsse.key()}\0{ref.key()}".encode()).hexdigest()
    return digest[:16]


def first_difference(jsse: Result, ref: Result, ref_name: str) -> str:
    if jsse.outcome != ref.outcome:
        head = f"outcome: jsse={jsse.outcome} {ref_name}={ref.outcome}"
    else:
        head = ""
    a, b = jsse.stdout.split("\n"), ref.stdout.split("\n")
    for i in range(max(len(a), len(b))):
        la = a[i] if i < len(a) else "<missing>"
        lb = b[i] if i < len(b) else "<missing>"
        if la != lb:
            line = f"stdout line {i + 1}: jsse={la!r} {ref_name}={lb!r}"
            return f"{head}; {line}" if head else line
    return head


def load_approved(path: Path) -> dict[str, dict]:
    if not path.exists():
        return {}
    data = tomllib.loads(path.read_text(encoding="utf-8"))
    approved = {}
    for entry in data.get("approved", []):
        for key in ("file", "fingerprint", "reason"):
            if not entry.get(key):
                raise ValueError(f"{path}: approved entry missing '{key}': {entry}")
        approved[entry["file"]] = entry
    return approved


def judge(
    rel: str, jsse: Result, refs: dict[str, Result], approved: dict[str, dict]
) -> FileReport:
    node = refs["node"]
    report = FileReport(rel, "match", jsse, refs)
    if jsse == node:
        return report
    report.fingerprint = fingerprint(jsse, node)
    report.detail = first_difference(jsse, node, "node")
    if "boa" in refs:
        verdict = "agrees" if refs["boa"] == jsse else "disagrees"
        report.detail += f" (boa {verdict} with jsse)"
    entry = approved.get(rel)
    if entry is None:
        report.status = "diverge"
    elif entry["fingerprint"] == report.fingerprint:
        report.status = "approved"
        report.reason = entry["reason"]
    else:
        report.status = "changed"
        report.reason = entry["reason"]
    return report


def collect(paths: list[str]) -> list[Path]:
    files: list[Path] = []
    for raw in paths or [str(DEFAULT_CORPUS)]:
        p = Path(raw).resolve()
        if p.is_dir():
            files.extend(sorted(p.rglob("*.js")))
        elif p.is_file():
            files.append(p)
        else:
            raise FileNotFoundError(raw)
    return files


def relative(p: Path) -> str:
    try:
        return p.relative_to(REPO_ROOT).as_posix()
    except ValueError:
        return p.as_posix()


def write_summary(reports: list[FileReport], stale: list[str], out) -> None:
    counts = {s: 0 for s in ("match", "approved", "diverge", "changed")}
    for r in reports:
        counts[r.status] += 1
    print("## Differential check (jsse vs node)\n", file=out)
    print(
        f"{len(reports)} files: {counts['match']} match, "
        f"{counts['approved']} approved, {counts['diverge']} diverge, "
        f"{counts['changed']} approved-but-changed, {len(stale)} stale approvals\n",
        file=out,
    )
    bad = [r for r in reports if r.status in ("diverge", "changed")]
    if bad:
        print("| file | status | fingerprint | first difference |", file=out)
        print("|---|---|---|---|", file=out)
        for r in bad:
            detail = r.detail.replace("|", "\\|")
            print(
                f"| `{r.file}` | {r.status} | `{r.fingerprint}` | {detail} |", file=out
            )
        print(file=out)
    for name in stale:
        print(f"- stale approval (now matches, remove it): `{name}`", file=out)


def parse_args(argv: list[str] | None) -> argparse.Namespace:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument(
        "paths", nargs="*", help="files or directories (default: tests/differential)"
    )
    ap.add_argument("--jsse", default=str(REPO_ROOT / "target/release/jsse"))
    ap.add_argument("--node", default="node")
    ap.add_argument(
        "--boa", default=None, help="Boa CLI binary (default: `boa` on PATH, if any)"
    )
    ap.add_argument(
        "--no-boa", action="store_true", help="skip Boa even if it is on PATH"
    )
    ap.add_argument("--approved", default=str(DEFAULT_APPROVED))
    ap.add_argument("--timeout", type=float, default=10.0)
    ap.add_argument(
        "-j", "--jobs", type=int, default=max((os.cpu_count() or 2) // 2, 1)
    )
    ap.add_argument(
        "--json", dest="json_out", help="write a machine-readable report here"
    )
    ap.add_argument("--observe", action="store_true", help="report only; always exit 0")
    return ap.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    args.jsse = str(Path(args.jsse).resolve())
    if not Path(args.jsse).is_file():
        print(
            f"jsse binary not found: {args.jsse} (run cargo build --release)",
            file=sys.stderr,
        )
        return 2
    if shutil.which(args.node) is None:
        print(f"node not found: {args.node}", file=sys.stderr)
        return 2
    engines = {"node": args.node}
    boa = None if args.no_boa else (args.boa or shutil.which("boa"))
    if boa:
        engines["boa"] = boa
    approved = load_approved(Path(args.approved))
    files = collect(args.paths)

    def run_one(path: Path) -> FileReport:
        with tempfile.TemporaryDirectory(prefix="jsse-differential-") as tmp:
            src = with_prelude(path, Path(tmp))
            jsse = run_engine("jsse", args.jsse, src, args.timeout)
            refs = {
                name: run_engine(name, b, src, args.timeout)
                for name, b in engines.items()
            }
        return judge(relative(path), jsse, refs, approved)

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        reports = list(pool.map(run_one, files))

    stale = sorted(
        r.file for r in reports if r.status == "match" and r.file in approved
    )
    write_summary(reports, stale, sys.stdout)
    if summary_path := os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(summary_path, "a", encoding="utf-8") as fh:
            write_summary(reports, stale, fh)
    if args.json_out:
        payload = {
            "engines": ["jsse", *engines],
            "reports": [asdict(r) for r in reports],
            "stale_approvals": stale,
        }
        Path(args.json_out).write_text(json.dumps(payload, indent=2), encoding="utf-8")

    failed = any(r.status in ("diverge", "changed") for r in reports)
    if failed and not args.observe:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
