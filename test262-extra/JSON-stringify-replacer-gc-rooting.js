/*---
description: >
  JSON.stringify keeps the holder and the values returned by toJSON and the
  replacer function reachable while they are serialized, across forced
  collections.
esid: sec-json.stringify
info: |
  SerializeJSONProperty replaces value with the result of toJSON and then of
  the ReplacerFunction before serializing it. Those results, and the holder
  they were read from, are owned only by the algorithm while a nested call
  (which may collect) is in progress.

  Regression test for issue #1204: json_stringify_internal did not root the
  holder or the post-toJSON/replacer value, so a collection triggered inside a
  nested getter reclaimed it and the members serialized as null.
features: [host-gc-required]
---*/

var fromReplacer = JSON.stringify([{ a: 1 }], function (k, v) {
  if (k === "") {
    return [{ get c() { $262.gc(); return 1; } }, 5, { x: 1 }];
  }
  return v;
});

assert.sameValue(fromReplacer, '[{"c":1},5,{"x":1}]', "replacer result");

var fromToJSON = JSON.stringify({
  toJSON: function () {
    return [{ get c() { $262.gc(); return 1; } }, 5, { x: 1 }];
  }
});

assert.sameValue(fromToJSON, '[{"c":1},5,{"x":1}]', "toJSON result");

var nested = JSON.stringify({ a: 1, b: 2 }, function (k, v) {
  $262.gc();
  return typeof v === "number" && k !== "n" ? { n: v } : v;
});

assert.sameValue(nested, '{"a":{"n":1},"b":{"n":2}}', "replacer wrapping members");

var source = { a: [1, 2] };
var withGap = JSON.stringify(source, function (k, v) {
  $262.gc();
  return typeof v === "number" && this === source.a ? [v] : v;
}, 1);

assert.sameValue(withGap, '{\n "a": [\n  [\n   1\n  ],\n  [\n   2\n  ]\n ]\n}', "replacer with gap");

var listItem = new String("a");
listItem.toString = function () { $262.gc(); return "a"; };
var propertyList = JSON.stringify({ a: { b: 1 }, c: 2 }, [listItem, new String("b")]);

assert.sameValue(propertyList, '{"a":{"b":1}}', "property list items");
