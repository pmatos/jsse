var N = 10000;
function bench(){ var n=0; for(var j=0;j<N/200;j++){ var a=[];var x=j+1;for(var i=0;i<500;i++){x=(x*1103515245+12345)&0x7fffffff;a.push(x);} a.sort(); n+=a[0]; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'arr_sort_default', Date.now()-__t, v); });
else console.log('EXTRA', 'arr_sort_default', Date.now()-__t, __r);
