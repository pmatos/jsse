var N = 10000;
function bench(){ var n=0; for(var i=0;i<N;i++){ n+=String(i).padStart(8,'0').toUpperCase().trim().length+'Hello'.toLowerCase().length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'str_pad_case', Date.now()-__t, v); });
else console.log('EXTRA', 'str_pad_case', Date.now()-__t, __r);
