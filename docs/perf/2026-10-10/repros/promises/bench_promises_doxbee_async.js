// Realistic reduction of JetStream doxbee-async (simple/doxbee-async.js):
// 5k (JetStream: 10k) concurrent async functions, each doing 7 sequential `await`s on async
// functions that return immediately, wrapped in try/catch.
// Models: doxbee-async (jsse 6.14x slower than boa in the JetStream run).
// Measured (buildbox3, median of 5, jsse HEAD 2b621c5 v0.11.0 vs boa 0.22.0, 2026-10-10, loadavg 4.4):
//   jsse 2.73 s, boa 0.62 s, ratio 4.4x; jsse --no-bytecode 2.68 s.
"use strict";
async function dummy_1() { }
async function dummy_2(a) { }
function Queryish() {}
Queryish.prototype.execWithin = dummy_2;
Queryish.prototype.get = dummy_1;
function queryish() { return new Queryish(); }
const cqQueryish = queryish();
function Blob() {}
Blob.prototype.put = dummy_2;
function Transaction() {}
Transaction.prototype.commit = dummy_1;
Transaction.prototype.rollback = dummy_1;
const userAccount = { id: 1 };
const createHash = v => 1;
async function createQuery(x, y) { return cqQueryish; }

async function doxbee(stream, idOrPath) {
  const blob = new Blob();
  const tx = new Transaction();
  try {
    const blobId = await blob.put(stream);
    const file = await queryish().get();
    const previousId = file ? file.version : null;
    const version = { userAccountId: userAccount.id, blobId: blobId,
                      creatorId: userAccount.id, previousId: previousId };
    version.id = createHash(version);
    await queryish().execWithin(tx);
    let fileId;
    if (!file) {
      const splitPath = idOrPath.split("/");
      const fileName = splitPath[splitPath.length - 1];
      fileId = 7;
      const q = await createQuery(idOrPath, { id: fileId, userAccountId: userAccount.id,
                                              name: fileName, version: version.id });
      await q.execWithin(tx);
    } else {
      fileId = file.id;
    }
    await queryish().execWithin(tx);
    await queryish().execWithin(tx);
    await tx.commit();
  } catch (err) {
    await tx.rollback();
    throw err;
  }
}

const N = 5000, ROUNDS = 1;
let done = 0;
async function main() {
  for (let r = 0; r < ROUNDS; r++) {
    const ps = new Array(N);
    for (let i = 0; i < N; i++) ps[i] = doxbee(i, "foo");
    await Promise.all(ps);
    done += N;
  }
  console.log("doxbee-async done=" + done);
}
main();
