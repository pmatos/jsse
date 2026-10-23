var N = 10000;
function bench(){ var o={a:1,b:2,[Symbol('s')]:3}; var n=0; for(var i=0;i<N;i++){ n+=Reflect.ownKeys(o).length+Object.getOwnPropertyNames(o).length+(Reflect.has(o,'a')?1:0); } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'reflect_ownkeys', Date.now()-__t, v); });
else console.log('EXTRA', 'reflect_ownkeys', Date.now()-__t, __r);
