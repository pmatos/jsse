var N = 10000;
function bench(){ var n=0; for(var i=0;i<N*2;i++){ var {a,b:{c},d=5}={a:i,b:{c:2}}; n+=a+c+d; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'destructure_obj', Date.now()-__t, v); });
else console.log('EXTRA', 'destructure_obj', Date.now()-__t, __r);
