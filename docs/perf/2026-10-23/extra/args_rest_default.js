var N = 10000;
function bench(){ function f(a,b=2,...r){return a+b+r.length} function g(){return arguments.length+arguments[0]} var n=0; for(var i=0;i<N*2;i++){ n+=f(i)+f(i,1,2,3)+g(i,2); } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'args_rest_default', Date.now()-__t, v); });
else console.log('EXTRA', 'args_rest_default', Date.now()-__t, __r);
