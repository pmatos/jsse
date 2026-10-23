var N = 10000;
function bench(){ var n=0; outer: for(var i=0;i<N*2;i++){ switch(i%5){case 0:n+=1;break;case 1:n+=2;break;case 2:n+=3;case 3:n+=4;break;default:n+=5} for(var j=0;j<3;j++){ if(j==2)continue outer; n++ } } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'switch_loops', Date.now()-__t, v); });
else console.log('EXTRA', 'switch_loops', Date.now()-__t, __r);
