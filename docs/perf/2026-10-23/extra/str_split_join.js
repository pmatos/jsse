var N = 10000;
function bench(){ var s='alpha,beta,gamma,delta,epsilon,zeta,eta,theta'; var n=0; for(var i=0;i<N;i++){ n+=s.split(',').join('-').length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'str_split_join', Date.now()-__t, v); });
else console.log('EXTRA', 'str_split_join', Date.now()-__t, __r);
