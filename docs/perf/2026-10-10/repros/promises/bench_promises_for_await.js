// Isolates `for await` over an async generator whose body yields plain values
// (per-item cost: next() promise + request queue + await + iterator result
// object), plus an `async` method that returns immediately per item. Shallow
// queue (one consumer at a time).
// Models: Directory.ls()/forEachFile() `for await` loops in JetStream async-fs.
// Measured (buildbox3, median of 5, jsse HEAD 2b621c5 v0.11.0 vs boa 0.22.0, 2026-10-10, loadavg 10.6):
//   jsse 3.78 s, boa 1.72 s, ratio 2.2x; jsse --no-bytecode 3.78 s.
async function* gen(n) { for (let i = 0; i < n; i++) yield i; }
async function leaf(x) { return x & 3; }
async function main() {
  let s = 0;
  for (let r = 0; r < 20; r++) {
    for await (const v of gen(5000)) s = (s + v + await leaf(v)) | 0;
  }
  console.log("for-await s=" + s);
}
main();
