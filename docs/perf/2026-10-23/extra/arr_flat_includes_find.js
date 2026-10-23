var N = 10000;
function bench(){ var a=[[1,2],[3,[4,5]],[6]]; var n=0; for(var i=0;i<N;i++){ n+=a.flat(2).length+(a.includes(3)?1:0)+[1,2,3,4,5].find(x=>x>3)+[1,2,3].findIndex(x=>x==2); } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'arr_flat_includes_find', Date.now()-__t, v); });
else console.log('EXTRA', 'arr_flat_includes_find', Date.now()-__t, __r);
