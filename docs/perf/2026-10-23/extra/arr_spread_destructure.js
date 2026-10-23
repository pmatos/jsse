var N = 10000;
function bench(){ var n=0; for(var i=0;i<N;i++){ var a=[i,i+1,i+2]; var b=[...a,...a]; var [x,y,...r]=b; n+=x+y+r.length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'arr_spread_destructure', Date.now()-__t, v); });
else console.log('EXTRA', 'arr_spread_destructure', Date.now()-__t, __r);
