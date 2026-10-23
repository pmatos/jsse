var N = 10000;
function bench(){ var n=0; var ps=[];for(var j=0;j<N/100;j++){ ps.push(Promise.all([1,2,3].map(x=>Promise.resolve(x))).then(a=>{n+=a.length})); } return Promise.all(ps).then(()=>n) }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'promise_all', Date.now()-__t, v); });
else console.log('EXTRA', 'promise_all', Date.now()-__t, __r);
