#!/usr/bin/env python3
"""Cover JetStream host-adapter omissions, expression results, and paired controls."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time
import zlib
from audit import HERE, ROOT, jet

def prepare(base):
    manifest = json.loads((HERE/'manifest.json').read_text())
    manifest = [m for m in manifest if not m['name'].startswith(('jetstream-extra/','boa-validation/','audit-controls/'))]
    def add(name,group,text,sources):
        path = HERE/'workloads'/(name.replace('/','__')+'.js')
        path.write_text(text)
        manifest.append(dict(name=name,group=group,path=str(path.relative_to(HERE)),source=sources,sha256=hashlib.sha256(text.encode()).hexdigest()))
    js = base/'JetStream'
    def asset(name):
        p = js/name
        if p.exists():
            return p.read_bytes()
        return zlib.decompress(Path(str(p)+'.z').read_bytes())
    startup = ['utils/StartupBenchmark.js']
    specs = [
        ('mobx-startup',startup+['mobx/benchmark.js'],{'BUNDLE':'mobx/dist/bundle.es6.min.js'},{},{}),
        ('jsdom-d3-startup',startup+['jsdom-d3-startup/benchmark.js'],{'BUNDLE':'jsdom-d3-startup/dist/bundle.min.js','US_DATA':'jsdom-d3-startup/data/counties-albers-10m.json','AIRPORTS':'jsdom-d3-startup/data/airports.csv'},{},{}),
        ('web-ssr',startup+['web-ssr/benchmark.js'],{'BUNDLE':'web-ssr/dist/bundle.min.js'},{},{}),
        ('typescript-lib',['TypeScript/src/mock/sys.js','TypeScript/dist/bundle.js','TypeScript/benchmark.js'],{'tsconfig':'TypeScript/src/gen/immer-tiny/tsconfig.json','files':'TypeScript/src/gen/immer-tiny/files.json'},{},{}),
        ('babylonjs-startup-es5',startup+['babylonjs/benchmark/startup.js'],{'BUNDLE':'babylonjs/dist/bundle.es5.min.js'},{},{'expectedCacheCommentCount':23988}),
        ('babylonjs-startup-es6',startup+['babylonjs/benchmark/startup.js'],{'BUNDLE':'babylonjs/dist/bundle.es6.min.js'},{},{'expectedCacheCommentCount':21222}),
        ('babylonjs-scene-es5',['babylonjs/dist/bundle.es5.min.js','babylonjs/benchmark/scene.js'],{'PARTICLES_BLOB':'babylonjs/data/particles.json'},{'PIRATE_FORT_BLOB':'babylonjs/data/pirateFort.glb','CANNON_BLOB':'babylonjs/data/cannon.glb'},{}),
        ('babylonjs-scene-es6',['babylonjs/dist/bundle.es6.min.js','babylonjs/benchmark/scene.js'],{'PARTICLES_BLOB':'babylonjs/data/particles.json'},{'PIRATE_FORT_BLOB':'babylonjs/data/pirateFort.glb','CANNON_BLOB':'babylonjs/data/cannon.glb'},{}),
        ('threejs',['threejs/three.js','threejs/benchmark.js'],{},{},{}),
    ]
    for name,files,strings,binaries,args in specs:
        contents = {p:asset(p).decode('utf-8') for p in strings.values()}
        binary = {p:asset(p).decode('latin1') for p in binaries.values()}
        parts = [jet.build_polyfill_preamble(), 'globalThis.print = console.log.bind(console); globalThis.printErr = console.error.bind(console);',jet.build_deterministic_random_code()]
        parts.append('globalThis.JetStream = {preload:'+json.dumps(dict(strings,**binaries))+',getString:async p=>__strings[p],getBinary:async p=>{const s=__binary[p];const a=new Int8Array(s.length);for(let i=0;i<s.length;i++)a[i]=s.charCodeAt(i);return a;}}; const __strings='+json.dumps(contents)+';const __binary='+json.dumps(binary)+';')
        parts.extend(asset(f).decode('utf-8') for f in files)
        harness = (jet.build_sync_harness if name=='threejs' else jet.build_async_harness)(1,True,0)
        params = dict(iterationCount=1,**args)
        harness = harness.replace('new Benchmark()', 'new Benchmark('+json.dumps(params)+')')
        parts.append(harness)
        # Preserve output when a workload replaces console.log.
        add('jetstream-extra/'+name,'jetstream','\n'.join(parts),files)
    for p in sorted((base/'boa/core/engine/benches/bench_scripts').glob('*.js')):
        expr = p.read_text().strip().removesuffix(';')
        text = 'var __value = ('+expr+');\nconsole.log(JSON.stringify([typeof __value,Object.prototype.toString.call(__value),typeof __value === "symbol" ? String(__value) : __value]));\n'
        add('boa-validation/'+p.stem,'boa-engine',text,str(p))
    opmix = (ROOT/'benchmarks/scripts/bench_opmix.js').read_text().split('bench("arith"')[0]
    for label,n in [('arith','N*10'),('elem','N*5'),('called','N*5'),('mixed','N*3'),('labeledLoop','N*5'),('labeledControl','N*50')]:
        add('audit-controls/opmix-'+label,'diagnostic-1013',opmix+f'bench("{label}", {label}, {n});\n','benchmarks/scripts/bench_opmix.js')
    for kind in ['var','let']:
        add('audit-controls/loop-'+kind,'diagnostic-1013',f'function main(n){{{kind} sum=0;for({kind} i=0;i<n;i++)sum+=i&31;return sum;}}\nconsole.log(main(5000000));\n','paired local loop control')
    for strict in [False,True]:
        for captured in [False,True]:
            prefix = '"use strict";\n' if strict else ''
            fn = 'function make(x){return function leaf(y){return x+y;};}var leaf=make(1);' if captured else 'function leaf(y){return 1+y;}'
            text = prefix+fn+'\nfunction main(n){var sum=0;for(var i=0;i<n;i++)sum+=leaf(i);return sum;}console.log(main(3000000));\n'
            add('audit-controls/call-'+('strict' if strict else 'sloppy')+'-'+('captured' if captured else 'local'),'diagnostic-1013',text,'paired call control')
    for label,fn,call in [
        ('params-simple','function add(a,b){return a+b;}','add(i,1)'),
        ('params-default-supplied','function add(a,b=1){return a+b;}','add(i,1)'),
        ('params-default-omitted','function add(a,b=1){return a+b;}','add(i)'),
        ('args-parameters','function add(a,b,c){return a+b+c;}','add(i,1,2)'),
        ('args-object','function add(){var s=0;for(var j=0;j<arguments.length;j++)s+=arguments[j];return s;}','add(i,1,2)'),
    ]:
        text=fn+f'\nfunction main(){{var sum=0;for(var i=0;i<1000000;i++)sum+={call};return sum;}}console.log(main());\n'
        add('audit-controls/'+label,'diagnostic-1013',text,'paired parameter control')
    (HERE/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print('Extended manifest:',len(manifest),'workloads',flush=True)

def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--scratch',type=Path,required=True)
    ap.add_argument('--prepare-only',action='store_true')
    args=ap.parse_args()
    if args.prepare_only:
        prepare(args.scratch)
        return
    while True:
        try:
            rs=json.loads((HERE/'summary.json').read_text())
        except json.JSONDecodeError:
            time.sleep(1)
            continue
        rs=[r for r in rs if not r['name'].startswith(('jetstream-extra/','boa-validation/','audit-controls/'))]
        if len(rs)==263 and all(all(v['n']>=3 or v['status']!='pass' for k,v in r['engines'].items() if k!='node') and 'jsse' in r['engines'] and 'boa' in r['engines'] for r in rs):
            break
        time.sleep(5)
    # The base runner finishes its final atomic write immediately after this state.
    time.sleep(2)
    for selected in ['jetstream-extra/','boa-validation/','audit-controls/']:
        with (HERE/'progress.log').open('a') as f:
            p=subprocess.run(['uv','run','python',str(HERE/'audit.py'),'--jsse',str(args.scratch/'jsse'),'--boa',str(args.scratch/'boa-bin'),'--filter',selected],stdout=f,stderr=subprocess.STDOUT)
        if p.returncode:
            raise SystemExit(p.returncode)
    (HERE/'extensions-complete').touch()

if __name__=='__main__':
    main()
