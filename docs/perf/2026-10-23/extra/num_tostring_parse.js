var N = 10000;
function bench(){ var n=0; for(var i=0;i<N;i++){ n+=parseInt(String(i*7),10)+parseFloat((i*1.25).toString())+(i/3).toFixed(2).length+(i).toString(16).length+Number('12.5'); } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'num_tostring_parse', Date.now()-__t, v); });
else console.log('EXTRA', 'num_tostring_parse', Date.now()-__t, __r);
