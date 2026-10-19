/*---
description: >
  Resources a `using` declaration queues on a block environment, and the
  bindings next to them, stay alive across minor collections while an old
  closure captures that environment, and are disposed in reverse order.
info: |
  The pending-resource stack of an environment is state the write barrier has
  to see, like its bindings: a resource pushed after the environment was last
  scanned must be re-scanned at the next minor collection.
esid: sec-disposeresources
features: [explicit-resource-management, host-gc-required]
flags: [noStrict]
---*/

var sink;

function minor() {
  for (var i = 0; i < 30000; i++) sink = { i: i };
}

function promote() {
  $262.gc();
  minor();
  minor();
}

var disposed = [];

function resource(label) {
  var payload = { label: label, pad: [label.length] };
  return {
    payload: payload,
    [Symbol.dispose]: function () {
      disposed.push(payload.label + ":" + payload.pad[0]);
    },
  };
}

var peek;
{
  var cell = { value: "before" };
  peek = function () { return cell.value; };
  promote();

  using first = resource("first");
  minor();
  minor();
  cell.value = { label: "stored after first" };
  using second = resource("second");
  minor();
  minor();
  minor();

  assert.sameValue(first.payload.label, "first", "first resource payload");
  assert.sameValue(second.payload.label, "second", "second resource payload");
  assert.sameValue(peek().label, "stored after first", "old closure reads the stored object");
}

assert.sameValue(disposed.join(), "second:6,first:5", "disposal order and payloads");
assert.sameValue(peek().label, "stored after first", "after the block");
