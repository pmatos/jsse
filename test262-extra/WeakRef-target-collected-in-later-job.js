/*---
description: >
  A WeakRef target with no other reference is reclaimed by a collection in a
  later job, whether the WeakRef was created by the script or by a promise job.
esid: sec-weak-ref-objects
info: |
  The spec permits but does not require collection; JSSE collects, and this
  guards the regression where WeakRef held its target strongly (#1317).
  Within the job that created the WeakRef the target stays alive (KeepDuringJob).
flags: [async]
features: [WeakRef, host-gc-required]
---*/

var refs = {};
function make(name) {
  refs[name] = new WeakRef({});
}

make("script");
Promise.resolve()
  .then(function () {
    make("job");
    $262.gc();
    assert.notSameValue(refs.job.deref(), undefined, "kept during its own job");
  })
  .then(function () {
    $262.gc();
    assert.sameValue(refs.script.deref(), undefined, "created by the script");
    assert.sameValue(refs.job.deref(), undefined, "created by an earlier promise job");
  })
  .then($DONE, $DONE);
