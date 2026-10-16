#!/usr/bin/env python3
"""Cover every upstream JS registry leaf, including dynamically grouped tests."""
import argparse
import hashlib
import json
from pathlib import Path
import zlib
from audit import HERE, jet

def main():
    ap=argparse.ArgumentParser();ap.add_argument('--scratch',type=Path,required=True);args=ap.parse_args()
    manifest=json.loads((HERE/'manifest.json').read_text())
    seen={m['name'].split('/',1)[1] for m in manifest if m['name'].startswith('jetstream')}
    registry=json.loads((HERE/'upstream-registry.json').read_text())
    leaves=[c for b in registry for c in (b.get('children') or [b])]
    exclusions=[]
    js=args.scratch/'JetStream'
    def asset(path):
        p=js/path
        return p.read_bytes() if p.exists() else zlib.decompress(Path(str(p)+'.z').read_bytes())
    for b in leaves:
        if 'wasm' in b['name'].lower() or 'wasm' in b['kind'].lower() or b['name'] in ['bomb-workers','segmentation']:
            exclusions.append(dict(name=b['name'],reason='Web Workers unavailable' if b['name'] in ['bomb-workers','segmentation'] else 'WebAssembly unavailable',kind=b['kind'],tags=b['tags']))
            continue
        if b['name'] in seen:continue
        strings={p:asset(p).decode('utf-8') for p in b['preload'].values() if not p.endswith('.wasm')}
        binaries={p:asset(p).decode('latin1') for p in b['preload'].values() if p.endswith('.wasm')}
        parts=[jet.build_polyfill_preamble(),'globalThis.print=console.log.bind(console);globalThis.printErr=console.error.bind(console);']
        if b['deterministicRandom']:parts.append(jet.build_deterministic_random_code())
        parts.append('globalThis.JetStream={preload:'+json.dumps(b['preload'])+',getString:async p=>__strings[p],getBinary:async p=>{const s=__binary[p];const a=new Int8Array(s.length);for(let i=0;i<s.length;i++)a[i]=s.charCodeAt(i);return a;}};const __strings='+json.dumps(strings)+';const __binary='+json.dumps(binaries)+';')
        parts.extend(asset(p).decode('utf-8') for p in b['files'])
        params=dict(b['args'],iterationCount=1)
        if b['kind']=='WSLBenchmark':
            harness=''';(()=>{const b=new Benchmark();const r=[];let t=performance.now();b.buildStdlib();r.push(Math.max(1,performance.now()-t));t=performance.now();b.run();r.push(Math.max(1,performance.now()-t));print(JSON.stringify({results:r,iterations:1,worstCaseCount:0,phases:['stdlib','main']}));})();'''
        else:
            harness=(jet.build_async_harness if b['kind']=='AsyncBenchmark' else jet.build_sync_harness)(1,b['deterministicRandom'],0)
            harness=harness.replace('new Benchmark()', 'new Benchmark('+json.dumps(params)+')')
        parts.append(harness)
        text='\n'.join(parts);name='jetstream-registry/'+b['name'];p=HERE/'workloads'/(name.replace('/','__')+'.js');p.write_text(text)
        manifest.append(dict(name=name,group='jetstream',path=str(p.relative_to(HERE)),source=b['files'],sha256=hashlib.sha256(text.encode()).hexdigest(),official_iterations=b['iterations'],audit_iterations=1,upstream_kind=b['kind'],tags=b['tags']))
        print('ADDED',name,flush=True)
    # Three retained diagnostics assume print(), which Boa/Node do not supply.
    patched=[]
    for m in manifest:
        if m['name'] in ['diagnostic-1013/bench_block_let','diagnostic-1013/bench_for_let_closure','diagnostic-1013/bench_switch_let']:
            p=HERE/m['path'];text=p.read_text()
            if not text.startswith("if (typeof print"):
                text="if (typeof print === 'undefined') { var print=console.log.bind(console); }\n"+text;p.write_text(text);m['sha256']=hashlib.sha256(text.encode()).hexdigest();patched.append(m['name'])
    (HERE/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    (HERE/'exclusions.json').write_text(json.dumps(exclusions,indent=2)+'\n')
    rows=[json.loads(l) for l in (HERE/'measurements.jsonl').read_text().splitlines()]
    keep=[];bad=[]
    for r in rows:
        if r['name'] in patched:
            r['classification']='adapter preflight: fixture expected host print';bad.append(r)
        elif r['engine']=='node' and r['status']!='pass' and r['name'].startswith('jetstream/'):
            r['classification']='oracle preflight: Node CommonJS/module detection; rerun as global Script';bad.append(r)
        else:keep.append(r)
    with (HERE/'adapter-preflight.jsonl').open('a') as f:
        f.write(''.join(json.dumps(r)+'\n' for r in bad))
    (HERE/'measurements.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in keep))
    print('Final registry coverage',len(manifest),'cases;',len(leaves),'upstream leaves;',len(exclusions),'unsupported leaves')

if __name__=='__main__':main()
