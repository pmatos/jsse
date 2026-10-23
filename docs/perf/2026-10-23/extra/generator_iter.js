var N = 10000;
function bench(){ function* g(k){for(var i=0;i<k;i++)yield i} var n=0; for(var j=0;j<N/50;j++){ for(var v of g(100))n+=v; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'generator_iter', Date.now()-__t, v); });
else console.log('EXTRA', 'generator_iter', Date.now()-__t, __r);
