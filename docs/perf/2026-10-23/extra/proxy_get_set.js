var N = 10000;
function bench(){ var p=new Proxy({a:1},{get(t,k,r){return Reflect.get(t,k,r)},set(t,k,v,r){return Reflect.set(t,k,v,r)}}); var n=0; for(var i=0;i<N*2;i++){ p.a=i; n+=p.a; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'proxy_get_set', Date.now()-__t, v); });
else console.log('EXTRA', 'proxy_get_set', Date.now()-__t, __r);
