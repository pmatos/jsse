var N = 10000;
function bench(){ var n=0; for(var j=0;j<N/10;j++){ var t=new Int32Array(256).map((_,i)=>i*3); t.sort((a,b)=>b-a); n+=t.subarray(10,20).reduce((a,b)=>a+b,0)+t.indexOf(30); } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'arr_typed_methods', Date.now()-__t, v); });
else console.log('EXTRA', 'arr_typed_methods', Date.now()-__t, __r);
