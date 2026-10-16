// Copyright (C) 2026 the JSSE authors. All rights reserved.
/*---
esid: sec-addrestrictedfunctionproperties
description: >
  The legacy Annex B `Function.prototype.arguments` accessor of a sloppy
  function that never mentions `arguments` reads the live activation: extra
  arguments beyond the parameter list, mapped parameters that were reassigned,
  objects reachable only through the call record across a collection, nested
  activations of the same function, and a cleared record after return.
features: [host-gc-required]
flags: [noStrict]
---*/

function probe(f) {
  return f["argu" + "ments"];
}

function extra(a, b) {
  return probe(extra);
}
var record = extra(1, 2, 3, 4);
assert.sameValue(record.length, 4, "length counts extra arguments");
assert.sameValue(record[2], 3, "extra argument 2");
assert.sameValue(record[3], 4, "extra argument 3");

function mutated(a, b) {
  a = "changed";
  return probe(mutated);
}
record = mutated("orig", "kept");
assert.sameValue(record[0], "changed", "mapped parameter write is visible");
assert.sameValue(record[1], "kept", "untouched parameter");

function aliasing(a) {
  var o = probe(aliasing);
  o[0] = "via-arguments";
  return a;
}
assert.sameValue(aliasing("start"), "via-arguments", "materialized record stays mapped");

function identity(a) {
  var first = probe(identity);
  var second = probe(identity);
  return first === second;
}
assert.sameValue(identity(1), true, "materialized object is cached for the activation");

function collect(a, b) {
  $262.gc();
  var held = probe(collect);
  $262.gc();
  return held;
}
record = collect({ tag: "first" }, [1, 2, 3]);
assert.sameValue(record[0].tag, "first", "object argument survives collection");
assert.sameValue(record[1].length, 3, "array argument survives collection");

function onlyInRecord(a) {
  a = null;
  $262.gc();
  return probe(onlyInRecord)[0];
}
assert.sameValue(onlyInRecord({ tag: "only" }), null, "mapped write replaces the argument");

function unmapped(a = 1, b) {
  $262.gc();
  return probe(unmapped);
}
record = unmapped(undefined, { tag: "unmapped" });
assert.sameValue(record[1].tag, "unmapped", "non-simple parameter list keeps its arguments");
assert.sameValue(record[0], undefined, "non-simple parameters are not mapped");

function recursive(n) {
  if (n === 0) {
    return [probe(recursive)[0]];
  }
  var inner = recursive(n - 1);
  inner.push(probe(recursive)[0]);
  return inner;
}
record = recursive(2);
assert.sameValue(record.join(), "0,1,2", "each activation sees its own arguments");

function done(a) {}
done(1);
assert.sameValue(probe(done), null, "no record once the activation returned");

function viaCaller(a) {
  return probe(viaCaller);
}
function outer(x, y) {
  var r = viaCaller(x + y);
  return [r[0], probe(outer)[1]];
}
record = outer(1, 2);
assert.sameValue(record[0], 3, "callee record is the callee's own arguments");
assert.sameValue(record[1], 2, "caller record is untouched by the callee");

function fewerArgs(a, b, c) {
  return probe(fewerArgs).length;
}
assert.sameValue(fewerArgs(), 0, "no arguments passed");
assert.sameValue(fewerArgs(1), 1, "fewer arguments than parameters");
assert.sameValue(fewerArgs(1, 2, 3, 4, 5, 6, 7, 8, 9, 10), 10, "ten arguments");
