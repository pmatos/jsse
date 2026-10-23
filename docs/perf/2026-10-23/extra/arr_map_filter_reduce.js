var N = 10000;
function bench(){ var a=[];for(var i=0;i<1000;i++)a.push(i); var n=0; for(var j=0;j<N/50;j++){ n+=a.map(x=>x*2).filter(x=>x%3==0).reduce((p,c)=>p+c,0); } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'arr_map_filter_reduce', Date.now()-__t, v); });
else console.log('EXTRA', 'arr_map_filter_reduce', Date.now()-__t, __r);
