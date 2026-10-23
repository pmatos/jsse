var N = 10000;
function bench(){ var n=0; for(var i=0;i<N*3;i++){ var f=(function(a){return function(b){return a+b}})(i); n+=f(1); } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'closure_create', Date.now()-__t, v); });
else console.log('EXTRA', 'closure_create', Date.now()-__t, __r);
