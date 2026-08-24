/*---
description: >
  Simple, compound, logical, and super property writes share OrdinarySet's
  prototype, descriptor, Proxy, receiver, and strict-rejection semantics.
info: |
  OrdinarySet and OrdinarySetWithOwnDescriptor (sec-ordinaryset and
  sec-ordinarysetwithowndescriptor) recurse through the holder's prototype
  [[Set]], reject inherited non-writable data descriptors, and call inherited
  setters with the original Reference receiver. PutValue (sec-putvalue)
  converts a false [[Set]] result to TypeError only for strict References.
esid: sec-ordinaryset
flags: [noStrict]
features: [Proxy, logical-assignment-operators]
---*/

var proxyTarget = {
  compound: 2,
  logical: 0,
};
var proxyCalls = [];
var proxyPrototype = new Proxy(proxyTarget, {
  set: function (target, key, value, receiver) {
    proxyCalls.push({ key: key, value: value, receiver: receiver });
    return true;
  },
});
var proxyReceiver = Object.create(proxyPrototype);

assert.sameValue((proxyReceiver.simple = 4), 4, "simple Proxy-chain assignment result");
assert.sameValue((proxyReceiver.compound += 3), 5, "compound Proxy-chain assignment result");
assert.sameValue((proxyReceiver.logical ||= 6), 6, "logical Proxy-chain assignment result");
assert.sameValue(proxyCalls.length, 3, "each reached write invokes the Proxy set trap once");
assert.sameValue(proxyCalls[0].key, "simple", "simple key");
assert.sameValue(proxyCalls[0].value, 4, "simple value");
assert.sameValue(proxyCalls[1].key, "compound", "compound key");
assert.sameValue(proxyCalls[1].value, 5, "compound value");
assert.sameValue(proxyCalls[2].key, "logical", "logical key");
assert.sameValue(proxyCalls[2].value, 6, "logical value");
for (var proxyIndex = 0; proxyIndex < proxyCalls.length; proxyIndex += 1) {
  assert.sameValue(proxyCalls[proxyIndex].receiver, proxyReceiver, "Proxy receiver is the LHS base");
}
assert.sameValue(
  Object.prototype.hasOwnProperty.call(proxyReceiver, "simple"),
  false,
  "a handled Proxy write does not create a receiver property"
);

var rejectingPrototype = new Proxy(
  { compound: 1, logical: 0 },
  {
    set: function () {
      return false;
    },
  }
);

var sloppyProxyReceiver = Object.create(rejectingPrototype);
assert.sameValue((sloppyProxyReceiver.simple = 2), 2, "sloppy rejected simple write returns RHS");
assert.sameValue((sloppyProxyReceiver.compound += 2), 3, "sloppy rejected compound write returns result");
assert.sameValue((sloppyProxyReceiver.logical ||= 4), 4, "sloppy rejected logical write returns RHS");
assert.sameValue(
  Object.prototype.hasOwnProperty.call(sloppyProxyReceiver, "simple"),
  false,
  "sloppy false Proxy result is silent and creates no property"
);

assert.throws(TypeError, function () {
  "use strict";
  var receiver = Object.create(rejectingPrototype);
  receiver.simple = 2;
});
assert.throws(TypeError, function () {
  "use strict";
  var receiver = Object.create(rejectingPrototype);
  receiver.compound += 2;
});
assert.throws(TypeError, function () {
  "use strict";
  var receiver = Object.create(rejectingPrototype);
  receiver.logical ||= 4;
});

var readOnlyPrototype = {};
Object.defineProperty(readOnlyPrototype, "simple", {
  value: 1,
  writable: false,
  configurable: true,
});
Object.defineProperty(readOnlyPrototype, "compound", {
  value: 1,
  writable: false,
  configurable: true,
});
Object.defineProperty(readOnlyPrototype, "logical", {
  value: 0,
  writable: false,
  configurable: true,
});

var sloppyReadOnlyReceiver = Object.create(readOnlyPrototype);
assert.sameValue((sloppyReadOnlyReceiver.simple = 2), 2, "sloppy inherited read-only simple result");
assert.sameValue((sloppyReadOnlyReceiver.compound += 2), 3, "sloppy inherited read-only compound result");
assert.sameValue((sloppyReadOnlyReceiver.logical ||= 4), 4, "sloppy inherited read-only logical result");
assert.sameValue(sloppyReadOnlyReceiver.simple, 1, "simple inherited value is unchanged");
assert.sameValue(sloppyReadOnlyReceiver.compound, 1, "compound inherited value is unchanged");
assert.sameValue(sloppyReadOnlyReceiver.logical, 0, "logical inherited value is unchanged");

assert.throws(TypeError, function () {
  "use strict";
  var receiver = Object.create(readOnlyPrototype);
  receiver.simple = 2;
});
assert.throws(TypeError, function () {
  "use strict";
  var receiver = Object.create(readOnlyPrototype);
  receiver.compound += 2;
});
assert.throws(TypeError, function () {
  "use strict";
  var receiver = Object.create(readOnlyPrototype);
  receiver.logical ||= 4;
});

var setterCalls = [];
var setterPrototype = {};
Object.defineProperty(setterPrototype, "simple", {
  configurable: true,
  set: function (value) {
    setterCalls.push({ key: "simple", value: value, receiver: this });
  },
});
Object.defineProperty(setterPrototype, "compound", {
  configurable: true,
  get: function () {
    return 2;
  },
  set: function (value) {
    setterCalls.push({ key: "compound", value: value, receiver: this });
  },
});
Object.defineProperty(setterPrototype, "logical", {
  configurable: true,
  get: function () {
    return 0;
  },
  set: function (value) {
    setterCalls.push({ key: "logical", value: value, receiver: this });
  },
});
var setterReceiver = Object.create(setterPrototype);

assert.sameValue((setterReceiver.simple = 4), 4, "inherited simple setter result");
assert.sameValue((setterReceiver.compound += 3), 5, "inherited compound setter result");
assert.sameValue((setterReceiver.logical ||= 6), 6, "inherited logical setter result");
assert.sameValue(setterCalls.length, 3, "each inherited setter is called once");
for (var setterIndex = 0; setterIndex < setterCalls.length; setterIndex += 1) {
  assert.sameValue(setterCalls[setterIndex].receiver, setterReceiver, "setter receiver is the LHS base");
}
assert.sameValue(setterCalls[0].value, 4, "simple setter value");
assert.sameValue(setterCalls[1].value, 5, "compound setter value");
assert.sameValue(setterCalls[2].value, 6, "logical setter value");

var loopSetterReceiver;
var loopSetterValue;
var loopSetterPrototype = {};
Object.defineProperty(loopSetterPrototype, "value", {
  configurable: true,
  set: function (value) {
    loopSetterReceiver = this;
    loopSetterValue = value;
  },
});
var loopReceiver = Object.create(loopSetterPrototype);
for (loopReceiver.value of [8]) {
  // Assignment to the member target happens before the loop body.
}
assert.sameValue(loopSetterReceiver, loopReceiver, "for-of member target preserves its receiver");
assert.sameValue(loopSetterValue, 8, "for-of member target invokes the inherited setter");

var loopProxyCalls = 0;
var loopProxyReceiver;
var loopProxyPrototype = new Proxy({}, {
  set: function (target, key, value, receiver) {
    loopProxyCalls += 1;
    loopProxyReceiver = receiver;
    assert.sameValue(key, "value", "for-of Proxy key");
    assert.sameValue(value, 9, "for-of Proxy value");
    return true;
  },
});
var loopProxyBase = Object.create(loopProxyPrototype);
for (loopProxyBase.value of [9]) {
  // Assignment to the member target happens before the loop body.
}
assert.sameValue(loopProxyCalls, 1, "for-of member target invokes a prototype Proxy once");
assert.sameValue(loopProxyReceiver, loopProxyBase, "for-of Proxy receives the LHS base");

var superCalls = [];
class SuperBase {}
Object.defineProperty(SuperBase.prototype, "simple", {
  configurable: true,
  set: function (value) {
    superCalls.push({ key: "simple", value: value, receiver: this });
  },
});
Object.defineProperty(SuperBase.prototype, "compound", {
  configurable: true,
  get: function () {
    return 2;
  },
  set: function (value) {
    superCalls.push({ key: "compound", value: value, receiver: this });
  },
});
Object.defineProperty(SuperBase.prototype, "logical", {
  configurable: true,
  get: function () {
    return 0;
  },
  set: function (value) {
    superCalls.push({ key: "logical", value: value, receiver: this });
  },
});
Object.defineProperty(SuperBase.prototype, "fixed", {
  configurable: true,
  value: 1,
  writable: false,
});

class SuperDerived extends SuperBase {
  writeSimple(value) {
    return (super.simple = value);
  }
  writeCompound(value) {
    return (super.compound += value);
  }
  writeLogical(value) {
    return (super.logical ||= value);
  }
  writeFixed(value) {
    return (super.fixed = value);
  }
}

var superReceiver = new SuperDerived();
assert.sameValue(superReceiver.writeSimple(4), 4, "super simple result");
assert.sameValue(superReceiver.writeCompound(3), 5, "super compound result");
assert.sameValue(superReceiver.writeLogical(6), 6, "super logical result");
assert.sameValue(superCalls.length, 3, "each super setter is called once");
for (var superIndex = 0; superIndex < superCalls.length; superIndex += 1) {
  assert.sameValue(superCalls[superIndex].receiver, superReceiver, "super setter receives actual this");
}
assert.throws(TypeError, function () {
  superReceiver.writeFixed(2);
});
assert.sameValue(superReceiver.fixed, 1, "rejected super write leaves inherited data unchanged");

var superProxyCalls = 0;
var superProxyReceiver;
var superProxyPrototype = new Proxy({}, {
  set: function (target, key, value, receiver) {
    superProxyCalls += 1;
    superProxyReceiver = receiver;
    assert.sameValue(key, "value", "super Proxy key");
    assert.sameValue(value, 10, "super Proxy value");
    return true;
  },
});
class SuperProxyHolder {
  write(value) {
    return (super.value = value);
  }
}
Object.setPrototypeOf(SuperProxyHolder.prototype, superProxyPrototype);
var superProxyBase = new SuperProxyHolder();
assert.sameValue(superProxyBase.write(10), 10, "super Proxy assignment result");
assert.sameValue(superProxyCalls, 1, "super write invokes a prototype Proxy once");
assert.sameValue(superProxyReceiver, superProxyBase, "super Proxy receives actual this");
