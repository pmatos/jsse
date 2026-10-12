// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.

/*---
esid: sec-initializeinstanceelements
description: >
  The object a base class constructor is building must stay reachable while
  its field initializers run: until the constructor body runs, nothing else
  holds it, and an initializer that triggers a garbage collection must not
  free it, whichever path constructs the class.
info: |
  [[Construct]] ( argumentsList, newTarget ) (10.2.2)
    ...
    5. If kind is base, then
      a. Let thisArgument be ? OrdinaryCreateFromConstructor(newTarget, "%Object.prototype%").
    ...
    8. If kind is base, then
      a. Perform OrdinaryCallBindThis(F, calleeContext, thisArgument).
      b. Let initializeResult be Completion(InitializeInstanceElements(thisArgument, F)).
features: [class-fields-public, Reflect.construct, host-gc-required]
---*/

function collect() {
  for (var i = 0; i < 1000; i++) {
    [{}, {}, {}, {}];
  }
  $262.gc();
}

class Base {
  before = { tag: "before" };
  during = collect();
  after = { tag: "after" };
}

function check(obj, Ctor, label) {
  assert(obj instanceof Ctor, label + ": the constructed object keeps its prototype");
  assert.sameValue(obj.before.tag, "before", label + ": the field set before the collection survived");
  assert.sameValue(obj.after.tag, "after", label + ": the field set after the collection is on the same object");
}

check(new Base(), Base, "new");
check(Reflect.construct(Base, []), Base, "Reflect.construct");

class DefaultDerived extends Base {}
check(new DefaultDerived(), DefaultDerived, "super() from a default constructor");

class ExplicitDerived extends Base {
  constructor() {
    super();
  }
}
check(new ExplicitDerived(), ExplicitDerived, "super() from an explicit constructor");

// The object must also survive a collection before the initializers run (a
// Proxy newTarget's `prototype` getter) and after them (parameter defaults run
// before the constructor body).
var newTarget = new Proxy(function() {}, {
  get: function(target, key) {
    if (key === "prototype") {
      collect();
      return Base.prototype;
    }
    return target[key];
  }
});
check(Reflect.construct(Base, [], newTarget), Base, "Reflect.construct with a Proxy newTarget");

class WithDefaults {
  before = { tag: "before" };
  after = { tag: "after" };
  constructor(unused = collect()) {}
}
check(new WithDefaults(), WithDefaults, "a parameter default");
check(Reflect.construct(WithDefaults, []), WithDefaults, "Reflect.construct with a parameter default");
