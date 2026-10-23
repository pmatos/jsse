var N = 10000;
function bench(){ var o={a:1,b:2,c:3,d:4,e:5,f:6}; var n=0; for(var i=0;i<N;i++){ n+=Object.keys(o).length+Object.entries(o).length+Object.values(o)[2]; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'obj_keys_entries', Date.now()-__t, v); });
else console.log('EXTRA', 'obj_keys_entries', Date.now()-__t, __r);
