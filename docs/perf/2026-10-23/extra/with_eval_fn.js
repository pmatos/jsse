var N = 10000;
function bench(){ var n=0; for(var i=0;i<N/50;i++){ n+=eval('1+'+(i%10)) + new Function('a','return a*2')(i); } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'with_eval_fn', Date.now()-__t, v); });
else console.log('EXTRA', 'with_eval_fn', Date.now()-__t, __r);
