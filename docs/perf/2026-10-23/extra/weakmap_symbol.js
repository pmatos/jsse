var N = 10000;
function bench(){ var w=new WeakMap(); var keys=[];for(var i=0;i<100;i++)keys.push({}); var n=0; for(var i=0;i<N*2;i++){ var k=keys[i%100]; w.set(k,i); n+=w.get(k); } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'weakmap_symbol', Date.now()-__t, v); });
else console.log('EXTRA', 'weakmap_symbol', Date.now()-__t, __r);
