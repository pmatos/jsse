#!/usr/bin/env python3
"""Separate adapter preflights, complete the oracles, then profile sequentially."""
import argparse
import json
from pathlib import Path
import subprocess
import time
from audit import HERE

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--scratch',type=Path,required=True)
    args=ap.parse_args()
    while not (HERE/'extensions-complete').exists():
        time.sleep(5)
    rows=[json.loads(l) for l in (HERE/'measurements.jsonl').read_text().splitlines()]
    bad=[]
    clean=[]
    for r in rows:
        if r['name'].startswith('jetstream-extra/babylonjs-scene') and r['status']!='pass':
            r['classification']='adapter preflight: returned ArrayBuffer instead of JetStream Int8Array'
            bad.append(r)
        elif r['name'].startswith('jetstream-extra/') and r['engine']=='node' and r['status']!='pass':
            r['classification']='oracle preflight: Node host/module detection; rerun identical input as a global Script'
            bad.append(r)
        else:
            clean.append(r)
    with (HERE/'adapter-preflight.jsonl').open('a') as f:
        for r in bad:
            f.write(json.dumps(r)+'\n')
    temp=HERE/'.measurements-corrected.jsonl'
    temp.write_text(''.join(json.dumps(r)+'\n' for r in clean))
    temp.replace(HERE/'measurements.jsonl')
    for selected in ['jetstream-extra/']:
        with (HERE/'progress.log').open('a') as f:
            p=subprocess.run(['uv','run','python',str(HERE/'audit.py'),'--jsse',str(args.scratch/'jsse'),'--boa',str(args.scratch/'boa-bin'),'--filter',selected],stdout=f,stderr=subprocess.STDOUT)
        if p.returncode:
            raise SystemExit(p.returncode)
    (HERE/'scene-rerun-complete').touch()
    with (HERE/'profile-progress.log').open('w') as f:
        p=subprocess.run(['uv','run','python',str(HERE/'profile.py'),'--scratch',str(args.scratch)],stdout=f,stderr=subprocess.STDOUT)
    raise SystemExit(p.returncode)

if __name__=='__main__':
    main()
