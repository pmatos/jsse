// Isolates `await` cost in a single async function (shallow queue): awaits a
// plain value, an already-resolved native promise, and a promise returned from
// a nested async-function call, in a hot loop.
// Models: await sequences in doxbee-async and async-fs.
// Measured (buildbox3, median of 5, jsse HEAD 2b621c5 v0.11.0 vs boa 0.22.0, 2026-10-10, loadavg 4.6):
//   jsse 1.92 s, boa 0.97 s, ratio 2.0x; jsse --no-bytecode 1.92 s.
async function leaf(x) { return x + 1; }
async function main() {
  const N = 60000;
  let s = 0;
  for (let i = 0; i < N; i++) {
    s = (s + await i) | 0;                    // await non-promise
    s = (s + await Promise.resolve(i)) | 0;   // await resolved native promise
    s = (s + await leaf(i)) | 0;              // await async call (call + resolve + await)
  }
  console.log("await-loop s=" + s);
}
main();
