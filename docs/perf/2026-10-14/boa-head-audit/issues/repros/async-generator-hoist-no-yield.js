async function* g(){return s;function s(){}}g().next().then(v=>console.log(typeof v.value),e=>console.log(e.name+":"+e.message));
