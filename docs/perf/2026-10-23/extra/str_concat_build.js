var N = 10000;
function bench(){ var n=0; for(var j=0;j<N/50;j++){ var s=''; for(var i=0;i<200;i++){ s+='ab'+i; } n+=s.length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'str_concat_build', Date.now()-__t, v); });
else console.log('EXTRA', 'str_concat_build', Date.now()-__t, __r);
