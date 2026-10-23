var N = 10000;
function bench(){ var a=1n,n=0; for(var i=0;i<N/2;i++){ a=(a*31n+BigInt(i))%1000000007n; n+=Number(a&255n); } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'bigint_arith', Date.now()-__t, v); });
else console.log('EXTRA', 'bigint_arith', Date.now()-__t, __r);
