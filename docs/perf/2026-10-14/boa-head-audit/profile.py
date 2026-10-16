#!/usr/bin/env python3
"""Collect sampling profiles and deterministic counters after timing completes."""
import argparse
import json
import math
import os
from pathlib import Path
import subprocess
import time

HERE = Path(__file__).resolve().parent

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--scratch', type=Path, required=True)
    ap.add_argument('--wait', action='store_true')
    ap.add_argument('--append', action='store_true')
    args = ap.parse_args()
    manifest = {m['name']: m for m in json.loads((HERE/'manifest.json').read_text())}
    while args.wait:
        try:
            summary = json.loads((HERE/'summary.json').read_text())
        except json.JSONDecodeError:
            time.sleep(1)
            continue
        if len(summary) == len(manifest) and all('jsse' in r['engines'] and 'boa' in r['engines'] and (any(r['engines'][k]['status'] != 'pass' for k in ('jsse','boa')) or all(r['engines'][k]['n'] >= 3 for k in ('jsse','boa'))) for r in summary):
            break
        time.sleep(10)
    from audit import summarize, ensure_workloads
    ensure_workloads()
    summary = summarize([json.loads(l) for l in (HERE/'measurements.jsonl').read_text().splitlines()])
    outdir = HERE/'profiles'
    outdir.mkdir(exist_ok=True)
    env = dict(os.environ, LD_LIBRARY_PATH=str(args.scratch/'profiler/usr/lib'))
    perf = str(args.scratch/'profiler/usr/bin/perf')
    selected = [r for r in summary if r.get('jsse_over_boa',0) > 1.0 or any(v>1 for v in r.get('subbenchmark_ratios',{}).values()) or r.get('iteration_time_ratio',0)>1.0]
    results = json.loads((HERE/'profile-index.json').read_text()) if args.append and (HERE/'profile-index.json').exists() else []
    existing = {r['name'] for r in results}
    for r in selected:
        if r['name'] in existing:
            continue
        slug = r['name'].replace('/','__')
        work = HERE/manifest[r['name']]['path']
        data = args.scratch/'profiler'/(slug+'.data')
        # A repeat of fresh CLI invocations preserves the workload's script goal.
        n = min(100,max(1,math.ceil(1.0/r['engines']['jsse']['median_s'])))
        driver = 'import subprocess,sys\nfor _ in range(int(sys.argv[1])): subprocess.run(sys.argv[2:],stdout=subprocess.DEVNULL,check=True)'
        cmd = [perf,'record','-q','-F','499','-e','cpu-clock:u','--call-graph','dwarf,8192','-o',str(data),'--','taskset','-c','0','python3','-c',driver,str(n),str(args.scratch/'jsse'),str(work)]
        with (outdir/(slug+'.record.log')).open('w') as f:
            p = subprocess.run(cmd,stdout=f,stderr=subprocess.STDOUT,env=env,timeout=240)
        with (outdir/(slug+'.self.txt')).open('w') as f:
            subprocess.run([perf,'report','--stdio','--no-children','-g','none','--sort','dso,symbol','--percent-limit','0.5','-i',str(data)],stdout=f,stderr=subprocess.STDOUT,env=env,timeout=60)
        with (outdir/(slug+'.callgraph.txt')).open('w') as f:
            subprocess.run([perf,'report','--stdio','--children','-g','graph,2,caller','--sort','dso,symbol','--percent-limit','2','-i',str(data)],stdout=f,stderr=subprocess.STDOUT,env=env,timeout=60)
        with (outdir/(slug+'.counters.txt')).open('w') as f:
            subprocess.run(['taskset','-c','0',str(args.scratch/'jsse-counters'),str(work)],stdout=f,stderr=subprocess.STDOUT,timeout=240)
        results.append(dict(name=r['name'],ratio=r['jsse_over_boa'],profile_invocations=n,perf_returncode=p.returncode,data=str(data),self_profile=str((outdir/(slug+'.self.txt')).relative_to(HERE))))
        (HERE/'profile-index.json').write_text(json.dumps(results,indent=2)+'\n')
        print('PROFILED',r['name'],round(r['jsse_over_boa'],3),'x',n,flush=True)
    (HERE/'profiles-complete').touch()

if __name__ == '__main__':
    main()
