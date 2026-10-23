var N = 10000;
function bench(){ var n=0; for(var i=0;i<N/10;i++){ var e=new Error('boom'); n+=e.stack.length>0?1:0; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'error_stack', Date.now()-__t, v); });
else console.log('EXTRA', 'error_stack', Date.now()-__t, __r);
