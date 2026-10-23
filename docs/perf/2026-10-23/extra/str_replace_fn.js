var N = 10000;
function bench(){ var s='the quick brown fox jumps over the lazy dog'; var n=0; for(var i=0;i<N;i++){ n+=s.replace(/o/g,function(m){return '0'}).length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'str_replace_fn', Date.now()-__t, v); });
else console.log('EXTRA', 'str_replace_fn', Date.now()-__t, __r);
