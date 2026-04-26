#!/usr/bin/env python3
"""Parse ccusage daily JSON export and print summary statistics.

Usage:
    uv run python scripts/parse_ccusage.py [path/to/ccusage_daily.json]

If no path is given, defaults to plan/ccusage_daily.json.

To regenerate the JSON:
    npx ccusage daily -j > plan/ccusage_daily.json
"""
import json
import sys
import os

path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), '..', 'plan', 'ccusage_daily.json')
raw = json.load(open(path))
data = raw['daily']

total_cost = sum(d['totalCost'] for d in data)
total_input = sum(d['inputTokens'] for d in data)
total_output = sum(d['outputTokens'] for d in data)
total_cache_read = sum(d['cacheReadTokens'] for d in data)
total_cache_create = sum(d['cacheCreationTokens'] for d in data)
total_tokens = sum(d['totalTokens'] for d in data)

print("=== OVERALL ===")
print(f"Days with usage: {len(data)}")
print(f"Total cost (USD): ${total_cost:,.2f}")
print(f"Input tokens: {total_input:,}")
print(f"Output tokens: {total_output:,}")
print(f"Cache read tokens: {total_cache_read:,}")
print(f"Cache creation tokens: {total_cache_create:,}")
print(f"Total tokens: {total_tokens:,}")
print()

model_totals = {}
for d in data:
    for m in d.get('modelBreakdowns', []):
        name = m['modelName']
        if name not in model_totals:
            model_totals[name] = {'cost': 0, 'output': 0, 'input': 0, 'cache_read': 0, 'cache_create': 0}
        model_totals[name]['cost'] += m.get('cost', 0)
        model_totals[name]['output'] += m.get('outputTokens', 0)
        model_totals[name]['input'] += m.get('inputTokens', 0)
        model_totals[name]['cache_read'] += m.get('cacheReadTokens', 0)
        model_totals[name]['cache_create'] += m.get('cacheCreationTokens', 0)

print("=== PER-MODEL BREAKDOWN ===")
for name, t in sorted(model_totals.items(), key=lambda x: -x[1]['cost']):
    print(f"{name}:")
    print(f"  Cost: ${t['cost']:,.2f}")
    print(f"  Output tokens: {t['output']:,}")
    print(f"  Input tokens: {t['input']:,}")
    print(f"  Cache read: {t['cache_read']:,}")
    print(f"  Cache create: {t['cache_create']:,}")
    print()

print("=== TOP 5 MOST EXPENSIVE DAYS ===")
sorted_days = sorted(data, key=lambda x: x['totalCost'], reverse=True)
for d in sorted_days[:5]:
    print(f"  {d['date']}: ${d['totalCost']:,.2f}")
