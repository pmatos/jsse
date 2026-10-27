"""Normalisers of the tsc projects oracle for output whose values move from run to run (#1247).

Only the moving numbers go: line labels and their order, every count, and the structure of a
trace or CPU profile stay compared."""

import json
import re

STAT_LINE = re.compile(r"^([A-Za-z/ ]+?):[ \t]+(\d+(?:\.\d+)?)([sK]?)$", re.MULTILINE)
TRACE_FILE = re.compile(r"(^|/)trace(\.[^/]*)?\.json$")
PROFILE_FILE = re.compile(r"\.cpuprofile$")


def normalise_stats(text):
    """--diagnostics and --extendedDiagnostics: times and memory become <TIME> and <MEM>, and the
    padding that depends on the widest value collapses; counts are kept."""

    def one(m):
        unit = {"s": "<TIME>", "K": "<MEM>"}.get(m.group(3))
        return f"{m.group(1)}: {unit or m.group(2)}"

    return STAT_LINE.sub(one, text)


def normalise_trace(text):
    """--generateTrace: tsc writes a complete 'X' event only for a span that outlives a 10 ms
    sampling interval, so how many there are depends on speed. The metadata and begin/end events
    stay, without their timestamps."""
    events = json.loads(text)
    kept = [
        {k: v for k, v in e.items() if k not in ("ts", "dur")}
        for e in events
        if e.get("ph") != "X"
    ]
    return "".join(json.dumps(e) + "\n" for e in kept)


def normalise_profile(text):
    """--generateCpuProfile: Node samples the run, jsse has no profiler and records none, so only
    the shape is compared."""
    p = json.loads(text)
    nodes = p["nodes"]
    shape = {
        "keys": sorted(p),
        "keys of every node": sorted(set.intersection(*(set(n) for n in nodes))),
        "call frame keys": sorted(
            set.intersection(*(set(n["callFrame"]) for n in nodes))
        ),
        "root": nodes[0]["callFrame"]["functionName"],
        "ids unique": len({n["id"] for n in nodes}) == len(nodes),
        "samples and deltas match": len(p["samples"]) == len(p["timeDeltas"]),
        "times ordered": 0 <= p["startTime"] <= p["endTime"],
    }
    return json.dumps(shape, indent=1) + "\n"


def normalise_file(path, text):
    for pattern, normalise in (
        (TRACE_FILE, normalise_trace),
        (PROFILE_FILE, normalise_profile),
    ):
        if pattern.search(path):
            try:
                return normalise(text)
            except (ValueError, KeyError, TypeError, IndexError):
                return "<unparsable>\n" + text
    return text
