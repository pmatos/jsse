"""Measurement and budget arithmetic of run-ts-budget-oracle.py (issue #1297); no TypeScript checkout needed."""

import oracle_procs

RSS_RATIO_CAP = 6


def run_measured(argv, cwd, env, timeout):
    """Run argv as the leader of its own session; return (exit code or 'timeout', output, seconds, peak RSS MiB).

    The peak is the `ru_maxrss` that wait4 reports for the child and every descendant it reaped, so it is the
    largest single process of the tree. (Linux folds the high-water mark of the mm a vfork child borrowed, the
    driver's, into the child's at exec; the driver stays far below the smallest measured process.) The whole
    group is killed when the run ends, times out, or the oracle is interrupted (scripts/oracle_procs.py).
    """
    code, out, _, secs, rss = oracle_procs.run_measured(
        argv, cwd, env, timeout, merge_stderr=True
    )
    return (code if code == "timeout" else str(code)), out, secs, rss


def summarise(report):
    """(passed titles, failed titles, pending count) of a mocha JSON report, or None without one."""
    if not report:
        return None
    passed = [t["fullTitle"] for t in report.get("passes", [])]
    failed = [t["fullTitle"] for t in report.get("failures", [])]
    return passed, failed, len(report.get("pending", []))


def compare(case, node, jsse, known):
    """Lines describing how the two runs of a case differ in outcome (empty when they agree).

    `known` is the set of full titles allowed to pass on Node and fail on jsse. Node must pass every test; every
    test passing on Node must pass on jsse or be known, and a known test that passes fails the run (ratchet).
    """
    lines = []
    for label, run in (("Node", node), ("jsse", jsse)):
        if run["summary"] is None:
            lines.append(
                f"{case}: {label} produced no report (exit {run['exit']}):\n{run['tail']}"
            )
    if lines:
        return lines
    n_pass, n_fail, _ = node["summary"]
    j_pass, j_fail, _ = jsse["summary"]
    for label, run, failed in (("Node", node, n_fail), ("jsse", jsse, j_fail)):
        if run["exit"] != str(min(len(failed), 255)):
            lines.append(
                f"{case}: {label} exited {run['exit']} with {len(failed)} failing test(s):\n{run['tail']}"
            )
    if n_fail:
        lines.append(f"{case}: {len(n_fail)} test(s) fail on Node, e.g. {n_fail[0]}")
    if not n_pass:
        lines.append(
            f"{case}: Node ran no passing test (the selection matches nothing)"
        )
    j_set = set(j_pass)
    gone = sorted(set(n_pass) - j_set)
    unexpected = [t for t in gone if t not in known]
    if unexpected:
        lines.append(
            f"{case}: {len(unexpected)} test(s) pass on Node and fail or do not run on jsse, e.g. {unexpected[0]}"
        )
    fixed = sorted(known & j_set)
    if fixed:
        lines.append(f"{case}: known failure(s) now pass, remove them: {fixed[0]}")
    if node["local"]:
        lines.append(
            f"{case}: Node wrote {len(node['local'])} file(s) to tests/baselines/local"
        )
    return lines


def ratio(jsse, node, key):
    return jsse[key] / node[key] if node[key] > 0 else float("inf")


def check_budget(name, node, jsse, budget):
    """Lines naming each budget the jsse run exceeded (empty when within budget; a null limit is no limit)."""
    cfg = budget.get(name)
    if cfg is None:
        return [f"{name}: no entry in the budget file"]
    limits = (
        ("wall time", "seconds", "jsse_max_seconds", "max_time_ratio", "s"),
        ("peak RSS", "rss_mb", "jsse_max_rss_mb", "max_rss_ratio", " MiB"),
    )
    problems = []
    for label, field, absolute, relative, unit in limits:
        if cfg.get(absolute) is not None and jsse[field] > cfg[absolute]:
            problems.append(
                f"{name}: jsse {label} {jsse[field]:.1f}{unit} exceeds the budget of {cfg[absolute]}{unit}"
            )
        if cfg.get(relative) is not None and node[field] > 0:
            got = ratio(jsse, node, field)
            if got > cfg[relative]:
                problems.append(
                    f"{name}: jsse {label} is {got:.2f}x Node ({jsse[field]:.1f} vs {node[field]:.1f}{unit}), over the budget of {cfg[relative]}x"
                )
    return problems


def validate_budget(budget, cases):
    """Lines describing what is wrong with a budget file (empty when it covers exactly `cases`)."""
    problems = [f"{n}: case has no budget entry" for n in cases if n not in budget]
    problems += [f"{n}: budget entry has no case" for n in budget if n not in cases]
    for name, cfg in budget.items():
        rss = cfg.get("max_rss_ratio")
        if rss is None or rss > RSS_RATIO_CAP:
            problems.append(
                f"{name}: max_rss_ratio must be set and at most {RSS_RATIO_CAP} (ruling on #1297)"
            )
        if (
            cfg.get("max_time_ratio") is not None
            or cfg.get("jsse_max_seconds") is not None
        ):
            problems.append(
                f"{name}: wall time is recorded, not capped (ruling on #1297)"
            )
    return problems


def table(rows):
    """Aligned text table of per-case measurements: rows of (name, tests, node, jsse)."""
    head = f"{'case':<24}{'tests':>7}{'node s':>9}{'jsse s':>9}{'x':>7}{'node MiB':>10}{'jsse MiB':>10}{'x':>7}{'tests/s':>9}"
    lines = [head]
    for name, tests, node, jsse in rows:
        lines.append(
            f"{name:<24}{tests:>7}{node['seconds']:>9.1f}{jsse['seconds']:>9.1f}"
            f"{ratio(jsse, node, 'seconds'):>7.1f}{node['rss_mb']:>10.0f}{jsse['rss_mb']:>10.0f}"
            f"{ratio(jsse, node, 'rss_mb'):>7.2f}{tests / jsse['seconds'] if jsse['seconds'] else 0:>9.2f}"
        )
    return "\n".join(lines)
