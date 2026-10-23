var N = 10000;
function bench(){ var f=new Float64Array(1000), u=new Uint8Array(1000); var n=0; for(var j=0;j<N/20;j++){ for(var i=0;i<1000;i++){ f[i]=i*0.5; u[i]=i; } for(var i=0;i<1000;i++){ n+=f[i]+u[i]; } } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'arr_typed', Date.now()-__t, v); });
else console.log('EXTRA', 'arr_typed', Date.now()-__t, __r);
