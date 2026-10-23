var N = 10000;
function bench(){ var re=/\b(?:function|var|let|const|return|if|else|for|while|class|new|this|typeof|void|delete|in|of)\b/g; var s='function foo(){ var x=1; let y=2; const z=3; return typeof x in y of z }'.repeat(5); var n=0; for(var i=0;i<N/10;i++){ n+=s.match(re).length; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'regex_big_alternation', Date.now()-__t, v); });
else console.log('EXTRA', 'regex_big_alternation', Date.now()-__t, __r);
