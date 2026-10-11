// Isolates async-function CALL overhead: many concurrent async functions
// (no inner await) started synchronously, results gathered with Promise.all.
// Every call allocates a promise, a pair of resolving functions, and async
// function state; the body returns immediately.
// Models: dummy_1/dummy_2 (`async function dummy() {}`) in doxbee-async,
// Directory.addFile/rm/addDirectory in async-fs.
// Measured (buildbox3, median of 5, jsse HEAD 2b621c5 v0.11.0 vs boa 0.22.0, 2026-10-10, loadavg 4.3):
//   jsse 6.14 s, boa 1.72 s, ratio 3.6x; jsse --no-bytecode 6.09 s.
async function f(a, b) { return a + b; }
async function main() {
  let s = 0;
  const B = 2000;
  for (let r = 0; r < 100; r++) {
    const ps = new Array(B);
    for (let i = 0; i < B; i++) ps[i] = f(i, r);
    const vs = await Promise.all(ps);
    for (let i = 0; i < B; i++) s = (s + vs[i]) | 0;
  }
  console.log("async-call s=" + s);
}
main();
