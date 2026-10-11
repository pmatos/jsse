// Realistic reduction of JetStream doxbee-promise (simple/doxbee-promise.js):
// 5k (JetStream: 10k; change N below) concurrent 6-stage `.then()` chains over Promise.resolve() fakes,
// all started synchronously, then Promise.all-d.
// Models: doxbee-promise (jsse 6.18x slower than boa in the JetStream run).
// Measured (buildbox3, median of 5, jsse HEAD 2b621c5 v0.11.0 vs boa 0.22.0, 2026-10-10, loadavg 4.1):
//   jsse 4.58 s, boa 1.12 s, ratio 4.1x; jsse --no-bytecode 4.53 s.
//   At the JetStream size (N = 10000): jsse 13.22 s, boa 2.29 s, ratio 5.8x (single run, load ~5).
"use strict";
function dummy_1() { return Promise.resolve(undefined); }
function dummy_2(a) { return Promise.resolve(undefined); }
function Queryish() {}
Queryish.prototype.all = dummy_1;
Queryish.prototype.exec = dummy_1;
Queryish.prototype.execWithin = dummy_2;
Queryish.prototype.get = dummy_1;
function queryish() { return new Queryish(); }
const cqQueryish = queryish();
function Blob() {}
Blob.prototype.put = dummy_2;
function Transaction() {}
Transaction.prototype.commit = dummy_1;
Transaction.prototype.rollback = dummy_1;
const fakes = {
  userAccount: { id: 1 },
  newBlob() { return new Blob(); },
  begin() { return new Transaction(); },
  byUuidOrPath: queryish,
  createQuery(x, y) { return Promise.resolve(cqQueryish); },
  insert: queryish,
  whereUpdate: queryish,
  createHash(v) { return 1; },
};

function doxbee(stream, idOrPath) {
  const blob = fakes.newBlob();
  const tx = fakes.begin();
  let version, blobId, fileId, file;
  return blob.put(stream)
    .then(blobIdV => {
      blobId = blobIdV;
      return fakes.byUuidOrPath(idOrPath).get();
    })
    .then(fileV => {
      file = fileV;
      const previousId = file ? file.version : null;
      version = { userAccountId: fakes.userAccount.id, blobId: blobId,
                  creatorId: fakes.userAccount.id, previousId: previousId };
      version.id = fakes.createHash(version);
      return fakes.insert(version).execWithin(tx);
    })
    .then(_ => {
      if (!file) {
        const splitPath = idOrPath.lastIndexOf("/") + 1;
        const fileName = idOrPath.substring(splitPath);
        const newId = 7;
        return fakes.createQuery(idOrPath, { id: newId, userAccountId: fakes.userAccount.id,
                                             name: fileName, version: version.id })
          .then(q => q.execWithin(tx))
          .then(_ => newId);
      }
      return file.id;
    })
    .then(fileIdV => {
      fileId = fileIdV;
      return fakes.insert({ fileId: fileId, versionId: version.id }).execWithin(tx);
    })
    .then(_ => fakes.whereUpdate({ id: fileId }, { version: version.id }).execWithin(tx))
    .then(_ => tx.commit())
    .catch(err => tx.rollback().then(_ => Promise.reject(err)));
}

const N = 5000, ROUNDS = 1;
let done = 0;
function round(r) {
  const ps = new Array(N);
  for (let i = 0; i < N; i++) ps[i] = doxbee(i, "foo");
  return Promise.all(ps).then(() => { done += N; });
}
let p = Promise.resolve();
for (let r = 0; r < ROUNDS; r++) p = p.then(() => round(r));
p.then(() => console.log("doxbee-promise done=" + done));
