var N = 10000;
function bench(){ var n=0; for(var i=0;i<N/10;i++){ n+=structuredClone({a:[1,2,{b:i}]}).a.length + encodeURIComponent('a b&c='+i).length + btoa('hello'+i).length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'structured_misc', Date.now()-__t, v); });
else console.log('EXTRA', 'structured_misc', Date.now()-__t, __r);
