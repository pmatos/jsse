#!/usr/bin/env python3
"""Additional falsification probes, run after the timing and profiling phases."""
import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import time
from audit import HERE

def main():
    ap=argparse.ArgumentParser();ap.add_argument('--scratch',type=Path,required=True);args=ap.parse_args()
    root=HERE/'issues/repros';root.mkdir(exist_ok=True)
    for kind in ['free','local']:
        text=('const n=3000;\n' if kind=='free' else '')+'function main(){'+('const n=3000;' if kind=='local' else '')+'var count=0;for(let i=0;i<n;i++){for(let j=0;j<n;j++)count++;}return count;}console.log(main());\n'
        (root/('lexical-'+kind+'.js')).write_text(text)
    cases=[('lexical-'+k,root/('lexical-'+k+'.js')) for k in ['free','local']]
    cases += [('allocator-closures',HERE/'workloads/boa-scripts__closures__create.js'),('allocator-churn',HERE/'workloads/diagnostic-1010__alloc-gc__bench_alloc-gc_ctor_churn.js')]
    engines={'jsse':([str(args.scratch/'jsse')],{}),'boa':([str(args.scratch/'boa-bin')],{}),'jsse-jemalloc':([str(args.scratch/'jsse')],{'LD_PRELOAD':'/usr/lib/libjemalloc.so.2'})}
    records=[]
    for name,path in cases:
        for repeat in range(5):
            for engine in (list(engines) if repeat%2==0 else list(reversed(engines))):
                if name.startswith('lexical-') and engine=='jsse-jemalloc':continue
                cmd,extra=engines[engine];start=time.perf_counter()
                p=subprocess.run(['taskset','-c','0',*cmd,str(path)],text=True,capture_output=True,timeout=180,env=dict(os.environ,**extra))
                r=dict(case=name,engine=engine,repeat=repeat,wall_s=time.perf_counter()-start,returncode=p.returncode,stdout=p.stdout,stderr=p.stderr,loadavg=os.getloadavg())
                records.append(r);print(name,engine,repeat,round(r['wall_s'],4),p.returncode,flush=True)
                (HERE/'investigation-measurements.json').write_text(json.dumps(records,indent=2)+'\n')
    print('Medians',flush=True)
    for name,_ in cases:
        print(name,{e:statistics.median(r['wall_s'] for r in records if r['case']==name and r['engine']==e) for e in engines if any(r['case']==name and r['engine']==e for r in records)},flush=True)

if __name__=='__main__':main()
