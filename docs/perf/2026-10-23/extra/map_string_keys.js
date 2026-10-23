var N = 10000;
function bench(){ var m=new Map(); var n=0; for(var i=0;i<N*2;i++){ var k='key'+(i%500); m.set(k,(m.get(k)|0)+1); } for(var [k,v] of m)n+=v; return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'map_string_keys', Date.now()-__t, v); });
else console.log('EXTRA', 'map_string_keys', Date.now()-__t, __r);
