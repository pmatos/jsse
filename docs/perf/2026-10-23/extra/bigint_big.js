var N = 10000;
function bench(){ var a=1n; for(var i=1n;i<300n;i++)a*=i; var n=0; for(var j=0;j<N/500;j++){ n+=(a*a/(a-1n)).toString().length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'bigint_big', Date.now()-__t, v); });
else console.log('EXTRA', 'bigint_big', Date.now()-__t, __r);
