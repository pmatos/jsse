// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-prepareforordinarycall
description: >
  Function and block environments are recycled between activations. A recycled
  environment must behave exactly like a fresh one: no stale `this`, arrow,
  strictness, parameter-expression, `arguments`-immutability or function-scope
  state from the activation that used it before; and the operands of a call
  stay alive across collections whether the callee returns or throws.
features: [host-gc-required]
flags: [noStrict]
---*/

function withDefaults(a = 1) {
  eval("var shadow = 'body'");
  return typeof shadow;
}

function simple(a) {
  eval("var shadow = 'function'");
  return shadow;
}

function strictArguments() {
  "use strict";
  return arguments.length;
}

function arrowHost() {
  return (() => typeof this)();
}

for (var i = 0; i < 4; i++) {
  assert.sameValue(withDefaults(), "string", "default parameters, round " + i);
  assert.sameValue(simple(1), "function", "eval var lands in function scope, round " + i);
  assert.sameValue(strictArguments(1, 2), 2, "strict arguments, round " + i);
  assert.sameValue(arrowHost.call(7), "object", "sloppy arrow this, round " + i);
}

function blockThenFunction(n) {
  var total = 0;
  for (let i = 0; i < n; i++) {
    let doubled = i * 2;
    total += doubled;
  }
  eval("var fromEval = total");
  return fromEval;
}
assert.sameValue(blockThenFunction(4), 12, "var from eval after block scopes were recycled");

function varScopeProbe() {
  { let blockLocal = 1; }
  eval("var leaked = 2");
  return typeof leaked;
}
assert.sameValue(varScopeProbe(), "number", "recycled block scope does not capture eval vars");
assert.sameValue(typeof leaked, "undefined", "eval var does not leak out of the function");

function sloppyThis() {
  return this;
}
function strictThis() {
  "use strict";
  return this;
}
function thisInBlock() {
  var seen = [];
  for (let i = 0; i < 2; i++) {
    seen.push(this === undefined);
    { seen.push(this.tag); }
  }
  return seen.join();
}
for (var round = 0; round < 3; round++) {
  assert.sameValue(sloppyThis.call(undefined), this, "nullish this becomes the global object");
  assert.sameValue(typeof sloppyThis.call(1), "object", "primitive this is boxed");
  assert.sameValue(strictThis.call(undefined), undefined, "strict this stays undefined");
  assert.sameValue(strictThis.call(1), 1, "strict primitive this");
  assert.sameValue(thisInBlock.call({ tag: "t" }), "false,t,false,t", "this inside nested blocks");
}

class Base {
  constructor() {
    this.base = true;
  }
}
class Derived extends Base {
  constructor(early) {
    if (early) {
      try {
        this.x = 1;
      } catch (e) {
        super();
        this.caught = e.constructor === ReferenceError;
        return;
      }
    }
    super();
    this.caught = false;
  }
  arrowThis() {
    return (() => this)() === this;
  }
}
for (var round = 0; round < 3; round++) {
  assert.sameValue(new Derived(true).caught, true, "this is in TDZ before super()");
  assert.sameValue(new Derived(false).caught, false, "no TDZ error without early access");
  assert.sameValue(new Derived(false).arrowThis(), true, "arrow this inside a derived method");
}

function Thrower(arg) {
  $262.gc();
  throw arg;
}
function callAcrossGc(target, a, b, c) {
  $262.gc();
  return [target.tag, a.tag, b.tag, c.tag].join();
}
for (var round = 0; round < 3; round++) {
  var caught;
  try {
    Thrower({ tag: "thrown" });
  } catch (e) {
    caught = e;
  }
  assert.sameValue(caught.tag, "thrown", "thrown object survives the unwind");
  assert.sameValue(
    callAcrossGc({ tag: "t" }, { tag: "a" }, { tag: "b" }, { tag: "c" }),
    "t,a,b,c",
    "operands built just for the call survive a collection in the callee"
  );
  assert.sameValue(
    callAcrossGc({ tag: callAcrossGc({ tag: 1 }, { tag: 2 }, { tag: 3 }, { tag: 4 }) }, { tag: "a" }, { tag: "b" }, { tag: "c" }),
    "1,2,3,4,a,b,c",
    "nested call operands stay rooted"
  );
}
