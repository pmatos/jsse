// Isolates promise + resolving-function creation: `new Promise(executor)`
// resolved synchronously, and Promise.resolve(x), with no reactions and a
// shallow queue. Measures allocation of the promise object and the
// resolve/reject function objects, executor call overhead and pin_native_root.
// Models: dummy_1/dummy_2 `Promise.resolve(undefined)` in doxbee-promise and
// promisified `new Promise((res) => ...)` code.
// Measured (buildbox3, median of 5, jsse HEAD 2b621c5 v0.11.0 vs boa 0.22.0, 2026-10-10, loadavg 10.4):
//   jsse 5.94 s, boa 1.82 s, ratio 3.3x; jsse --no-bytecode 5.64 s.
const N = 300000;
let acc = 0;
const keep = [];
for (let i = 0; i < N; i++) {
  const p = new Promise((resolve, reject) => { resolve(i); });
  if ((i & 1023) === 0) keep.push(p);
  const q = Promise.resolve(i);
  if ((i & 1023) === 1) keep.push(q);
  acc = (acc + i) | 0;
}
Promise.all(keep).then(vs => {
  let s = 0;
  for (const v of vs) s = (s + v) | 0;
  console.log("new-promise acc=" + acc + " kept=" + vs.length + " s=" + s);
});
