var N = 10000;
function bench(){ var s='abcdefghijklmnopqrstuvwxyz'.repeat(20); var n=0; for(var i=0;i<N*4;i++){ n+=s.indexOf('xyz',i%100)+s.slice(i%50,i%50+10).length+s.charCodeAt(i%500); } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'str_indexof_slice', Date.now()-__t, v); });
else console.log('EXTRA', 'str_indexof_slice', Date.now()-__t, __r);
