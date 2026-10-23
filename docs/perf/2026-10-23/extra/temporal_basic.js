var N = 10000;
function bench(){ var n=0; if(typeof Temporal==='undefined') return 0; for(var i=0;i<N/20;i++){ n+=Temporal.PlainDate.from('2024-05-17').add({days:i%100}).toString().length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'temporal_basic', Date.now()-__t, v); });
else console.log('EXTRA', 'temporal_basic', Date.now()-__t, __r);
