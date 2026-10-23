var N = 10000;
function bench(){ var o={a:1,b:2,c:3}; var n=0; for(var i=0;i<N;i++){ var p={...o,d:i}; var q=Object.assign({},p,{e:1}); n+=q.d+q.a; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'obj_assign_spread', Date.now()-__t, v); });
else console.log('EXTRA', 'obj_assign_spread', Date.now()-__t, __r);
