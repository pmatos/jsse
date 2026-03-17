#!/usr/bin/env python3
"""Generate bar-plot comparison charts from benchmark results."""

import csv
import os
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
CSV_PATH = os.path.join(SCRIPT_DIR, "results.csv")
OUT_DIR = SCRIPT_DIR

ENGINE_LABELS = {
    "node_v18.20.4": "Node.js v18.20.4",
    "boa_v0.21.0": "Boa v0.21.0",
    "jsse_v0.1.0": "JSSE v0.1.0",
}
COLORS = {
    "node_v18.20.4": "#68A063",
    "boa_v0.21.0": "#E44D26",
    "jsse_v0.1.0": "#3178C6",
}

with open(CSV_PATH) as f:
    reader = csv.DictReader(f)
    rows = list(reader)

engines = [c for c in rows[0] if c != "benchmark"]
benchmarks = [r["benchmark"] for r in rows]
data = {e: [float(r[e]) for r in rows] for e in engines}

# --- Chart 1: All benchmarks, log scale ---
fig, ax = plt.subplots(figsize=(12, 6))
x = np.arange(len(benchmarks))
width = 0.25

for i, engine in enumerate(engines):
    bars = ax.bar(
        x + i * width, data[engine], width,
        label=ENGINE_LABELS[engine], color=COLORS[engine],
    )
    for bar, val in zip(bars, data[engine]):
        ax.text(
            bar.get_x() + bar.get_width() / 2, bar.get_height(),
            f"{val:.2f}s", ha="center", va="bottom", fontsize=7,
        )

ax.set_yscale("log")
ax.set_ylabel("Time (seconds, log scale)")
ax.set_title("JS Engine Benchmark Comparison")
ax.set_xticks(x + width)
ax.set_xticklabels(benchmarks)
ax.legend()
ax.grid(axis="y", alpha=0.3)
fig.tight_layout()
fig.savefig(os.path.join(OUT_DIR, "benchmark_all.png"), dpi=150)
plt.close(fig)

# --- Chart 2: Slowdown factor relative to Node ---
fig, ax = plt.subplots(figsize=(12, 6))
node_key = engines[0]

for i, engine in enumerate(engines[1:], 1):
    slowdowns = [data[engine][j] / data[node_key][j] for j in range(len(benchmarks))]
    bars = ax.bar(
        x + (i - 1) * 0.35, slowdowns, 0.35,
        label=ENGINE_LABELS[engine], color=COLORS[engine],
    )
    for bar, val in zip(bars, slowdowns):
        ax.text(
            bar.get_x() + bar.get_width() / 2, bar.get_height(),
            f"{val:.1f}x", ha="center", va="bottom", fontsize=8,
        )

ax.set_ylabel("Slowdown vs Node.js (×)")
ax.set_title("Slowdown Factor Relative to Node.js")
ax.set_xticks(x + 0.175)
ax.set_xticklabels(benchmarks)
ax.axhline(y=1, color="gray", linestyle="--", alpha=0.5)
ax.legend()
ax.grid(axis="y", alpha=0.3)
fig.tight_layout()
fig.savefig(os.path.join(OUT_DIR, "benchmark_slowdown.png"), dpi=150)
plt.close(fig)

# --- Chart 3: JSSE vs Boa only (log scale) ---
fig, ax = plt.subplots(figsize=(12, 6))
boa_key = engines[1]
jsse_key = engines[2]

for i, engine in enumerate([boa_key, jsse_key]):
    bars = ax.bar(
        x + i * 0.35, data[engine], 0.35,
        label=ENGINE_LABELS[engine], color=COLORS[engine],
    )
    for bar, val in zip(bars, data[engine]):
        ax.text(
            bar.get_x() + bar.get_width() / 2, bar.get_height(),
            f"{val:.2f}s", ha="center", va="bottom", fontsize=8,
        )

ax.set_yscale("log")
ax.set_ylabel("Time (seconds, log scale)")
ax.set_title("Boa vs JSSE — Direct Comparison")
ax.set_xticks(x + 0.175)
ax.set_xticklabels(benchmarks)
ax.legend()
ax.grid(axis="y", alpha=0.3)
fig.tight_layout()
fig.savefig(os.path.join(OUT_DIR, "benchmark_boa_vs_jsse.png"), dpi=150)
plt.close(fig)

print("Charts saved to", OUT_DIR)
