var N = 10000;
function bench(){ var a=[]; var n=0; for(var i=0;i<N*3;i++){ a.push(i); if(i%3==2){ n+=a.pop()+a.shift(); } } return n+a.length; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'arr_push_pop_shift', Date.now()-__t, v); });
else console.log('EXTRA', 'arr_push_pop_shift', Date.now()-__t, __r);
