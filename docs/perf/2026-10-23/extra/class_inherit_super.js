var N = 10000;
function bench(){ class A{constructor(x){this.x=x} m(){return this.x}} class B extends A{constructor(x){super(x);this.y=1} m(){return super.m()+this.y}} var n=0; for(var i=0;i<N*2;i++){ n+=new B(i).m(); } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'class_inherit_super', Date.now()-__t, v); });
else console.log('EXTRA', 'class_inherit_super', Date.now()-__t, __r);
