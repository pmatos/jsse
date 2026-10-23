var N = 10000;
function bench(){ var n=0; for(var i=0;i<N*2;i++){ n+=`item-${i}-${i*2}:${i%7}`.length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'str_template', Date.now()-__t, v); });
else console.log('EXTRA', 'str_template', Date.now()-__t, __r);
