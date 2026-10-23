var N = 10000;
function bench(){ var n=0; for(var j=0;j<N/20;j++){ var o={}; for(var i=0;i<20;i++)o['p'+i]=i; for(var k in o){n+=o[k]} delete o.p3; delete o.p7; n+=Object.keys(o).length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'obj_for_in_delete', Date.now()-__t, v); });
else console.log('EXTRA', 'obj_for_in_delete', Date.now()-__t, __r);
