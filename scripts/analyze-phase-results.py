#!/usr/bin/env python3
# /// script
# dependencies = ["matplotlib", "numpy"]
# ///
"""Analyze benchmark results across optimization phases and generate charts.

Usage:
    uv run scripts/analyze-phase-results.py \
        --baseline benchmark-results-baseline.json \
        --phase1 benchmark-results-phase1.json \
        --phase2 benchmark-results-phase2.json \
        --phase3 benchmark-results-phase3.json \
        --output-dir benchmark-charts
"""

import argparse
import json
import sys
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.ticker as ticker
import numpy as np


PHASE_LABELS = ["Baseline", "Phase 1\n(Strings)", "Phase 2\n(Calls)", "Phase 3\n(Regex)"]
PHASE_COLORS = ["#95a5a6", "#3498db", "#e67e22", "#e74c3c"]
SUITE_NAMES = ["sunspider", "kraken", "octane"]
ENGINE_COLORS = {"jsse": "#e74c3c", "node": "#2ecc71", "boa": "#3498db", "engine262": "#f39c12"}


def load_results(paths):
    """Load JSON results for each phase. Returns list of dicts."""
    results = []
    for p in paths:
        with open(p) as f:
            results.append(json.load(f))
    return results


def extract_jsse_times(phase_results, suite):
    """Extract {test_name: min_ms} for jsse from a single phase's results."""
    times = {}
    for entry in phase_results.get(suite, {}).get("jsse", []):
        if entry["passed"]:
            times[entry["name"]] = entry["min_ms"]
        else:
            times[entry["name"]] = None
    return times


def extract_engine_times(phase_result, suite, engine):
    """Extract {test_name: min_ms} for any engine from a single phase."""
    times = {}
    for entry in phase_result.get(suite, {}).get(engine, []):
        if entry["passed"]:
            times[entry["name"]] = entry["min_ms"]
        else:
            times[entry["name"]] = None
    return times


def compute_suite_totals(all_phases, suite):
    """Return list of total_ms for JSSE passing tests across phases (all passing)."""
    totals = []
    for phase_data in all_phases:
        times = extract_jsse_times(phase_data, suite)
        total = sum(v for v in times.values() if v is not None)
        totals.append(total)
    return totals


def compute_common_totals(all_phases, suite):
    """Return list of total_ms for JSSE, only counting tests that pass in ALL phases."""
    all_times = [extract_jsse_times(p, suite) for p in all_phases]
    all_tests = set()
    for t in all_times:
        all_tests.update(t.keys())
    common = [test for test in all_tests
              if all(t.get(test) is not None for t in all_times)]
    totals = []
    for t in all_times:
        totals.append(sum(t[test] for test in common))
    return totals, len(common)


def plot_suite_totals(all_phases, output_dir):
    """Bar chart: JSSE total time per suite across phases (common-passing only)."""
    fig, axes = plt.subplots(1, 3, figsize=(18, 6))

    for ax, suite in zip(axes, SUITE_NAMES):
        totals, count = compute_common_totals(all_phases, suite)
        x = np.arange(len(PHASE_LABELS))
        bars = ax.bar(x, [t / 1000 for t in totals], color=PHASE_COLORS, edgecolor="#333", linewidth=0.5)

        for bar, total in zip(bars, totals):
            ax.annotate(f"{total/1000:.1f}s",
                        (bar.get_x() + bar.get_width()/2, bar.get_height()),
                        textcoords="offset points", xytext=(0, 5),
                        ha="center", fontsize=9, fontweight="bold")

        ax.set_xticks(x)
        ax.set_xticklabels(PHASE_LABELS, fontsize=9)
        ax.set_ylabel("Total Time (seconds)")
        ax.set_title(f"{suite.upper()} ({count} common tests)", fontsize=13, fontweight="bold")

        if totals[0] > 0:
            pct = (1 - totals[-1] / totals[0]) * 100
            ax.annotate(f"{pct:.0f}% faster", xy=(0.95, 0.95), xycoords="axes fraction",
                        ha="right", va="top", fontsize=11, color="#27ae60", fontweight="bold")

    fig.suptitle("JSSE Performance: Common-Passing Tests Across Optimization Phases",
                 fontsize=14, fontweight="bold", y=1.02)
    plt.tight_layout()
    fig.savefig(output_dir / "suite_totals_by_phase.png", dpi=150, bbox_inches="tight")
    fig.savefig(output_dir / "suite_totals_by_phase.svg", bbox_inches="tight")
    plt.close(fig)
    print("  Saved suite_totals_by_phase.png/svg")


def plot_top_improved(all_phases, output_dir):
    """Grouped bar chart: top-10 most improved tests (baseline vs phase 3)."""
    improvements = []
    for suite in SUITE_NAMES:
        baseline_times = extract_jsse_times(all_phases[0], suite)
        final_times = extract_jsse_times(all_phases[-1], suite)

        for test, base_ms in baseline_times.items():
            final_ms = final_times.get(test)
            if base_ms and final_ms and base_ms > 100:
                speedup = base_ms / final_ms
                improvements.append({
                    "test": f"{test}\n({suite})",
                    "test_short": test,
                    "suite": suite,
                    "baseline_ms": base_ms,
                    "final_ms": final_ms,
                    "speedup": speedup,
                })

    improvements.sort(key=lambda x: x["speedup"], reverse=True)
    top = improvements[:12]

    if not top:
        return

    fig, ax = plt.subplots(figsize=(14, 8))
    y = np.arange(len(top))
    bar_height = 0.35

    baseline_vals = [e["baseline_ms"] / 1000 for e in top]
    final_vals = [e["final_ms"] / 1000 for e in top]

    ax.barh(y + bar_height/2, baseline_vals, bar_height, label="Baseline", color="#95a5a6", edgecolor="#333", linewidth=0.5)
    ax.barh(y - bar_height/2, final_vals, bar_height, label="Phase 3", color="#e74c3c", edgecolor="#333", linewidth=0.5)

    for i, entry in enumerate(top):
        ax.annotate(f"{entry['speedup']:.1f}x faster",
                    (max(baseline_vals[i], final_vals[i]), i),
                    textcoords="offset points", xytext=(5, 0),
                    fontsize=9, va="center", fontweight="bold", color="#27ae60")

    ax.set_yticks(y)
    ax.set_yticklabels([e["test"] for e in top], fontsize=9)
    ax.set_xlabel("Time (seconds)")
    ax.set_title("Top Improved Benchmarks: Baseline vs Phase 3", fontsize=13, fontweight="bold")
    ax.legend(loc="lower right")
    ax.invert_yaxis()

    plt.tight_layout()
    fig.savefig(output_dir / "top_improved.png", dpi=150, bbox_inches="tight")
    fig.savefig(output_dir / "top_improved.svg", bbox_inches="tight")
    plt.close(fig)
    print("  Saved top_improved.png/svg")


def plot_jsse_vs_boa_gap(all_phases, output_dir):
    """Line chart: JSSE/Boa ratio narrowing across phases, per suite."""
    fig, ax = plt.subplots(figsize=(10, 6))
    x = np.arange(len(PHASE_LABELS))
    suite_line_styles = {"sunspider": "-o", "kraken": "-s", "octane": "-^"}
    suite_colors = {"sunspider": "#e74c3c", "kraken": "#3498db", "octane": "#e67e22"}

    # Use the first phase's boa results as reference (boa doesn't change)
    for suite in SUITE_NAMES:
        boa_times = extract_engine_times(all_phases[0], suite, "boa")
        ratios = []
        for phase_data in all_phases:
            jsse_times = extract_jsse_times(phase_data, suite)
            # Only count tests that both engines pass
            common = [t for t in jsse_times if jsse_times[t] and boa_times.get(t)]
            if not common:
                ratios.append(None)
                continue
            jsse_total = sum(jsse_times[t] for t in common)
            boa_total = sum(boa_times[t] for t in common)
            ratios.append(jsse_total / boa_total if boa_total > 0 else None)

        valid = [(i, r) for i, r in enumerate(ratios) if r is not None]
        if valid:
            xs, ys = zip(*valid)
            ax.plot(xs, ys, suite_line_styles[suite], color=suite_colors[suite],
                    label=f"{suite.upper()}", markersize=8, linewidth=2)
            for xi, yi in zip(xs, ys):
                ax.annotate(f"{yi:.1f}x", (xi, yi), textcoords="offset points",
                            xytext=(5, 5), fontsize=9)

    ax.set_xticks(x)
    ax.set_xticklabels(PHASE_LABELS, fontsize=10)
    ax.set_ylabel("JSSE / Boa Time Ratio (lower = closer to Boa)")
    ax.set_title("JSSE vs Boa: Performance Gap Across Phases", fontsize=13, fontweight="bold")
    ax.legend(fontsize=10)
    ax.grid(True, alpha=0.3)
    ax.axhline(y=1.0, color="#27ae60", linestyle="--", alpha=0.5, label="Parity")

    plt.tight_layout()
    fig.savefig(output_dir / "jsse_vs_boa_gap.png", dpi=150, bbox_inches="tight")
    fig.savefig(output_dir / "jsse_vs_boa_gap.svg", bbox_inches="tight")
    plt.close(fig)
    print("  Saved jsse_vs_boa_gap.png/svg")


def plot_per_test_heatmap(all_phases, output_dir):
    """Heatmap: speedup factor per test across phases (baseline = 1.0)."""
    all_tests = []
    for suite in SUITE_NAMES:
        baseline = extract_jsse_times(all_phases[0], suite)
        final = extract_jsse_times(all_phases[-1], suite)
        for test in baseline:
            b = baseline.get(test)
            f = final.get(test)
            if b and f and b > 100:
                all_tests.append((suite, test, b / f))

    all_tests.sort(key=lambda x: x[2], reverse=True)
    top = all_tests[:20]

    if not top:
        return

    fig, ax = plt.subplots(figsize=(12, max(6, len(top) * 0.4)))

    data = []
    labels = []
    for suite, test, _ in top:
        baseline = extract_jsse_times(all_phases[0], suite)
        row = []
        for phase_data in all_phases:
            times = extract_jsse_times(phase_data, suite)
            b = baseline.get(test)
            t = times.get(test)
            if b and t:
                row.append(b / t)
            else:
                row.append(1.0)
        data.append(row)
        labels.append(f"{test} ({suite})")

    data = np.array(data)
    im = ax.imshow(data, aspect="auto", cmap="RdYlGn", vmin=0.8, vmax=max(data.max(), 3.0))

    ax.set_xticks(np.arange(len(PHASE_LABELS)))
    ax.set_xticklabels(PHASE_LABELS, fontsize=9)
    ax.set_yticks(np.arange(len(labels)))
    ax.set_yticklabels(labels, fontsize=8)

    for i in range(len(labels)):
        for j in range(len(PHASE_LABELS)):
            ax.text(j, i, f"{data[i, j]:.1f}x", ha="center", va="center", fontsize=8,
                    color="white" if data[i, j] > 2 else "black")

    ax.set_title("Speedup Factor vs Baseline (green = faster)", fontsize=13, fontweight="bold")
    fig.colorbar(im, ax=ax, label="Speedup factor")

    plt.tight_layout()
    fig.savefig(output_dir / "speedup_heatmap.png", dpi=150, bbox_inches="tight")
    fig.savefig(output_dir / "speedup_heatmap.svg", bbox_inches="tight")
    plt.close(fig)
    print("  Saved speedup_heatmap.png/svg")


def print_text_report(all_phases):
    """Print a text summary table."""
    print(f"\n{'='*100}")
    print(f"  JSSE OPTIMIZATION PROGRESS REPORT")
    print(f"{'='*100}")

    for suite in SUITE_NAMES:
        print(f"\n  {suite.upper()}")
        print(f"  {'Test':<35s}", end="")
        for label in ["Baseline", "Phase 1", "Phase 2", "Phase 3"]:
            print(f" {label:>12s}", end="")
        print(f" {'Speedup':>10s}")
        print(f"  {'-'*35}" + f" {'-'*12}" * 4 + f" {'-'*10}")

        baseline_times = extract_jsse_times(all_phases[0], suite)
        for test in sorted(baseline_times.keys()):
            row = f"  {test:<35s}"
            values = []
            for phase_data in all_phases:
                times = extract_jsse_times(phase_data, suite)
                ms = times.get(test)
                values.append(ms)
                if ms is not None:
                    if ms >= 1000:
                        row += f" {ms/1000:>10.1f}s "
                    else:
                        row += f" {ms:>10.1f}ms"
                else:
                    row += f" {'FAIL/TO':>12s}"

            base = values[0]
            final = values[-1]
            if base and final:
                speedup = base / final
                if speedup > 1.05:
                    row += f" {speedup:>8.1f}x  "
                else:
                    row += f" {'~1.0x':>10s}"
            else:
                row += f" {'N/A':>10s}"
            print(row)

        totals = compute_suite_totals(all_phases, suite)
        row = f"  {'TOTAL':<35s}"
        for t in totals:
            row += f" {t/1000:>10.1f}s "
        if totals[0] > 0 and totals[-1] > 0:
            row += f" {totals[0]/totals[-1]:>8.1f}x  "
        print(row)

    # Overall summary — common-passing tests only
    print(f"\n  {'='*80}")
    print(f"  SUMMARY (common-passing tests only — apples-to-apples comparison)")
    print(f"  {'='*80}")
    for suite in SUITE_NAMES:
        common_totals, common_count = compute_common_totals(all_phases, suite)
        if common_totals[0] > 0:
            pct = (1 - common_totals[-1] / common_totals[0]) * 100
            print(f"  {suite.upper():12s}: {common_totals[0]/1000:.1f}s -> {common_totals[-1]/1000:.1f}s "
                  f"({pct:.0f}% faster, {common_totals[0]/common_totals[-1]:.1f}x speedup) "
                  f"[{common_count} common tests]")

    print(f"\n  Pass count progression:")
    for suite in SUITE_NAMES:
        counts = []
        for phase_data in all_phases:
            times = extract_jsse_times(phase_data, suite)
            counts.append(sum(1 for v in times.values() if v is not None))
        total = len(extract_jsse_times(all_phases[0], suite))
        print(f"  {suite.upper():12s}: {' -> '.join(f'{c}/{total}' for c in counts)}")


def main():
    parser = argparse.ArgumentParser(description="Analyze benchmark results across optimization phases")
    parser.add_argument("--baseline", required=True, help="Baseline results JSON")
    parser.add_argument("--phase1", required=True, help="Phase 1 results JSON")
    parser.add_argument("--phase2", required=True, help="Phase 2 results JSON")
    parser.add_argument("--phase3", required=True, help="Phase 3 results JSON")
    parser.add_argument("--output-dir", default="benchmark-charts", help="Output directory for charts")
    args = parser.parse_args()

    all_phases = load_results([args.baseline, args.phase1, args.phase2, args.phase3])

    print_text_report(all_phases)

    output_dir = Path(args.output_dir)
    output_dir.mkdir(exist_ok=True)
    print(f"\nGenerating charts in {output_dir}/...")
    plot_suite_totals(all_phases, output_dir)
    plot_top_improved(all_phases, output_dir)
    plot_jsse_vs_boa_gap(all_phases, output_dir)
    plot_per_test_heatmap(all_phases, output_dir)
    print("\nDone!")


if __name__ == "__main__":
    main()
