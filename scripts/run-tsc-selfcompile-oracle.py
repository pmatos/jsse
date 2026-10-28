#!/usr/bin/env python3
"""Compile TypeScript's own sources with tsc 6.0.3 on Node and on jsse --node, diff the output, record wall time and RSS (issue #1249).

    TS_SRC_DIR=/path/to/TypeScript [TSC_DIR=/path/to/typescript] uv run python scripts/run-tsc-selfcompile-oracle.py [options] [case ...]

TS_SRC_DIR is a TypeScript v6.0.3 checkout with `npm ci` done (the sources under src/ and node_modules/@types/node are
used; the checkout is never written to). The compiler is TSC_DIR (an installed typescript 6.0.3 package), else
$TS_SRC_DIR/built/local when `hereby local` has run there. --smoke (the generated program only) needs just the
compiler, so TSC_DIR alone is enough. Without the inputs the script prints SKIP and exits 0 unless --require is given.

Each case copies src/ into a fresh tree per engine and runs `_tsc.js -p ...` there:

  compiler-dts      the real src/compiler project (declarations, declaration maps, tsbuildinfo)
  compiler-js       the same sources with JS emit and source maps (the checker, parser, emitter, ...)
  src-js            all of src/ (452 files, ~350k lines) as one program with JS emit
  generated-large   a generated multi-module program (see gen_program) with JS, d.ts and maps

The files each run wrote must be byte-identical (the tree's own path is the only normalised text), and stdout, stderr
and the exit status must agree. The wall time and peak RSS of every run are printed as a table (--report FILE writes
them as JSON) and checked against the budget file scripts/tsc-fixtures/selfcompile-budget.json (per case:
jsse_max_seconds, jsse_max_rss_mb, max_time_ratio, max_rss_ratio; null or absent means no limit) and against
--max-seconds / --max-rss-mb. Exits 1 on any difference or exceeded budget.

Options: --jsse PATH (else $JSSE, else target/release/jsse), --jobs N (jsse cases at once, default 1; budgets are only
meaningful at 1), --timeout SECONDS per run (default 7200), --smoke (generated-large only), --list, --keep DIR,
--budget FILE, --report FILE, --require, --ci.
"""

import argparse
import hashlib
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from concurrent.futures import ThreadPoolExecutor

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BUDGET = os.path.join(ROOT, "scripts", "tsc-fixtures", "selfcompile-budget.json")
DROP_ENV = ("NODE_OPTIONS", "FORCE_COLOR", "NO_COLOR", "COLUMNS", "CI")
OVERRIDES = [
    "--composite",
    "false",
    "--incremental",
    "false",
    "--emitDeclarationOnly",
    "false",
    "--declaration",
    "false",
    "--declarationMap",
    "false",
    "--isolatedDeclarations",
    "false",
]
SRC_PROJECTS = (
    "compiler jsTyping services server typingsInstallerCore deprecatedCompat tsc typescript typingsInstaller watchGuard"
).split()
SELF_CONFIG = {
    "extends": "./tsconfig-base",
    "compilerOptions": {"types": ["node"]},
    "include": [p + "/**/*" for p in SRC_PROJECTS],
}
GEN_CONFIG = {
    "compilerOptions": {
        "strict": True,
        "target": "es2020",
        "module": "NodeNext",
        "moduleResolution": "NodeNext",
        "declaration": True,
        "declarationMap": True,
        "sourceMap": True,
        "newLine": "lf",
        "skipLibCheck": True,
        "types": [],
        "lib": ["es2020"],
    },
    "include": ["*.ts"],
}
# name -> (needs the generated program, tsc arguments relative to the tree)
CASES = {
    "compiler-dts": (False, ["-p", "src/compiler", "--outDir", "built/compiler-dts"]),
    "compiler-js": (
        False,
        ["-p", "src/compiler", "--outDir", "built/compiler-js"] + OVERRIDES,
    ),
    "src-js": (
        False,
        ["-p", "src/tsconfig.selfcompile.json", "--outDir", "built/src-js"] + OVERRIDES,
    ),
    "generated-large": (True, ["-p", "gen", "--outDir", "built/generated-large"]),
}
SMOKE = ("generated-large",)


def gen_program(directory, modules=60):
    """Write a deterministic program of `modules` files: generic classes, unions, mapped and conditional types."""
    os.makedirs(directory, exist_ok=True)
    for i in range(modules):
        prev = (
            f'import {{ Box{i - 1}, make{i - 1} }} from "./m{i - 1}.js";\n' if i else ""
        )
        prev_field = f"    prev?: Box{i - 1}<T>;" if i else "    prev?: undefined;"
        tags = " | ".join(f'"t{i}_{j}"' for j in range(24))
        lines = [
            prev,
            f"export type Tag{i} = {tags};",
            f"export type Fields{i}<T> = {{ [K in Tag{i}]: T extends string ? K : T }};",
            f"export interface Box{i}<T> {{",
            "    value: T;",
            prev_field,
            f"    tag: Tag{i};",
            f"    fields: Fields{i}<T>;",
            "}",
            f"export class Holder{i}<T extends object> {{",
            f"    items: Box{i}<T>[] = [];",
            f"    add(item: Box{i}<T>): this {{ this.items.push(item); return this; }}",
            f"    map<U>(f: (x: Box{i}<T>) => U): U[] {{ return this.items.map(f); }}",
            "}",
            f"export function make{i}<T>(value: T, tag: Tag{i}): Box{i}<T> {{",
            f"    return {{ value, tag, fields: {{}} as Fields{i}<T> }};",
            "}",
            f"export const sum{i} = (xs: number[]): number => xs.reduce((a, b) => a + b, {i});",
            f"export function pick{i}<K extends Tag{i}>(k: K): K extends `t{i}_${{infer N}}` ? N : never {{",
            f"    return k.slice({len(str(i)) + 2}) as never;",
            "}",
        ]
        with open(os.path.join(directory, f"m{i}.ts"), "w", encoding="utf-8") as f:
            f.write("\n".join(lines) + "\n")


def tree_spellings(tree):
    names = {tree, os.path.realpath(tree)}
    home = os.path.expanduser("~")
    names |= {"~" + n[len(home) :] for n in list(names) if n.startswith(home + os.sep)}
    return sorted(names, key=len, reverse=True)


def normalise(data, tree):
    for old in tree_spellings(tree):
        data = data.replace(old.encode(), b"<TREE>")
    return data


def snapshot(directory, tree):
    """{path relative to the tree: sha256 of the normalised bytes} for every file under directory."""
    files = {}
    for dirpath, _, names in os.walk(directory):
        for n in names:
            p = os.path.join(dirpath, n)
            rel = os.path.relpath(p, tree)
            if os.path.islink(p):
                files[rel] = "link:" + os.readlink(p)
                continue
            with open(p, "rb") as f:
                files[rel] = hashlib.sha256(normalise(f.read(), tree)).hexdigest()
    return files


def run_measured(argv, cwd, env, timeout):
    """Run argv; return (exit code or 'timeout', stdout, stderr, seconds, peak RSS in MiB)."""
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        start = time.monotonic()
        proc = subprocess.Popen(
            argv, cwd=cwd, env=env, stdout=out, stderr=err, start_new_session=True
        )
        code = "timeout"
        while True:
            pid, status, usage = os.wait4(proc.pid, os.WNOHANG)
            if pid:
                code = os.waitstatus_to_exitcode(status)
                break
            if time.monotonic() - start > timeout:
                os.killpg(proc.pid, signal.SIGKILL)
                _, _, usage = os.wait4(proc.pid, 0)
                break
            time.sleep(0.05)
        secs = time.monotonic() - start
        proc.returncode = 0
        out.seek(0)
        err.seek(0)
        rss = usage.ru_maxrss / (1024 * 1024 if sys.platform == "darwin" else 1024)
        return code, out.read(), err.read(), secs, rss


def prepare(tree, ts_dir, generated):
    """Lay out a fresh tree: a copy of src/ with the self-compile config, or the generated program."""
    shutil.rmtree(tree, ignore_errors=True)
    os.makedirs(tree)
    if generated:
        gen_program(os.path.join(tree, "gen"))
        with open(os.path.join(tree, "gen", "tsconfig.json"), "w") as f:
            json.dump(GEN_CONFIG, f, indent=4)
        return
    src = os.path.join(tree, "src")
    src_root = os.path.join(ts_dir, "src")
    shutil.copytree(
        src_root,
        src,
        symlinks=True,
        ignore=lambda d, names: (
            [n for n in names if n in ("testRunner", "harness")]
            if d == src_root
            else []
        ),
    )
    with open(os.path.join(src, "tsconfig.selfcompile.json"), "w") as f:
        json.dump(SELF_CONFIG, f, indent=4)
    os.symlink(os.path.join(ts_dir, "node_modules"), os.path.join(tree, "node_modules"))


def run_case(engine, exe, tsc_js, work, ts_dir, name, timeout, env):
    generated, args = CASES[name]
    tree = os.path.join(work, engine, name)
    prepare(tree, ts_dir, generated)
    argv = [exe] + (["--node"] if engine == "jsse" else []) + [tsc_js] + args
    code, out, err, secs, rss = run_measured(argv, tree, env, timeout)
    built = snapshot(os.path.join(tree, "built"), tree)
    return {
        "exit": str(code),
        "stdout": normalise(out, tree).decode("utf-8", "replace"),
        "stderr": normalise(err, tree).decode("utf-8", "replace"),
        "files": built,
        "seconds": secs,
        "rss_mb": rss,
    }


def compare(node, jsse):
    """Lines describing how two runs of one case differ (empty when identical)."""
    lines = []
    for key in ("exit", "stdout", "stderr"):
        if node[key] != jsse[key]:
            lines.append(
                f"{key} differs:\n  node: {node[key][:300]!r}\n  jsse: {jsse[key][:300]!r}"
            )
    a, b = node["files"], jsse["files"]
    for rel in sorted(set(a) | set(b)):
        if rel not in b:
            lines.append(f"only on node: {rel}")
        elif rel not in a:
            lines.append(f"only on jsse: {rel}")
        elif a[rel] != b[rel]:
            lines.append(f"differs: {rel}")
    if not a:
        lines.append("no files were emitted")
    if node["exit"] != "0":
        lines.append(
            f"Node exited {node['exit']}: the sources are expected to compile cleanly"
        )
    return lines


def check_budget(name, node, jsse, budget, max_seconds=None, max_rss_mb=None):
    """Lines naming each budget the jsse run exceeded (empty when within budget or none is set)."""
    cfg = dict(budget.get(name) or {})
    if max_seconds is not None:
        cfg["jsse_max_seconds"] = max_seconds
    if max_rss_mb is not None:
        cfg["jsse_max_rss_mb"] = max_rss_mb
    limits = [
        ("wall time", jsse["seconds"], cfg.get("jsse_max_seconds"), "s"),
        ("peak RSS", jsse["rss_mb"], cfg.get("jsse_max_rss_mb"), " MiB"),
    ]
    for label, field, key in (
        ("wall time", "seconds", "max_time_ratio"),
        ("peak RSS", "rss_mb", "max_rss_ratio"),
    ):
        if cfg.get(key) is not None and node[field] > 0:
            limits.append((label, jsse[field] / node[field], cfg[key], "x node"))
    return [
        f"{name}: jsse {label} {got:.1f}{unit} exceeds the budget of {limit}{unit}"
        for label, got, limit, unit in limits
        if limit is not None and got > limit
    ]


def load_budget(path):
    if not os.path.isfile(path):
        return {}
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def package_version(directory):
    try:
        with open(os.path.join(directory, "package.json"), encoding="utf-8") as f:
            info = json.load(f)
    except OSError:
        return None
    return info.get("version") if info.get("name") == "typescript" else None


def check_inputs(ts_dir, tsc_dir, needs_src):
    """Return (path of _tsc.js, None) or (None, the reason the run cannot happen)."""
    if needs_src:
        if not ts_dir:
            return (
                None,
                "TS_SRC_DIR is not set; point it at a TypeScript v6.0.3 checkout (npm ci done) to run this oracle",
            )
        if not str(package_version(ts_dir)).startswith("6.0.3"):
            return None, f"{ts_dir} is TypeScript {package_version(ts_dir)}, not 6.0.3"
        for rel in (
            "src/compiler/tsconfig.json",
            "node_modules/@types/node/package.json",
        ):
            if not os.path.isfile(os.path.join(ts_dir, rel)):
                return None, f"{ts_dir} lacks {rel} (run npm ci)"
    if tsc_dir:
        if not str(package_version(tsc_dir)).startswith("6.0.3"):
            return None, f"{tsc_dir} is not an installed typescript 6.0.3 package"
        lib = os.path.join(tsc_dir, "lib")
    elif ts_dir:
        lib = os.path.join(ts_dir, "built", "local")
    else:
        return (
            None,
            "TSC_DIR is not set; point it at an installed typescript 6.0.3 package (or set TS_SRC_DIR)",
        )
    if not os.path.isfile(os.path.join(lib, "_tsc.js")):
        return (
            None,
            f"no compiler: {lib}/_tsc.js is missing (set TSC_DIR, or run `hereby local` in TS_SRC_DIR)",
        )
    return os.path.join(lib, "_tsc.js"), None


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("cases", nargs="*")
    ap.add_argument("--jsse")
    ap.add_argument("--jobs", type=int, default=1)
    ap.add_argument("--timeout", type=int, default=7200)
    ap.add_argument("--smoke", action="store_true")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--keep")
    ap.add_argument("--budget", default=BUDGET)
    ap.add_argument("--report")
    ap.add_argument("--max-seconds", type=float)
    ap.add_argument("--max-rss-mb", type=float)
    ap.add_argument("--require", action="store_true")
    ap.add_argument("--ci", action="store_true")
    o = ap.parse_args()
    if o.list:
        print("\n".join(CASES))
        return 0
    names = list(SMOKE) if o.smoke else o.cases or list(CASES)
    unknown = [n for n in names if n not in CASES]
    if unknown:
        print(f"unknown case(s): {' '.join(unknown)}", file=sys.stderr)
        return 2

    def skip(msg):
        print(
            ("ERROR: " if o.require else "SKIP: ") + msg,
            file=sys.stderr if o.require else sys.stdout,
        )
        return 1 if o.require else 0

    ts_dir = (
        os.path.realpath(os.environ["TS_SRC_DIR"])
        if os.environ.get("TS_SRC_DIR")
        else None
    )
    tsc_dir = (
        os.path.realpath(os.environ["TSC_DIR"]) if os.environ.get("TSC_DIR") else None
    )
    needs_src = any(not CASES[n][0] for n in names)
    tsc_js, problem = check_inputs(ts_dir, tsc_dir, needs_src)
    if problem:
        return skip(problem)
    jsse = os.path.abspath(
        o.jsse
        or os.environ.get("JSSE")
        or os.path.join(ROOT, "target", "release", "jsse")
    )
    if not os.access(jsse, os.X_OK):
        print(f"jsse binary not found or not executable: {jsse}", file=sys.stderr)
        return 1
    node = shutil.which("node")
    if not node:
        return skip("node is not on PATH")
    budget = load_budget(o.budget)
    work = (
        os.path.abspath(o.keep)
        if o.keep
        else tempfile.mkdtemp(prefix="jsse-tsc-selfcompile-")
    )
    os.makedirs(work, exist_ok=True)
    env = {k: v for k, v in os.environ.items() if k not in DROP_ENV}
    print(
        f"tsc self-compile from {ts_dir or 'generated sources'} (compiler {tsc_js}, node: {node}, jsse: {jsse}, work: {work})",
        flush=True,
    )

    def one(engine, exe, name):
        res = run_case(engine, exe, tsc_js, work, ts_dir, name, o.timeout, env)
        print(
            f"  {name} on {engine}: exit {res['exit']}, {res['seconds']:.1f}s, {res['rss_mb']:.0f} MiB",
            flush=True,
        )
        return res

    results = {}
    for name in names:
        results[name] = {"node": one("node", node, name)}
    with ThreadPoolExecutor(max_workers=max(o.jobs, 1)) as pool:
        futures = {n: pool.submit(one, "jsse", jsse, n) for n in names}
        for n, fut in futures.items():
            results[n]["jsse"] = fut.result()

    problems = []
    print(
        f"\n{'case':<18}{'files':>7}{'node s':>9}{'jsse s':>9}{'x':>7}{'node MiB':>10}{'jsse MiB':>10}{'x':>6}  result"
    )
    for name in names:
        a, b = results[name]["node"], results[name]["jsse"]
        diff = compare(a, b)
        over = check_budget(name, a, b, budget, o.max_seconds, o.max_rss_mb)
        problems += [f"{name}: {line}" for line in diff] + over
        print(
            f"{name:<18}{len(a['files']):>7}{a['seconds']:>9.1f}{b['seconds']:>9.1f}{b['seconds'] / max(a['seconds'], 1e-9):>7.0f}"
            f"{a['rss_mb']:>10.0f}{b['rss_mb']:>10.0f}{b['rss_mb'] / max(a['rss_mb'], 1e-9):>6.1f}  "
            + ("ok" if not diff and not over else "FAIL")
        )
    if o.report:
        slim = {
            n: {e: {k: v for k, v in r.items() if k != "files"} for e, r in rs.items()}
            for n, rs in results.items()
        }
        meta = {
            "jsse": jsse,
            "node": node,
            "load_average_at_end": os.getloadavg(),
            "cpus": os.cpu_count(),
        }
        with open(o.report, "w", encoding="utf-8") as f:
            json.dump({"host": meta, "cases": slim}, f, indent=2)
    if (
        not any(v is not None for n in names for v in (budget.get(n) or {}).values())
        and o.max_seconds is None
        and o.max_rss_mb is None
    ):
        print(
            "no budget is configured for these cases (scripts/tsc-fixtures/selfcompile-budget.json): measurements only"
        )
    if problems:
        for p in problems:
            print(
                (
                    "::error title=tsc self-compile oracle::" + p.replace("\n", "%0A")
                    if o.ci
                    else p
                ),
                file=sys.stderr,
            )
        print(
            f"\nFAIL: self-compile differs between node and jsse or exceeds its budget (work dir kept: {work})"
        )
        return 1
    if not o.keep:
        shutil.rmtree(work, ignore_errors=True)
    print("\nOK: self-compile output is byte-identical on jsse --node and Node")
    return 0


if __name__ == "__main__":
    sys.exit(main())
