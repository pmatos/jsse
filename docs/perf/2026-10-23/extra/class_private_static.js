var N = 10000;
function bench(){ class C{#v=0; static c=0; inc(){this.#v++; C.c++; return this.#v} } var o=new C(); var n=0; for(var i=0;i<N*3;i++){ n+=o.inc(); } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'class_private_static', Date.now()-__t, v); });
else console.log('EXTRA', 'class_private_static', Date.now()-__t, __r);
