var N = 10000;
function bench(){ var n=0; for(var i=0;i<N*3;i++){ var o={a:i,b:i+1,c:{d:i}}; n+=o.a+o.c.d; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'obj_alloc_props', Date.now()-__t, v); });
else console.log('EXTRA', 'obj_alloc_props', Date.now()-__t, __r);
