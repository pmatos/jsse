var N = 10000;
function bench(){ var it={[Symbol.iterator](){var i=0;return{next(){return i<100?{value:i++,done:false}:{value:undefined,done:true}}}}}; var n=0; for(var j=0;j<N/50;j++){ for(var v of it)n+=v; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'iter_protocol', Date.now()-__t, v); });
else console.log('EXTRA', 'iter_protocol', Date.now()-__t, __r);
