/*---
description: >
  Writes to a global function declaration's name through the global object,
  Object.defineProperty and Reflect.set are seen by bare-name reads, and bare
  writes are seen on the global object.
info: |
  A global function declaration is represented only as an own property of the
  global object (sec-createglobalfunctionbinding), so there is no second
  binding that could disagree with it.
esid: sec-createglobalfunctionbinding
flags: [noStrict]
---*/

function fd1346() { return 1; }
function readFd() { return fd1346; }

assert.sameValue(typeof readFd(), "function");
globalThis.fd1346 = "via global object";
assert.sameValue(readFd(), "via global object");
assert.sameValue(fd1346, "via global object");

fd1346 = "via bare write";
assert.sameValue(globalThis.fd1346, "via bare write");

Reflect.set(globalThis, "fd1346", "via Reflect");
assert.sameValue(readFd(), "via Reflect");

Object.defineProperty(globalThis, "fd1346", { value: "via define" });
assert.sameValue(readFd(), "via define");

var desc = Object.getOwnPropertyDescriptor(globalThis, "fd1346");
assert.sameValue(desc.writable, true);
assert.sameValue(desc.enumerable, true);
assert.sameValue(desc.configurable, false);
