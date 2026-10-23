var N = 10000;
function bench(){ var n=0; for(var i=0;i<N/2;i++){ try{ if(i%2) throw new Error('x'+i); n++; }catch(e){ n+=e.message.length; } finally{ n++ } } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'try_catch_throw', Date.now()-__t, v); });
else console.log('EXTRA', 'try_catch_throw', Date.now()-__t, __r);
