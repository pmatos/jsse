var N = 10000;
function bench(){ var n=0; for(var i=0;i<N/2;i++){ var o={}; Object.defineProperty(o,'x',{value:i,enumerable:true}); Object.freeze(o); n+=o.x+(Object.isFrozen(o)?1:0); } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'obj_defineprop_freeze', Date.now()-__t, v); });
else console.log('EXTRA', 'obj_defineprop_freeze', Date.now()-__t, __r);
