var N = 10000;
function bench(){ var o={_v:0,get v(){return this._v},set v(x){this._v=x}}; var n=0; for(var i=0;i<N*3;i++){ o.v=i; n+=o.v; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'getter_setter_defprop', Date.now()-__t, v); });
else console.log('EXTRA', 'getter_setter_defprop', Date.now()-__t, __r);
