var N = 10000;
function bench(){ var m=new Map(), s=new Set(); var n=0; for(var i=0;i<N*2;i++){ m.set(i%1000,i); s.add(i%777); n+=m.get((i*3)%1000)|0; if(s.has(i%500))n++; } return n+m.size+s.size; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'map_set_ops', Date.now()-__t, v); });
else console.log('EXTRA', 'map_set_ops', Date.now()-__t, __r);
