var N = 10000;
function bench(){ var o={}; var n=0; for(var i=0;i<N;i++){ o['k'+(i%200)]=i; n+=o['k'+((i*7)%200)]|0; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'obj_dynamic_keys', Date.now()-__t, v); });
else console.log('EXTRA', 'obj_dynamic_keys', Date.now()-__t, __r);
