var N = 10000;
function bench(){ var n=0; async function f(x){ return x+1 } async function run(){ for(var i=0;i<N/2;i++){ n+=await f(i); } } return run().then(()=>n) }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'async_await', Date.now()-__t, v); });
else console.log('EXTRA', 'async_await', Date.now()-__t, __r);
