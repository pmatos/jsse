#!/usr/bin/env python3
"""Reproducible CLI workload comparison; no engine implementation changes."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import resource
import statistics
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
sys.path.insert(0, str(ROOT / 'scripts'))
spec = importlib.util.spec_from_file_location('jetstream', ROOT / 'scripts/run-jetstream.py')
jet = importlib.util.module_from_spec(spec)
spec.loader.exec_module(jet)

MAIN = '''
;var __auditStart = Date.now();
var __auditValue = main();
var __auditMs = Date.now() - __auditStart;
if (typeof harnessErrorCount !== 'undefined' && harnessErrorCount)
    throw new Error('V8 benchmark validation failed: ' + harnessErrorCount);
var __auditText = JSON.stringify(__auditValue);
var __auditHash = 0;
if (__auditText !== undefined) for (var __auditI = 0; __auditI < __auditText.length; __auditI++)
    __auditHash = (Math.imul(__auditHash, 31) + __auditText.charCodeAt(__auditI)) | 0;
console.log('AUDIT_RESULT', typeof __auditValue, __auditText === undefined ? 0 : __auditText.length, __auditHash);
console.log('AUDIT_TIME_MS', __auditMs);
'''

def ensure_workloads():
    if not (HERE/'workloads').exists() and (HERE/'workloads.tar.zst').exists():
        subprocess.run(['tar','--zstd','-xf',str(HERE/'workloads.tar.zst'),'-C',str(HERE)],check=True)

def prepare(boa, jetdir):
    workloads = HERE / 'workloads'
    workloads.mkdir(exist_ok=True)
    manifest = []
    groups = [
        ('jsse', ROOT / 'benchmarks/scripts', 'bench_*.js', False),
        ('boa-scripts', boa / 'benches/scripts', '*.js', True),
        ('boa-engine', boa / 'core/engine/benches/bench_scripts', '*.js', False),
        ('diagnostic-1010', ROOT / 'docs/perf/2026-10-10/repros', 'bench_*.js', False),
        ('diagnostic-1013', ROOT / 'docs/perf/2026-10-13/repros', 'bench_*.js', False),
    ]
    for group, base, pattern, call_main in groups:
        for src in sorted(base.rglob(pattern)):
            name = group + '/' + src.relative_to(base).with_suffix('').as_posix()
            dst = workloads / (name.replace('/', '__') + '.js')
            text = src.read_text() + (MAIN if call_main else '')
            dst.write_text(text)
            manifest.append(dict(name=name, group=group, path=str(dst.relative_to(HERE)), source=str(src), sha256=hashlib.sha256(text.encode()).hexdigest()))
    for name, kind, files, preloads, iterations, det_random, worst in jet.BENCHMARKS:
        parts = [jet.build_polyfill_preamble()]
        if det_random:
            parts.append(jet.build_deterministic_random_code())
        if preloads:
            parts.append(jet.build_preload_code(preloads, str(jetdir)))
        parts.extend((jetdir / f).read_text(encoding='utf-8', errors='replace') for f in files)
        parts.append((jet.build_async_harness if kind == 'async' else jet.build_sync_harness)(1, det_random, worst))
        text = '\n'.join(parts)
        dst = workloads / ('jetstream__' + name + '.js')
        dst.write_text(text)
        manifest.append(dict(name='jetstream/' + name, group='jetstream', path=str(dst.relative_to(HERE)), source=files, sha256=hashlib.sha256(text.encode()).hexdigest(), official_iterations=iterations, audit_iterations=1))
    (HERE / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    return manifest

def limits():
    # Avoid an OOM affecting the user's desktop. The same cap applies to all engines.
    resource.setrlimit(resource.RLIMIT_AS, (8 * 1024**3, 8 * 1024**3))
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))

def execute(cmd, path, timeout, cpu):
    if cmd == ['node'] and path.name.startswith('jetstream'):
        # Match a JS shell's global Script environment. Node's CommonJS and
        # process globals otherwise select Node-specific bundle entry paths.
        cmd = ['node','-e','const fs=require("node:fs"),vm=require("node:vm");vm.runInNewContext(fs.readFileSync(process.argv[1],"utf8"),{console,setTimeout,clearTimeout,setInterval,clearInterval},{filename:process.argv[1]});']
    command = ['taskset', '-c', str(cpu), *cmd, str(path)]
    start = time.perf_counter()
    try:
        p = subprocess.run(command, capture_output=True, text=True, errors='replace', timeout=timeout, preexec_fn=limits)
        wall = time.perf_counter() - start
        status = 'pass' if p.returncode == 0 else 'error'
        out, err = p.stdout, p.stderr
        if re.search(r'^(?:Uncaught|ERROR )', out, re.M) or re.search(r'Uncaught|panicked at', err):
            status = 'error'
        # A dropped async rejection may exit successfully without completing.
        if path.name.startswith('jetstream'):
            lines = [l for l in out.splitlines() if l.startswith('{')]
            try:
                result = json.loads(lines[-1])
                if 'error' in result or not result.get('results'):
                    status = 'error'
            except (IndexError, ValueError):
                status = 'error'
        clipped_err = err if len(err)<=16000 else err[:4000]+'\n...[truncated code frame]...\n'+err[-12000:]
        return dict(status=status, returncode=p.returncode, wall_s=wall, stdout=out[:32000], stderr=clipped_err, loadavg=os.getloadavg())
    except subprocess.TimeoutExpired as e:
        return dict(status='timeout', wall_s=time.perf_counter()-start, returncode=None, stdout=(e.stdout or b'').decode(errors='replace')[:32000], stderr=(e.stderr or b'').decode(errors='replace')[:16000], loadavg=os.getloadavg())

def normalized(out):
    lines = []
    for line in out.splitlines():
        if line in ('undefined', 'Promise { undefined }') or line.startswith(('AUDIT_TIME_MS', 'RESULT ', 'SCORE ', 'PROGRESS ')):
            continue
        if line.startswith('{'):
            try:
                v = json.loads(line)
                if 'results' in v and 'iterations' in v:
                    line = json.dumps({'iterations': v['iterations'], 'completed': len(v['results'])})
            except ValueError:
                pass
        if line.startswith('BENCH\t'):
            f = line.split('\t')
            if len(f) == 4:
                line = '\t'.join([f[0],f[1],f[3]])
        lines.append(line)
    return '\n'.join(lines)

def summarize(records):
    by = {}
    for r in records:
        by.setdefault(r['name'], {}).setdefault(r['engine'], []).append(r)
    summary = []
    for name, engines in by.items():
        item = dict(name=name, engines={})
        for engine, samples in engines.items():
            times = [r['wall_s'] for r in samples if r['status'] == 'pass']
            ent = dict(status='pass' if all(r['status']=='pass' for r in samples) else samples[-1]['status'], n=len(samples))
            if times:
                ent.update(median_s=statistics.median(times), min_s=min(times), max_s=max(times), unstable=max(times)/min(times)-1 > .05)
            scores = [float(m.group(1)) for s in samples if (m := re.search(r'^SCORE ([\d.]+)$',s['stdout'],re.M))]
            if scores:
                ent.update(v8_score_median=statistics.median(scores), v8_scores=scores, score_unstable=max(scores)/min(scores)-1 > .05)
            sub = {}
            for s in samples:
                for label, ms, value in re.findall(r'^BENCH\t([^\t]+)\t([\d.]+)\t([^\n]+)$',s['stdout'],re.M):
                    sub.setdefault(label,[]).append(float(ms))
            if sub:
                ent['subbenchmark_ms'] = {k:dict(median=statistics.median(v), samples=v) for k,v in sub.items()}
            inner_times = []
            if name.startswith('jetstream'):
                for s in samples:
                    for line in s['stdout'].splitlines():
                        if line.startswith('{'):
                            try:
                                inner_times.extend(json.loads(line).get('results',[]))
                            except ValueError:
                                pass
            if inner_times:
                ent.update(iteration_ms_median=statistics.median(inner_times), iteration_ms=inner_times)
            ent['output'] = normalized(samples[0]['stdout'])
            ent['stderr'] = samples[0]['stderr'][:2000]
            item['engines'][engine] = ent
        j, b = engines.get('jsse', []), engines.get('boa', [])
        if j and b and all(r['status']=='pass' for r in j+b):
            item['jsse_over_boa'] = statistics.median(r['wall_s'] for r in j) / statistics.median(r['wall_s'] for r in b)
            item['metric'] = 'process_wall_time'
            if '/v8-benches/' in name and all('v8_score_median' in item['engines'][e] for e in ['jsse','boa']):
                item['wall_time_ratio'] = item['jsse_over_boa']
                item['jsse_over_boa'] = item['engines']['boa']['v8_score_median'] / item['engines']['jsse']['v8_score_median']
                item['metric'] = 'inverse_v8_throughput_score'
            if name.startswith('jetstream') and all('iteration_ms_median' in item['engines'][e] for e in ['jsse','boa']):
                item['iteration_time_ratio'] = item['engines']['jsse']['iteration_ms_median'] / item['engines']['boa']['iteration_ms_median']
            if all('subbenchmark_ms' in item['engines'][e] for e in ['jsse','boa']):
                item['subbenchmark_ratios'] = {k: v['median']/item['engines']['boa']['subbenchmark_ms'][k]['median'] for k,v in item['engines']['jsse']['subbenchmark_ms'].items() if item['engines']['boa']['subbenchmark_ms'].get(k,{}).get('median',0)>0}
        oracle = engines.get('node', [])
        if oracle and oracle[0]['status'] == 'pass':
            item['output_matches_node'] = {e: normalized(s[0]['stdout']) == normalized(oracle[0]['stdout']) for e,s in engines.items() if e != 'node' and s[0]['status']=='pass'}
        summary.append(item)
    (HERE / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    return summary

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--boa-source', type=Path)
    ap.add_argument('--jetstream', type=Path, default=Path('/tmp/JetStream'))
    ap.add_argument('--jsse')
    ap.add_argument('--boa')
    ap.add_argument('--repeats', type=int, default=3)
    ap.add_argument('--timeout', type=int, default=180)
    ap.add_argument('--cpu', type=int, default=0)
    ap.add_argument('--filter', default='')
    ap.add_argument('--prepare-only', action='store_true')
    args = ap.parse_args()
    manifest = prepare(args.boa_source, args.jetstream) if args.boa_source else json.loads((HERE / 'manifest.json').read_text())
    if args.prepare_only:
        print('Prepared', len(manifest), 'workloads', flush=True)
        return
    ensure_workloads()
    records = [json.loads(l) for l in (HERE/'measurements.jsonl').read_text().splitlines()] if (HERE/'measurements.jsonl').exists() else []
    commands = dict(jsse=[args.jsse], boa=[args.boa], node=['node'])
    completed = {(r['name'],r['engine'],r['repeat']) for r in records}
    # Broad suite first, then retained diagnostic workloads; one process at a time.
    order = {'jsse':0,'boa-scripts':1,'boa-engine':2,'jetstream':3,'diagnostic-1010':4,'diagnostic-1013':5}
    manifest.sort(key=lambda m:(order.get(m['group'],6),m['name']))
    for m in manifest:
        if args.filter not in m['name']:
            continue
        path = HERE / m['path']
        for repeat in range(args.repeats):
            if repeat > 0 and any(r['name']==m['name'] and r['engine'] in ('jsse','boa') and r['status']!='pass' for r in records):
                # A failed counterpart provides no valid speed ratio. One
                # complete validated run of the successful engine is enough.
                break
            labels = (['node'] if repeat == 0 else []) + (['jsse','boa'] if repeat%2 == 0 else ['boa','jsse'])
            for label in labels:
                key = (m['name'],label,repeat)
                if key in completed:
                    continue
                prev = [r for r in records if r['name']==m['name'] and r['engine']==label]
                if prev and prev[-1]['status'] != 'pass':
                    continue
                result = dict(name=m['name'], engine=label, repeat=repeat, **execute(commands[label],path,args.timeout,args.cpu))
                records.append(result)
                with (HERE/'measurements.jsonl').open('a') as f:
                    f.write(json.dumps(result) + '\n')
                print(m['name'],label,repeat,result['status'],round(result['wall_s'],4),flush=True)
            summarize(records)
    summarize(records)

if __name__ == '__main__':
    main()
