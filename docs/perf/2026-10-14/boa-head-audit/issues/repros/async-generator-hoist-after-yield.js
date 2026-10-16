async function* f(){yield 0;return s();function s(){return 42;}}const it=f();it.next().then(()=>it.next()).then(v=>console.log(v.value),e=>console.log(e.name+":"+e.message));
