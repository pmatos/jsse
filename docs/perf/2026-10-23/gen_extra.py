import pathlib
import sys

out = pathlib.Path(sys.argv[1])
out.mkdir(exist_ok=True)
B = {}
B["str_split_join"] = (
    "var s='alpha,beta,gamma,delta,epsilon,zeta,eta,theta'; var n=0; for(var i=0;i<N;i++){ n+=s.split(',').join('-').length; } return n;"
)
B["str_replace_fn"] = (
    "var s='the quick brown fox jumps over the lazy dog'; var n=0; for(var i=0;i<N;i++){ n+=s.replace(/o/g,function(m){return '0'}).length; } return n;"
)
B["str_indexof_slice"] = (
    "var s='abcdefghijklmnopqrstuvwxyz'.repeat(20); var n=0; for(var i=0;i<N*4;i++){ n+=s.indexOf('xyz',i%100)+s.slice(i%50,i%50+10).length+s.charCodeAt(i%500); } return n;"
)
B["str_concat_build"] = (
    "var n=0; for(var j=0;j<N/50;j++){ var s=''; for(var i=0;i<200;i++){ s+='ab'+i; } n+=s.length; } return n;"
)
B["str_template"] = (
    "var n=0; for(var i=0;i<N*2;i++){ n+=`item-${i}-${i*2}:${i%7}`.length; } return n;"
)
B["str_pad_case"] = (
    "var n=0; for(var i=0;i<N;i++){ n+=String(i).padStart(8,'0').toUpperCase().trim().length+'Hello'.toLowerCase().length; } return n;"
)
B["str_unicode"] = (
    "var s='héllo wörld ünïcödé 日本語 テキスト'; var n=0; for(var i=0;i<N;i++){ n+=s.normalize('NFD').length+s.codePointAt(i%10)+[...s].length; } return n;"
)
B["str_localecompare"] = (
    "var a=['pear','apple','fig','banana','cherry','date','elderberry','grape']; var n=0; for(var i=0;i<N/10;i++){ n+=a.slice().sort(function(x,y){return x.localeCompare(y)})[0].length; } return n;"
)
B["arr_map_filter_reduce"] = (
    "var a=[];for(var i=0;i<1000;i++)a.push(i); var n=0; for(var j=0;j<N/50;j++){ n+=a.map(x=>x*2).filter(x=>x%3==0).reduce((p,c)=>p+c,0); } return n;"
)
B["arr_sort_num"] = (
    "var n=0; for(var j=0;j<N/200;j++){ var a=[];var x=j+1;for(var i=0;i<500;i++){x=(x*1103515245+12345)&0x7fffffff;a.push(x);} a.sort((p,q)=>p-q); n+=a[0]; } return n;"
)
B["arr_sort_default"] = (
    "var n=0; for(var j=0;j<N/200;j++){ var a=[];var x=j+1;for(var i=0;i<500;i++){x=(x*1103515245+12345)&0x7fffffff;a.push(x);} a.sort(); n+=a[0]; } return n;"
)
B["arr_push_pop_shift"] = (
    "var a=[]; var n=0; for(var i=0;i<N*3;i++){ a.push(i); if(i%3==2){ n+=a.pop()+a.shift(); } } return n+a.length;"
)
B["arr_foreach_forof"] = (
    "var a=[];for(var i=0;i<1000;i++)a.push(i); var n=0; for(var j=0;j<N/50;j++){ a.forEach(function(x){n+=x}); for(var v of a){n+=v} } return n;"
)
B["arr_spread_destructure"] = (
    "var n=0; for(var i=0;i<N;i++){ var a=[i,i+1,i+2]; var b=[...a,...a]; var [x,y,...r]=b; n+=x+y+r.length; } return n;"
)
B["arr_flat_includes_find"] = (
    "var a=[[1,2],[3,[4,5]],[6]]; var n=0; for(var i=0;i<N;i++){ n+=a.flat(2).length+(a.includes(3)?1:0)+[1,2,3,4,5].find(x=>x>3)+[1,2,3].findIndex(x=>x==2); } return n;"
)
B["arr_typed"] = (
    "var f=new Float64Array(1000), u=new Uint8Array(1000); var n=0; for(var j=0;j<N/20;j++){ for(var i=0;i<1000;i++){ f[i]=i*0.5; u[i]=i; } for(var i=0;i<1000;i++){ n+=f[i]+u[i]; } } return n;"
)
B["arr_typed_methods"] = (
    "var n=0; for(var j=0;j<N/10;j++){ var t=new Int32Array(256).map((_,i)=>i*3); t.sort((a,b)=>b-a); n+=t.subarray(10,20).reduce((a,b)=>a+b,0)+t.indexOf(30); } return n;"
)
B["obj_alloc_props"] = (
    "var n=0; for(var i=0;i<N*3;i++){ var o={a:i,b:i+1,c:{d:i}}; n+=o.a+o.c.d; } return n;"
)
B["obj_keys_entries"] = (
    "var o={a:1,b:2,c:3,d:4,e:5,f:6}; var n=0; for(var i=0;i<N;i++){ n+=Object.keys(o).length+Object.entries(o).length+Object.values(o)[2]; } return n;"
)
B["obj_assign_spread"] = (
    "var o={a:1,b:2,c:3}; var n=0; for(var i=0;i<N;i++){ var p={...o,d:i}; var q=Object.assign({},p,{e:1}); n+=q.d+q.a; } return n;"
)
B["obj_dynamic_keys"] = (
    "var o={}; var n=0; for(var i=0;i<N;i++){ o['k'+(i%200)]=i; n+=o['k'+((i*7)%200)]|0; } return n;"
)
B["obj_for_in_delete"] = (
    "var n=0; for(var j=0;j<N/20;j++){ var o={}; for(var i=0;i<20;i++)o['p'+i]=i; for(var k in o){n+=o[k]} delete o.p3; delete o.p7; n+=Object.keys(o).length; } return n;"
)
B["obj_defineprop_freeze"] = (
    "var n=0; for(var i=0;i<N/2;i++){ var o={}; Object.defineProperty(o,'x',{value:i,enumerable:true}); Object.freeze(o); n+=o.x+(Object.isFrozen(o)?1:0); } return n;"
)
B["map_set_ops"] = (
    "var m=new Map(), s=new Set(); var n=0; for(var i=0;i<N*2;i++){ m.set(i%1000,i); s.add(i%777); n+=m.get((i*3)%1000)|0; if(s.has(i%500))n++; } return n+m.size+s.size;"
)
B["map_string_keys"] = (
    "var m=new Map(); var n=0; for(var i=0;i<N*2;i++){ var k='key'+(i%500); m.set(k,(m.get(k)|0)+1); } for(var [k,v] of m)n+=v; return n;"
)
B["weakmap_symbol"] = (
    "var w=new WeakMap(); var keys=[];for(var i=0;i<100;i++)keys.push({}); var n=0; for(var i=0;i<N*2;i++){ var k=keys[i%100]; w.set(k,i); n+=w.get(k); } return n;"
)
B["class_instances"] = (
    "class P{constructor(x,y){this.x=x;this.y=y} get sum(){return this.x+this.y} add(o){return new P(this.x+o.x,this.y+o.y)}} var n=0,p=new P(0,0),d=new P(1,2); for(var i=0;i<N*2;i++){ p=p.add(d); n+=p.sum; } return n;"
)
B["class_inherit_super"] = (
    "class A{constructor(x){this.x=x} m(){return this.x}} class B extends A{constructor(x){super(x);this.y=1} m(){return super.m()+this.y}} var n=0; for(var i=0;i<N*2;i++){ n+=new B(i).m(); } return n;"
)
B["class_private_static"] = (
    "class C{#v=0; static c=0; inc(){this.#v++; C.c++; return this.#v} } var o=new C(); var n=0; for(var i=0;i<N*3;i++){ n+=o.inc(); } return n;"
)
B["closure_create"] = (
    "var n=0; for(var i=0;i<N*3;i++){ var f=(function(a){return function(b){return a+b}})(i); n+=f(1); } return n;"
)
B["call_apply_bind"] = (
    "function f(a,b){return this.k+a+b} var o={k:1}; var g=f.bind(o,2); var n=0; for(var i=0;i<N*2;i++){ n+=f.call(o,i,1)+f.apply(o,[i,2])+g(3); } return n;"
)
B["args_rest_default"] = (
    "function f(a,b=2,...r){return a+b+r.length} function g(){return arguments.length+arguments[0]} var n=0; for(var i=0;i<N*2;i++){ n+=f(i)+f(i,1,2,3)+g(i,2); } return n;"
)
B["destructure_obj"] = (
    "var n=0; for(var i=0;i<N*2;i++){ var {a,b:{c},d=5}={a:i,b:{c:2}}; n+=a+c+d; } return n;"
)
B["generator_iter"] = (
    "function* g(k){for(var i=0;i<k;i++)yield i} var n=0; for(var j=0;j<N/50;j++){ for(var v of g(100))n+=v; } return n;"
)
B["iter_protocol"] = (
    "var it={[Symbol.iterator](){var i=0;return{next(){return i<100?{value:i++,done:false}:{value:undefined,done:true}}}}}; var n=0; for(var j=0;j<N/50;j++){ for(var v of it)n+=v; } return n;"
)
B["try_catch_throw"] = (
    "var n=0; for(var i=0;i<N/2;i++){ try{ if(i%2) throw new Error('x'+i); n++; }catch(e){ n+=e.message.length; } finally{ n++ } } return n;"
)
B["error_stack"] = (
    "var n=0; for(var i=0;i<N/10;i++){ var e=new Error('boom'); n+=e.stack.length>0?1:0; } return n;"
)
B["proxy_get_set"] = (
    "var p=new Proxy({a:1},{get(t,k,r){return Reflect.get(t,k,r)},set(t,k,v,r){return Reflect.set(t,k,v,r)}}); var n=0; for(var i=0;i<N*2;i++){ p.a=i; n+=p.a; } return n;"
)
B["reflect_ownkeys"] = (
    "var o={a:1,b:2,[Symbol('s')]:3}; var n=0; for(var i=0;i<N;i++){ n+=Reflect.ownKeys(o).length+Object.getOwnPropertyNames(o).length+(Reflect.has(o,'a')?1:0); } return n;"
)
B["json_roundtrip"] = (
    "var o={a:[1,2,3,{b:'x'}],c:'hello',d:{e:null,f:true,g:1.5}}; var n=0; for(var i=0;i<N/2;i++){ n+=JSON.parse(JSON.stringify(o)).a.length; } return n;"
)
B["json_big"] = (
    "var a=[];for(var i=0;i<2000;i++)a.push({id:i,name:'n'+i,tags:['a','b'],v:i*1.5}); var s=JSON.stringify(a); var n=0; for(var j=0;j<N/500;j++){ n+=JSON.parse(s).length+JSON.stringify(a).length; } return n;"
)
B["regex_exec_groups"] = (
    "var re=/(?<y>\\d{4})-(?<m>\\d{2})-(?<d>\\d{2})/; var n=0; for(var i=0;i<N;i++){ var m=re.exec('on 2024-05-17 ok'); n+=m.groups.y.length+m.index; } return n;"
)
B["regex_matchall_replace"] = (
    "var s='a1b22c333d4444'.repeat(10); var n=0; for(var i=0;i<N/10;i++){ for(var m of s.matchAll(/\\d+/g))n+=m[0].length; n+=s.replace(/(\\d)(\\d)/g,'$2$1').length; } return n;"
)
B["regex_sticky_test"] = (
    "var re=/[a-z]+\\d*/y; var s='abc12 def3 ghi'; var n=0; for(var i=0;i<N*2;i++){ re.lastIndex=0; while(re.test(s)){n++; re.lastIndex++} } return n;"
)
B["date_ops"] = (
    "var n=0; for(var i=0;i<N/2;i++){ var d=new Date(2024,0,1+i%365,i%24); n+=d.getDay()+d.getMonth()+d.toISOString().length; } return n;"
)
B["date_parse_fmt"] = (
    "var n=0; for(var i=0;i<N/4;i++){ n+=Date.parse('2024-05-17T10:20:30Z')%1000+new Date(1e12+i).toString().length+new Date(1e12+i).toUTCString().length; } return n;"
)
B["math_numeric"] = (
    "var n=0; for(var i=1;i<N*4;i++){ n+=Math.sqrt(i)+Math.sin(i)+Math.floor(i/3)+Math.max(i,5)+Math.abs(-i)+Math.pow(i,0.5)+Math.imul(i,3); } return n;"
)
B["num_tostring_parse"] = (
    "var n=0; for(var i=0;i<N;i++){ n+=parseInt(String(i*7),10)+parseFloat((i*1.25).toString())+(i/3).toFixed(2).length+(i).toString(16).length+Number('12.5'); } return n;"
)
B["bigint_arith"] = (
    "var a=1n,n=0; for(var i=0;i<N/2;i++){ a=(a*31n+BigInt(i))%1000000007n; n+=Number(a&255n); } return n;"
)
B["bigint_big"] = (
    "var a=1n; for(var i=1n;i<300n;i++)a*=i; var n=0; for(var j=0;j<N/500;j++){ n+=(a*a/(a-1n)).toString().length; } return n;"
)
B["async_await"] = (
    "var n=0; async function f(x){ return x+1 } async function run(){ for(var i=0;i<N/2;i++){ n+=await f(i); } } return run().then(()=>n)"
)
B["promise_chain"] = (
    "var n=0; var p=Promise.resolve(0); for(var i=0;i<N/2;i++){ p=p.then(v=>{n++;return v+1}); } return p.then(()=>n)"
)
B["promise_all"] = (
    "var n=0; var ps=[];for(var j=0;j<N/100;j++){ ps.push(Promise.all([1,2,3].map(x=>Promise.resolve(x))).then(a=>{n+=a.length})); } return Promise.all(ps).then(()=>n)"
)
B["symbol_wellknown"] = (
    "var o={[Symbol.toPrimitive](h){return h=='number'?1:'s'}}; var n=0; for(var i=0;i<N;i++){ n+=+o+(''+o).length+(Symbol.for('k'+(i%10)).description.length); } return n;"
)
B["getter_setter_defprop"] = (
    "var o={_v:0,get v(){return this._v},set v(x){this._v=x}}; var n=0; for(var i=0;i<N*3;i++){ o.v=i; n+=o.v; } return n;"
)
B["switch_loops"] = (
    "var n=0; outer: for(var i=0;i<N*2;i++){ switch(i%5){case 0:n+=1;break;case 1:n+=2;break;case 2:n+=3;case 3:n+=4;break;default:n+=5} for(var j=0;j<3;j++){ if(j==2)continue outer; n++ } } return n;"
)
B["with_eval_fn"] = (
    "var n=0; for(var i=0;i<N/50;i++){ n+=eval('1+'+(i%10)) + new Function('a','return a*2')(i); } return n;"
)
B["structured_misc"] = (
    "var n=0; for(var i=0;i<N/10;i++){ n+=structuredClone({a:[1,2,{b:i}]}).a.length + encodeURIComponent('a b&c='+i).length + btoa('hello'+i).length; } return n;"
)
B["intl_format"] = (
    "var nf=new Intl.NumberFormat('en-US'); var dtf=new Intl.DateTimeFormat('en-US'); var n=0; for(var i=0;i<N/20;i++){ n+=nf.format(i*1234.5).length+dtf.format(new Date(1e12+i*1e6)).length+(i*1234.5).toLocaleString().length; } return n;"
)
B["temporal_basic"] = (
    "var n=0; if(typeof Temporal==='undefined') return 0; for(var i=0;i<N/20;i++){ n+=Temporal.PlainDate.from('2024-05-17').add({days:i%100}).toString().length; } return n;"
)
B["regex_big_alternation"] = (
    "var re=/\\b(?:function|var|let|const|return|if|else|for|while|class|new|this|typeof|void|delete|in|of)\\b/g; var s='function foo(){ var x=1; let y=2; const z=3; return typeof x in y of z }'.repeat(5); var n=0; for(var i=0;i<N/10;i++){ n+=s.match(re).length; } return n;"
)
for k, v in B.items():
    (out / f"{k}.js").write_text(f"""var N = 10000;
function bench(){{ {v} }}
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){{ console.log('EXTRA', '{k}', Date.now()-__t, v); }});
else console.log('EXTRA', '{k}', Date.now()-__t, __r);
""")
print(len(B))
