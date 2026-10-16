async function f() {
  return s;
  function s() {}
}
f().then(v => console.log(typeof v), e => console.log(e.name + ":" + e.message));
