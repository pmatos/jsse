var N = 10000;
function bench(){ var o={[Symbol.toPrimitive](h){return h=='number'?1:'s'}}; var n=0; for(var i=0;i<N;i++){ n+=+o+(''+o).length+(Symbol.for('k'+(i%10)).description.length); } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'symbol_wellknown', Date.now()-__t, v); });
else console.log('EXTRA', 'symbol_wellknown', Date.now()-__t, __r);
