#!/usr/bin/env python3
"""Render the audit's machine-readable evidence as reviewable tables."""
import collections
import csv
import json
from pathlib import Path
from audit import HERE, summarize

def strong(r):
    if r.get('jsse_over_boa',0) <= 1:
        return False
    j,b = r['engines']['jsse'],r['engines']['boa']
    if r.get('metric') == 'inverse_v8_throughput_score':
        return max(j['v8_scores']) < min(b['v8_scores'])
    return j.get('min_s',0) > b.get('max_s',float('inf')) and max(j.get('median_s',0),b.get('median_s',0)) >= .03

def main():
    records = [json.loads(l) for l in (HERE/'measurements.jsonl').read_text().splitlines()]
    rs = summarize(records)
    env = json.loads((HERE/'environment.json').read_text())
    issues = json.loads((HERE/'issue-map.json').read_text()) if (HERE/'issue-map.json').exists() else {}
    profiles = {r['name']:r for r in json.loads((HERE/'profile-index.json').read_text())} if (HERE/'profile-index.json').exists() else {}
    manifest = {r['name']:r for r in json.loads((HERE/'manifest.json').read_text())}
    groups = collections.defaultdict(list)
    for r in rs:
        groups[r['name'].split('/')[0]].append(r)
    text = ['# JSSE HEAD versus Boa HEAD: benchmark audit','',
        f"JSSE `{env['jsse_head']}`; Boa `{env['boa_head']}`; JetStream `{env['jetstream_head']}`.",'',
        'Both engines were built with their default fat-LTO release profiles. Boa includes its default bundled Intl and fast allocator. The unchanged JSSE release binary is the timing target; a separate release-nolto perf-counters build is used only for execution counts. Engine binaries were snapshotted before measurements.','',
        'All measurements are sequential, pinned to CPU 0, with alternating JSSE/Boa order, three fresh CLI invocations per comparable successful workload (failed counterparts stop further timing repetitions), a 180-second timeout and an 8 GiB address-space cap. Node supplies the output/validation reference. Failed workloads are retained and excluded from ratios. Background load was present; these are diagnostic comparisons, not controlled benchmark publication results. Every sample retains load averages, exit status and output.','',
        'The JetStream audit executes one complete workload iteration in each fresh process, including init, preparation and validation. It does not run the official 4–160-iteration schedules or publish an official JetStream score. Both process wall time and inner iteration time are retained. Boa V8 workloads adapt their work to a time window: their primary ratio is inverse reported throughput, not process wall time. Tiny Boa engine scripts mostly measure CLI startup; no runtime-performance conclusions should be drawn from those rows.','',
        'The inventory covers the JavaScript benchmark scripts from both source trees, 146 retained diagnostics, and every JavaScript leaf in the pinned upstream JetStream registry: 88 attempted out of 109 total leaves. It includes the otherwise omitted startup/scene, Intl, SunSpider, WebTooling and WSL workloads, with equivalent in-memory text/binary preload adapters. The 21 excluded leaves (19 WebAssembly, two Workers) and their upstream tags are enumerated in `exclusions.json`; optional/disabled JavaScript leaves are included. The JavaScript source-map WebTooling leaf was also attempted, but JSSE cannot run its WebAssembly dependency. Rust-only parser/GC/microarchitecture benchmarks do not provide a cross-engine JavaScript workload. The extra 23 Boa expression checks make otherwise unprinted results observable; 17 paired controls isolate arithmetic, lexical loops, function calls and parameter binding.','',
        '## Coverage','',
        '| Group | Cases | JSSE success | Boa success | Both succeed |',
        '|---|---:|---:|---:|---:|']
    for group, rows in groups.items():
        jp=sum(r['engines'].get('jsse',{}).get('status')=='pass' for r in rows)
        bp=sum(r['engines'].get('boa',{}).get('status')=='pass' for r in rows)
        both=sum('jsse_over_boa' in r for r in rows)
        text.append(f'| {group} | {len(rows)} | {jp} | {bp} | {both} |')
    text += ['',f'{len(rs)} cases, {len(records)} process measurements. {len(profiles)} cases have sampling profiles and deterministic execution counters.','',
        'For output checks, the harness removes timing lines and Boa’s display of a script’s final undefined/Promise value. The original Boa engine scripts do not print their expression results; use the separate `boa-validation` rows for those correctness checks. Intl benchmarks returning undefined only establish successful execution, not locale-output equivalence.','',
        '## Findings','', '[GitHub audit tracker](https://github.com/pmatos/jsse/issues/1034) links the findings and summarizes scope, controls and exclusions.', '',
        '| Finding | Issue |', '|---|---|']
    unique={v['url']:v for v in issues.values() if isinstance(v,dict) and 'url' in v}
    for url,v in unique.items():
        text.append(f"| {v.get('title','Finding')} | {url} |")
    text += ['', 'Two confirmed bugs and eleven performance findings were opened; each issue includes a reproduction, measurements, profile/counter evidence, a potential cause and semantic constraints. All 107 observed process/throughput slowdowns map to an investigated issue. Tiny or overlapping observations are attached as evidence without asserting a demonstrated defect.', '', '## Measured JSSE slowdowns','',
        'Ratios greater than 1 mean JSSE is slower. “Separated” means the three observed ranges do not overlap and the workload exceeds the 30 ms startup floor; it is not a statistical confidence interval. Small or overlapping differences remain observations, not established performance defects. V8 rows use reported scores and their score ranges. Every observed slowdown is profiled, including differences that remain within noise.','',
        '| Workload | Metric | JSSE median | Boa median | Slowdown | Separated | Finding | Profile |',
        '|---|---|---:|---:|---:|:---:|---|---|']
    for r in sorted(rs,key=lambda r:r.get('jsse_over_boa',0),reverse=True):
        if r.get('jsse_over_boa',0)<=1 or r['name'].startswith('boa-validation/'):
            continue
        j,b=r['engines']['jsse'],r['engines']['boa']
        issue=issues.get(r['name'],{})
        url=issue.get('url','') if isinstance(issue,dict) else issue
        profile=profiles.get(r['name'],{}).get('self_profile','')
        metric='V8 score' if r.get('metric')=='inverse_v8_throughput_score' else 'seconds'
        jv=j['v8_score_median'] if metric=='V8 score' else j['median_s']
        bv=b['v8_score_median'] if metric=='V8 score' else b['median_s']
        text.append(f"| {r['name']} | {metric} | {jv:.6f} | {bv:.6f} | {r['jsse_over_boa']:.3f} | {'yes' if strong(r) else 'no'} | {url} | {'[profile]('+profile+')' if profile else ''} |")
    text += ['', '## Phases and longer paired controls','',
        'The opmix aggregate slightly favors JSSE, but that hides slower arithmetic and call phases. The longer paired controls and their independent profiles are the basis of #1023/#1030.', '',
        '| Op-mix phase | JSSE median ms | Boa median ms | JSSE / Boa |', '|---|---:|---:|---:|']
    op=next(r for r in rs if r['name']=='jsse/bench_opmix')
    for key,ratio in op['subbenchmark_ratios'].items():
        text.append(f"| {key} | {op['engines']['jsse']['subbenchmark_ms'][key]['median']:.0f} | {op['engines']['boa']['subbenchmark_ms'][key]['median']:.0f} | {ratio:.3f} |")
    text += ['', '## Differential diagnosis and falsification','',
        '- **Prettier:** JSSE fails in both execution modes. Reduced to an async function returning a function declared after return, and a generator using a function declared after yield. Whole-body declaration instantiation is missing before state-machine fragments execute. Node and Boa pass. See #1032 and `issues/*hoist-evidence.json` / `issues/async-function-minimal-evidence.json`.',
        '- **SSR console:** HTML validation succeeds, but JSSE emits logging that the workload disabled through `globalThis.console`. A lexical console binding is disconnected from the global object. This is a host compatibility defect (#1021), not a core ECMAScript conformance claim.',
        '- **PluralRules:** a separate 500,000-select probe validates the checksum on all engines and takes 0.265434 s versus Boa 0.138281 s (1.920x). Locale parsing/fallback and rule reconstruction dominate a substantial part of the profile; zero AST work/GC/slot misses. See #1033 and `issues/pluralrules-*`.',
        '- **Lexical free binding:** moving only n=3000 into the called function reduces the five-repeat JSSE/Boa ratio from 1.191 to 1.061. The independent var/let loop pair is 0.917 versus 1.540. See `investigation-measurements.json` and #1022.',
        '- **Allocator falsification:** preloading jemalloc leaves the closure gap unchanged: JSSE 0.066146 s versus 0.066581 s with jemalloc; Boa 0.035369 s. Constructor churn is unchanged too. An allocator swap alone is not supported as the fix (#1028).',
        '- **DateTimeFormat validation:** the upstream aggregate minimum-length assertion fails for JSSE (435674 versus required 438000) and Boa (352316). An 800-case follow-up over 25 locales, 16 style combinations and local/UTC zones has zero errors and formatToParts joined text equals format in all cases on all engines. Locale text/data differ; no spec defect has been established from this length floor. Evidence: `issues/intl-consistency-*.json`.',
        '- **WSL:** JSSE completes the entire validation workload in 147.70 s; Boa rejects before execution. There is no valid comparative speed ratio. A redundant repetition was canceled after the completed validated run; it is recorded separately in `canceled-runs.json`.',
        '- **Oracle/harness corrections:** Node executes bundles as classic Scripts with vm, and binary scene preloads return the upstream Int8Array shape. Three historical print-only diagnostics received an equivalent print shim. Superseded adapter results are retained in `adapter-preflight.jsonl` and are not counted as engine bugs.',
        '- **Boa expression scripts:** most original tiny core scripts leave a final expression that Boa displays and Node/JSSE do not. The textual mismatches in those rows are CLI display behavior. All 23 separate expression-output validation variants pass on both engines. Undefined-returning Intl scripts establish completion only, not output equivalence.',
        '- **Profile interpretation:** fat LTO folds some identical helpers into generator-named symbols even in generator-free programs; these names alone do not establish generator execution. Sampling profiles and counter runs are separate evidence, and counter-run GC times are never divided by default-binary wall times.',
        '', '## Selected JSSE wins','',
        'These cold workload results provide context, not an overall engine ranking. No pooled geomean is reported: overlapping diagnostics, startup costs and different score definitions would bias it.', '',
        '| Workload | JSSE seconds | Boa seconds | Boa / JSSE |', '|---|---:|---:|---:|']
    for name in ['jetstream-extra/babylonjs-startup-es5','jetstream-extra/babylonjs-startup-es6','jetstream-extra/jsdom-d3-startup','jetstream/mandreel','jetstream-registry/regex-dna-SP','audit-controls/loop-var']:
        r=next((r for r in rs if r['name']==name),None)
        if r and 'jsse_over_boa' in r:
            text.append(f"| {name} | {r['engines']['jsse']['median_s']:.6f} | {r['engines']['boa']['median_s']:.6f} | {1/r['jsse_over_boa']:.3f} |")
    text += ['', '## Execution failures','', 'Errors and benchmark-validation failures are retained as failures, never speedups. Boa failures are recorded for comparison; the requested issue scope is JSSE. The JSSE failures are classified above.', '']
    for r in rs:
        for engine,e in r['engines'].items():
            if e['status']=='pass':
                continue
            text += [f"- `{r['name']}` on {engine}: {e['status']}. `{(e.get('output','') or e.get('stderr','')).splitlines()[0][:220] if (e.get('output','') or e.get('stderr','')) else 'No output'}`"]
    text += ['', '## Reproduction and evidence','',
        '- `environment.json`: full revisions, build commands, compiler/Node versions, CPU information and binary hashes.',
        '- `manifest.json`: every source path, generated script path, SHA-256 and original JetStream iteration count. `workloads.tar.zst` retains all immutable sources; scripts extract it automatically if workloads/ is absent.',
        '- `upstream-registry.json` and `exclusions.json`: complete upstream registry inventory and unsupported leaves.',
        '- `measurements.jsonl`: every timed process, output, status and contemporaneous load averages.',
        '- `summary.json` and `results.csv`: medians, min/max ranges, validation outcomes and ratios.',
        '- `profiles/`: self/inclusive sampling reports and deterministic execution counters. `profile-index.json` points to raw sampling data in the temporary audit directory.',
        '- `issues/`: exact issue bodies and minimized repros. `issue-map.json` maps workloads to issue URLs.',
        '', 'Build the pinned engines, then run from this directory (the harness resumes existing results; use a copied evidence directory with measurements.jsonl removed for a fresh run):', '', '```sh',
        'uv run python audit.py --jsse /path/to/jsse --boa /path/to/boa',
        'uv run python profile.py --scratch /path/to/audit-scratch',
        'uv run python report.py', '```','',
        'The retained workload scripts are immutable inputs to this run. The audit changed no engine implementation, spec/test262 submodule or conformance baseline. The full test262 suite was not rerun: this is a benchmark and differential-validation audit, not a new conformance certification.']
    (HERE/'README.md').write_text('\n'.join(text)+'\n')
    with (HERE/'results.csv').open('w') as f:
        w=csv.writer(f); w.writerow(['workload','jsse_status','boa_status','jsse_median_s','boa_median_s','primary_metric','jsse_over_boa','jsse_inner_ms','boa_inner_ms','inner_ratio','ranges_separated','issue'])
        for r in rs:
            j,b=r['engines'].get('jsse',{}),r['engines'].get('boa',{})
            issue=issues.get(r['name'],{})
            w.writerow([r['name'],j.get('status'),b.get('status'),j.get('median_s'),b.get('median_s'),r.get('metric'),r.get('jsse_over_boa'),j.get('iteration_ms_median'),b.get('iteration_ms_median'),r.get('iteration_time_ratio'),strong(r),issue.get('url') if isinstance(issue,dict) else issue])

if __name__=='__main__':
    main()
