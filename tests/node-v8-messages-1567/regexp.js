// V8 text of for await, yield*, calls and patterns in iteration position, parameter
// patterns, string keys in callees, Intl receivers and RegExp syntax errors (#1567, #1568,
// #1557). Run verbatim on Node and on `jsse --node`.
const srcs = [['(','u'],['(',''],['a**','u'],['\\','u'],['(?<=a)+','u'],['[b-a]','v'],['(?<n>a)(?<n>b)','u'],['\\k<x>','u'],['{1}','u'],['a{2,1}','giu'],['(?<=a)+','dgimsuy'],['\\u{110000}','u'],['(?<=a)+','']];
for (const [s,f] of srcs) { try { new RegExp(s,f); console.log('ok'); } catch(e) { console.log(e.message); } }
try { eval('/(?<=a)+/u'); } catch(e) { console.log(e.message); }
try { eval('/(/gu'); } catch(e) { console.log(e.message); }
