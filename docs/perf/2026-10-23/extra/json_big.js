var N = 10000;
function bench(){ var a=[];for(var i=0;i<2000;i++)a.push({id:i,name:'n'+i,tags:['a','b'],v:i*1.5}); var s=JSON.stringify(a); var n=0; for(var j=0;j<N/500;j++){ n+=JSON.parse(s).length+JSON.stringify(a).length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'json_big', Date.now()-__t, v); });
else console.log('EXTRA', 'json_big', Date.now()-__t, __r);
