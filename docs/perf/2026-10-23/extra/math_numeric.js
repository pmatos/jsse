var N = 10000;
function bench(){ var n=0; for(var i=1;i<N*4;i++){ n+=Math.sqrt(i)+Math.sin(i)+Math.floor(i/3)+Math.max(i,5)+Math.abs(-i)+Math.pow(i,0.5)+Math.imul(i,3); } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'math_numeric', Date.now()-__t, v); });
else console.log('EXTRA', 'math_numeric', Date.now()-__t, __r);
