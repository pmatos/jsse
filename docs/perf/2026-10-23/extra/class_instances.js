var N = 10000;
function bench(){ class P{constructor(x,y){this.x=x;this.y=y} get sum(){return this.x+this.y} add(o){return new P(this.x+o.x,this.y+o.y)}} var n=0,p=new P(0,0),d=new P(1,2); for(var i=0;i<N*2;i++){ p=p.add(d); n+=p.sum; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'class_instances', Date.now()-__t, v); });
else console.log('EXTRA', 'class_instances', Date.now()-__t, __r);
