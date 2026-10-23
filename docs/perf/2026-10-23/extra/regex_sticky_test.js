var N = 10000;
function bench(){ var re=/[a-z]+\d*/y; var s='abc12 def3 ghi'; var n=0; for(var i=0;i<N*2;i++){ re.lastIndex=0; while(re.test(s)){n++; re.lastIndex++} } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'regex_sticky_test', Date.now()-__t, v); });
else console.log('EXTRA', 'regex_sticky_test', Date.now()-__t, __r);
