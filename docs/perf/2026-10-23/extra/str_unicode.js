var N = 10000;
function bench(){ var s='héllo wörld ünïcödé 日本語 テキスト'; var n=0; for(var i=0;i<N;i++){ n+=s.normalize('NFD').length+s.codePointAt(i%10)+[...s].length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'str_unicode', Date.now()-__t, v); });
else console.log('EXTRA', 'str_unicode', Date.now()-__t, __r);
