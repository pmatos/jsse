var N = 10000;
function bench(){ function f(a,b){return this.k+a+b} var o={k:1}; var g=f.bind(o,2); var n=0; for(var i=0;i<N*2;i++){ n+=f.call(o,i,1)+f.apply(o,[i,2])+g(3); } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'call_apply_bind', Date.now()-__t, v); });
else console.log('EXTRA', 'call_apply_bind', Date.now()-__t, __r);
