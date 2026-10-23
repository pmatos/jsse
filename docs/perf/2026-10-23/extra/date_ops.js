var N = 10000;
function bench(){ var n=0; for(var i=0;i<N/2;i++){ var d=new Date(2024,0,1+i%365,i%24); n+=d.getDay()+d.getMonth()+d.toISOString().length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'date_ops', Date.now()-__t, v); });
else console.log('EXTRA', 'date_ops', Date.now()-__t, __r);
