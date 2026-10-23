var N = 10000;
function bench(){ var s='a1b22c333d4444'.repeat(10); var n=0; for(var i=0;i<N/10;i++){ for(var m of s.matchAll(/\d+/g))n+=m[0].length; n+=s.replace(/(\d)(\d)/g,'$2$1').length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'regex_matchall_replace', Date.now()-__t, v); });
else console.log('EXTRA', 'regex_matchall_replace', Date.now()-__t, __r);
