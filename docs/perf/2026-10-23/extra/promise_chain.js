var N = 10000;
function bench(){ var n=0; var p=Promise.resolve(0); for(var i=0;i<N/2;i++){ p=p.then(v=>{n++;return v+1}); } return p.then(()=>n) }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'promise_chain', Date.now()-__t, v); });
else console.log('EXTRA', 'promise_chain', Date.now()-__t, __r);
