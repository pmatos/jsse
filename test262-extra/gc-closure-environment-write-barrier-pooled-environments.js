/*---
description: >
  Function environments that a collection scanned and that the interpreter then
  recycles keep their referents alive when they are reused under a parent
  chain that nothing else roots, and when old closures capture them.
info: |
  The collector re-scans environments that were written since their last scan,
  found through a registry of every environment it has scanned. A recycled
  environment stays in that registry while its new parent chain has never been
  scanned: scanning the environment must scan that chain too, or a young
  object only the chain references is freed under the closure that reads it.
esid: sec-environment-records
features: [host-gc-required]
---*/

var sink;

function minor() {
  for (var i = 0; i < 30000; i++) sink = { i: i };
}

// A call that collects while its frame is live, so the frame's environment is
// scanned and registered, then recycled when the call returns.
function scanned() {
  minor();
}

function unscanned() {}

function make(i) {
  var cfg = { n: i, pad: [i, i + 1] };
  return (function () {
    var count = 0;
    return function () {
      return count++ + cfg.n;
    };
  })();
}

var fns = [];
for (var round = 0; round < 24; round++) {
  scanned();
  unscanned();
  fns.push(make(round));
  minor();
  minor();
}
minor();
minor();

for (var i = 0; i < fns.length; i++) {
  assert.sameValue(fns[i](), i, "first call of closure " + i);
}
minor();
for (var i = 0; i < fns.length; i++) {
  assert.sameValue(fns[i](), i + 1, "second call of closure " + i);
}
