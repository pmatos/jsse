var N = 10000;
function bench(){ var re=/(?<y>\d{4})-(?<m>\d{2})-(?<d>\d{2})/; var n=0; for(var i=0;i<N;i++){ var m=re.exec('on 2024-05-17 ok'); n+=m.groups.y.length+m.index; } return n; }
var __t = Date.now();
var __r = bench();
if (__r && typeof __r.then === 'function') __r.then(function(v){ console.log('EXTRA', 'regex_exec_groups', Date.now()-__t, v); });
else console.log('EXTRA', 'regex_exec_groups', Date.now()-__t, __r);
