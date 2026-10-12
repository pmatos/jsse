const N = 300000; let acc = 0; const keep = [];
for (let i = 0; i < N; i++) {
  const p = new Promise((resolve, reject) => { resolve(i); });
  if ((i & 1023) === 0) keep.push(p);
  const q = Promise.resolve(i);
  if ((i & 1023) === 1) keep.push(q);
  acc = (acc + i) | 0;
}
Promise.all(keep).then(vs => { let s = 0; for (const v of vs) s = (s + v) | 0; console.log("acc=" + acc + " s=" + s); });
