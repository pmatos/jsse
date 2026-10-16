async function f(){function s(){return 42;}await 0;return s();}
f().then(v=>console.log(v),e=>console.log(e.name+":"+e.message));
