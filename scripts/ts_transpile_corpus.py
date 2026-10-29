"""Corpus differential of scripts/run-ts-transpile-oracle.py (issue #1287).

The transpile runner has 22 files. `--corpus N` goes wider: scripts/ts-transpile-corpus.js runs `ts.transpileModule`
(six option sets: target/module/jsx combinations, source maps, `isolatedModules`, `verbatimModuleSyntax`, decorator
metadata with helpers) and `ts.transpileDeclaration` over N test files of the checkout (an even stride through the
sorted `.ts`/`.tsx`/`.mts`/`.cts` files of tests/cases/{transpile,compiler,conformance}; `all` takes every one) on Node
and on `jsse --node`, and the digest of every output (text, source map, diagnostics) must be equal.
"""

import os

import oracle_procs

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
DRIVER = os.path.join(SCRIPTS, "ts-transpile-corpus.js")
ROOTS = ("transpile", "compiler", "conformance")
EXTENSIONS = (".ts", ".tsx", ".mts", ".cts")
CHUNK = 250


def corpus_files(ts_dir, count):
    """Paths (relative to the checkout) of `count` test files, an even stride through the sorted list; None = all."""
    found = []
    for root in ROOTS:
        top = os.path.join(ts_dir, "tests", "cases", root)
        for dirpath, _, names in os.walk(top):
            for n in names:
                if n.endswith(EXTENSIONS):
                    found.append(os.path.relpath(os.path.join(dirpath, n), ts_dir))
    found.sort()
    if count is None or count >= len(found):
        return found
    if count <= 0:
        return []
    return [found[i * len(found) // count] for i in range(count)]


def chunks(files, size=CHUNK):
    return [files[i : i + size] for i in range(0, len(files), size)]


def run_chunk(engine, exe, ts_dir, files, work, tag, timeout):
    """Run the driver on `files`; returns ({(file, option set): 'digest<TAB>length'}, error text or None)."""
    listing = os.path.join(work, f"{tag}.list")
    with open(listing, "w", encoding="utf-8") as f:
        f.write("\n".join(files) + "\n")
    cmd = [exe] + (["--node"] if engine == "jsse" else [])
    cmd += [DRIVER, ts_dir, listing]
    env = {
        k: v for k, v in os.environ.items() if k not in ("NODE_OPTIONS", "FORCE_COLOR")
    }
    code, out, err = oracle_procs.run_group(cmd, work, env, timeout)
    got = {}
    for line in out.decode("utf-8", "replace").splitlines():
        parts = line.split("\t", 2)
        if len(parts) == 3:
            got[(parts[0], parts[1])] = parts[2]
    problem = None
    if code != "0":
        problem = (
            f"{engine} corpus chunk {tag} exited {code}: "
            + err.decode("utf-8", "replace")[-600:]
        )
    return got, problem


def compare(node, jsse, limit=20):
    """Problem lines for the two result maps of the whole corpus."""
    lines = []
    for key in sorted(set(node) | set(jsse)):
        a, b = node.get(key), jsse.get(key)
        if a != b:
            what = (
                "missing on jsse"
                if b is None
                else "missing on Node"
                if a is None
                else "differs"
            )
            lines.append(f"corpus {key[0]} [{key[1]}] {what}: node {a}, jsse {b}")
    if len(lines) > limit:
        lines = lines[:limit] + [f"corpus: {len(lines) - limit} more differences"]
    return lines


def parse_count(text):
    return None if text == "all" else int(text)
