var N = 10000;
function bench(){ var n=0; for(var i=0;i<N/4;i++){ n+=Date.parse('2024-05-17T10:20:30Z')%1000+new Date(1e12+i).toString().length+new Date(1e12+i).toUTCString().length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'date_parse_fmt', Date.now()-__t, v); });
else console.log('EXTRA', 'date_parse_fmt', Date.now()-__t, __r);
