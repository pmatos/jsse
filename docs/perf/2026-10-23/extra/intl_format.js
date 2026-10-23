var N = 10000;
function bench(){ var nf=new Intl.NumberFormat('en-US'); var dtf=new Intl.DateTimeFormat('en-US'); var n=0; for(var i=0;i<N/20;i++){ n+=nf.format(i*1234.5).length+dtf.format(new Date(1e12+i*1e6)).length+(i*1234.5).toLocaleString().length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'intl_format', Date.now()-__t, v); });
else console.log('EXTRA', 'intl_format', Date.now()-__t, __r);
