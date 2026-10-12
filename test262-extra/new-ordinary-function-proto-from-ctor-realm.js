// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-getprototypefromconstructor
description: >
  `new F()` on an ordinary function from another realm whose `prototype` is
  not an Object creates an object whose [[Prototype]] is that realm's
  %Object.prototype%, the same as Reflect.construct(F, []).
info: |
  10.2.2 [[Construct]] ( argumentsList, newTarget )
    5. If kind is base, then
      a. Let thisArgument be ? OrdinaryCreateFromConstructor(newTarget,
         "%Object.prototype%").

  10.1.14 GetPrototypeFromConstructor ( constructor, intrinsicDefaultProto )
    2. Let proto be ? Get(constructor, "prototype").
    3. If proto is not an Object, then
      a. Let realm be ? GetFunctionRealm(constructor).
      b. Set proto to realm's intrinsic object named intrinsicDefaultProto.
features: [cross-realm, Reflect]
---*/

var other = $262.createRealm().global;
var OtherObjectPrototype = other.Object.prototype;

[null, undefined, 1, "s", Symbol()].forEach(function(value) {
  var F = new other.Function();
  F.prototype = value;

  var viaNew = new F();
  assert.sameValue(
    Object.getPrototypeOf(viaNew),
    OtherObjectPrototype,
    "new F() with prototype " + String(value)
  );

  var viaReflect = Reflect.construct(F, []);
  assert.sameValue(
    Object.getPrototypeOf(viaReflect),
    OtherObjectPrototype,
    "Reflect.construct(F, []) with prototype " + String(value)
  );
});

var G = new other.Function("this.x = 1;");
G.prototype = null;
var g = new G();
assert.sameValue(Object.getPrototypeOf(g), OtherObjectPrototype, "body runs with that object as this");
assert.sameValue(g.x, 1, "body ran");
