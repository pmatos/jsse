var N = 10000;
function bench(){ var a=['pear','apple','fig','banana','cherry','date','elderberry','grape']; var n=0; for(var i=0;i<N/10;i++){ n+=a.slice().sort(function(x,y){return x.localeCompare(y)})[0].length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'str_localecompare', Date.now()-__t, v); });
else console.log('EXTRA', 'str_localecompare', Date.now()-__t, __r);
