#!/usr/bin/env python3
# /// script
# dependencies = ["matplotlib", "numpy"]
# ///
"""Analyze benchmark results and generate comparison charts.

Usage:
    uv run scripts/analyze-benchmarks.py benchmark-results.json
"""

import argparse
import json
import sys
from pathlib import Path

try:
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    import matplotlib.ticker as ticker
    import numpy as np
    HAS_MATPLOTLIB = True
except ImportError:
    HAS_MATPLOTLIB = False
    print("WARNING: matplotlib not available, will output text-only report")


def load_results(path):
    with open(path) as f:
        return json.load(f)


def plot_category_comparison(results, output_dir):
    """Bar chart comparing all engines across categories."""
    engines = results["engines"]
    cat_summary = results["category_summary"]

    # Sort categories by total test count (descending)
    cats = sorted(cat_summary.keys(),
                  key=lambda c: max(cat_summary[c].get(e, {}).get("count", 0) for e in engines),
                  reverse=True)

    # Only show categories with >= 10 tests
    cats = [c for c in cats if any(cat_summary[c].get(e, {}).get("count", 0) >= 10 for e in engines)]

    if len(cats) > 25:
        cats = cats[:25]

    colors = {"jsse": "#e74c3c", "node": "#2ecc71", "boa": "#3498db", "engine262": "#f39c12"}

    # -- Chart 1: Total time per category --
    fig, ax = plt.subplots(figsize=(16, max(8, len(cats) * 0.4)))
    y = np.arange(len(cats))
    bar_height = 0.2
    for i, engine in enumerate(engines):
        times = [cat_summary[c].get(engine, {}).get("total_time", 0) for c in cats]
        ax.barh(y + i * bar_height, times, bar_height, label=engine,
                color=colors.get(engine, "#999"))

    ax.set_yticks(y + bar_height * (len(engines) - 1) / 2)
    ax.set_yticklabels(cats, fontsize=9)
    ax.set_xlabel("Total Time (seconds)")
    ax.set_title("test262 Performance: Total Time by Category (common-passing tests)")
    ax.legend(loc="lower right")
    ax.invert_yaxis()
    plt.tight_layout()
    fig.savefig(output_dir / "category_total_time.png", dpi=150)
    fig.savefig(output_dir / "category_total_time.svg")
    plt.close(fig)
    print(f"  Saved category_total_time.png/svg")

    # -- Chart 2: Average time per test per category --
    fig, ax = plt.subplots(figsize=(16, max(8, len(cats) * 0.4)))
    for i, engine in enumerate(engines):
        avgs = [cat_summary[c].get(engine, {}).get("avg_time", 0) * 1000 for c in cats]
        ax.barh(y + i * bar_height, avgs, bar_height, label=engine,
                color=colors.get(engine, "#999"))

    ax.set_yticks(y + bar_height * (len(engines) - 1) / 2)
    ax.set_yticklabels(cats, fontsize=9)
    ax.set_xlabel("Average Time per Test (ms)")
    ax.set_title("test262 Performance: Average Time per Test by Category")
    ax.legend(loc="lower right")
    ax.invert_yaxis()
    plt.tight_layout()
    fig.savefig(output_dir / "category_avg_time.png", dpi=150)
    fig.savefig(output_dir / "category_avg_time.svg")
    plt.close(fig)
    print(f"  Saved category_avg_time.png/svg")

    # -- Chart 3: JSSE/Boa ratio --
    jsse_vs_boa = results.get("jsse_vs_boa", [])
    if jsse_vs_boa:
        top = jsse_vs_boa[:20]
        fig, ax = plt.subplots(figsize=(14, max(6, len(top) * 0.4)))
        categories = [e["category"] for e in top]
        ratios = [e["ratio"] for e in top]
        bar_colors = ["#e74c3c" if r > 2 else "#f39c12" if r > 1 else "#2ecc71" for r in ratios]

        y = np.arange(len(categories))
        ax.barh(y, ratios, color=bar_colors, edgecolor="#333", linewidth=0.5)
        ax.axvline(x=1.0, color="#3498db", linestyle="--", linewidth=1.5, label="Boa baseline")
        ax.set_yticks(y)
        ax.set_yticklabels(categories, fontsize=9)
        ax.set_xlabel("JSSE / Boa Time Ratio (lower = JSSE faster)")
        ax.set_title("JSSE vs Boa: Performance Ratio by Category")
        ax.legend()
        ax.invert_yaxis()

        for i, (ratio, entry) in enumerate(zip(ratios, top)):
            ax.annotate(f"{ratio:.1f}x ({entry['count']} tests)",
                        (ratio, i), textcoords="offset points",
                        xytext=(5, 0), fontsize=8, va="center")

        plt.tight_layout()
        fig.savefig(output_dir / "jsse_vs_boa_ratio.png", dpi=150)
        fig.savefig(output_dir / "jsse_vs_boa_ratio.svg")
        plt.close(fig)
        print(f"  Saved jsse_vs_boa_ratio.png/svg")

    # -- Chart 4: Overall engine comparison --
    per_engine = results.get("per_engine_pass", {})
    total = results.get("total_scenarios", 0)
    if per_engine:
        fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(14, 5))

        # Pass rate
        eng_names = list(per_engine.keys())
        pass_counts = [per_engine[e] for e in eng_names]
        eng_colors = [colors.get(e, "#999") for e in eng_names]
        ax1.bar(eng_names, pass_counts, color=eng_colors, edgecolor="#333")
        ax1.set_ylabel("Tests Passed")
        ax1.set_title(f"Pass Rate (out of {total} scenarios)")
        for i, (name, count) in enumerate(zip(eng_names, pass_counts)):
            ax1.annotate(f"{count}\n({100*count/total:.1f}%)",
                        (i, count), textcoords="offset points",
                        xytext=(0, 5), ha="center", fontsize=9)

        # Total time on common tests
        common_count = results.get("common_passing", 0)
        total_times = {}
        for engine in engines:
            t = sum(cat_summary[c].get(engine, {}).get("total_time", 0) for c in cat_summary)
            total_times[engine] = t

        ax2.bar(list(total_times.keys()), list(total_times.values()),
                color=[colors.get(e, "#999") for e in total_times],
                edgecolor="#333")
        ax2.set_ylabel("Total Time (seconds)")
        ax2.set_title(f"Total Time on {common_count} Common-Passing Tests")
        for i, (name, t) in enumerate(zip(total_times.keys(), total_times.values())):
            ax2.annotate(f"{t:.1f}s", (i, t), textcoords="offset points",
                        xytext=(0, 5), ha="center", fontsize=9)

        plt.tight_layout()
        fig.savefig(output_dir / "engine_overview.png", dpi=150)
        fig.savefig(output_dir / "engine_overview.svg")
        plt.close(fig)
        print(f"  Saved engine_overview.png/svg")


def print_text_report(results):
    """Print a text summary of the results."""
    engines = results["engines"]
    cat_summary = results["category_summary"]

    print(f"\n{'='*80}")
    print(f"  BENCHMARK REPORT")
    print(f"{'='*80}")

    print(f"\n  Total scenarios: {results['total_scenarios']}")
    print(f"  Common passing:  {results['common_passing']}")
    print()

    for engine in engines:
        passed = results["per_engine_pass"].get(engine, 0)
        total = results["total_scenarios"]
        print(f"  {engine:12s}: {passed:6d}/{total} ({100*passed/total:.1f}%)")

    # Category table
    cats = sorted(cat_summary.keys(),
                  key=lambda c: max(cat_summary[c].get(e, {}).get("count", 0) for e in engines),
                  reverse=True)[:25]

    print(f"\n  {'Category':<35s}", end="")
    for engine in engines:
        print(f" {engine:>12s}", end="")
    print(f" {'Tests':>6s}")
    print(f"  {'-'*35}", end="")
    for _ in engines:
        print(f" {'-'*12}", end="")
    print(f" {'-'*6}")

    for cat in cats:
        count = max(cat_summary[cat].get(e, {}).get("count", 0) for e in engines)
        if count < 5:
            continue
        print(f"  {cat:<35s}", end="")
        for engine in engines:
            t = cat_summary[cat].get(engine, {}).get("total_time", 0)
            print(f" {t:>11.2f}s", end="")
        print(f" {count:>6d}")

    jsse_vs_boa = results.get("jsse_vs_boa", [])
    if jsse_vs_boa:
        print(f"\n  JSSE vs Boa — Worst Performance Gaps:")
        print(f"  {'Category':<35s} {'JSSE':>10s} {'Boa':>10s} {'Ratio':>8s} {'Tests':>6s}")
        print(f"  {'-'*35} {'-'*10} {'-'*10} {'-'*8} {'-'*6}")
        for entry in jsse_vs_boa[:20]:
            print(f"  {entry['category']:<35s} {entry['jsse_total']:>9.2f}s {entry['boa_total']:>9.2f}s {entry['ratio']:>7.1f}x {entry['count']:>6d}")

    # Identify low-hanging fruit
    if jsse_vs_boa:
        print(f"\n  OPTIMIZATION SUGGESTIONS (categories where JSSE can catch Boa):")
        print(f"  {'-'*70}")
        # Look for categories where JSSE is only slightly behind (1.0-3.0x)
        # and has many tests — these are easier wins
        moderate = [e for e in jsse_vs_boa if 1.2 < e["ratio"] < 5.0 and e["count"] >= 20]
        for entry in moderate[:10]:
            print(f"  - {entry['category']} ({entry['count']} tests): "
                  f"JSSE {entry['jsse_total']:.1f}s vs Boa {entry['boa_total']:.1f}s "
                  f"({entry['ratio']:.1f}x slower)")


def main():
    parser = argparse.ArgumentParser(description="Analyze benchmark results")
    parser.add_argument("results", help="Path to benchmark-results.json")
    parser.add_argument("--output-dir", default="benchmark-charts",
                        help="Output directory for charts")
    args = parser.parse_args()

    results = load_results(args.results)
    print_text_report(results)

    if HAS_MATPLOTLIB:
        output_dir = Path(args.output_dir)
        output_dir.mkdir(exist_ok=True)
        print(f"\nGenerating charts in {output_dir}/...")
        plot_category_comparison(results, output_dir)
    else:
        print("\nInstall matplotlib for charts: uv pip install matplotlib numpy")


if __name__ == "__main__":
    main()
