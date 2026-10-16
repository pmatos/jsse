async function f(){return s();function s(){return 42;}}
f().then(v=>console.log(v),e=>console.log(e.name+":"+e.message));
