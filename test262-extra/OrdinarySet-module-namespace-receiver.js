/*---
description: >
  OrdinarySetWithOwnDescriptor consults the Receiver's [[GetOwnProperty]] and
  [[DefineOwnProperty]], so a module namespace exotic object used as the
  receiver of a super-property write rejects the write instead of gaining an
  own property.
info: |
  OrdinarySetWithOwnDescriptor ( O, P, V, Receiver, ownDesc )

  3. If IsDataDescriptor(ownDesc) is true, then
    b. If Receiver is not an Object, return false.
    c. Let existingDescriptor be ? Receiver.[[GetOwnProperty]](P).
    d. If existingDescriptor is not undefined, then
      iv. Return ? Receiver.[[DefineOwnProperty]](P, valueDesc).
    e. Else, return ? CreateDataProperty(Receiver, P, V).

  Module Namespace Exotic Objects [[DefineOwnProperty]] ( P, Desc )
    2. Let current be ! O.[[GetOwnProperty]](P).
    3. If current is undefined, return false.
    8. If Desc has a [[Value]] field, return
       SameValue(Desc.[[Value]], current.[[Value]]).

  A key that is not an export is rejected by step 3; an exported key reaches
  step 8 and is rejected because the written value differs from the binding's
  current value.

  PutValue ( V, W ) converts the false result into a TypeError because module
  code is always strict.
esid: sec-ordinarysetwithowndescriptor
flags: [module]
---*/

import * as ns from "./OrdinarySet-module-namespace-receiver_FIXTURE.mjs";

// The home object's prototype is the [[Set]] holder; `this` is the receiver.
// A null-prototype holder makes OrdinarySet reach the synthetic writable
// ownDesc, which is the branch that must consult the receiver.
var holder = {
  writeNewKey(value) {
    super.notAnExport = value;
  },
  writeExportedKey(value) {
    super.initialized = value;
  },
};
Object.setPrototypeOf(holder, Object.create(null));

assert.throws(
  TypeError,
  function () {
    holder.writeNewKey.call(ns, 1);
  },
  "a new key on a module namespace receiver rejects"
);

assert.throws(
  TypeError,
  function () {
    holder.writeExportedKey.call(ns, 1);
  },
  "an exported key on a module namespace receiver rejects"
);

assert.sameValue(
  Object.prototype.hasOwnProperty.call(ns, "notAnExport"),
  false,
  "the rejected write created no own property on the namespace"
);
assert.sameValue(ns.initialized, "initialized", "the exported binding is unchanged");
