var N = 10000;
function bench(){ var o={a:[1,2,3,{b:'x'}],c:'hello',d:{e:null,f:true,g:1.5}}; var n=0; for(var i=0;i<N/2;i++){ n+=JSON.parse(JSON.stringify(o)).a.length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'json_roundtrip', Date.now()-__t, v); });
else console.log('EXTRA', 'json_roundtrip', Date.now()-__t, __r);
