/*---
description: >
  Values pushed while an array or object literal is under construction stay
  alive when a later element or value forces a garbage collection, including
  when the literal is built in several operand chunks.
info: |
  jsse issue #959: the VM keeps the pending elements of a literal on its
  operand stack (rooted in `gc_bytecode_roots`) until `NewArray` / `NewObject`
  consumes them, and the partially built array of a chunked literal is itself
  an operand.
esid: sec-array-initializer
includes: [compareArray.js]
flags: [noStrict]
---*/

function make(tag) { return { tag: tag, list: [tag, tag + 1] }; }
function churn() {
  var junk = [];
  for (var n = 0; n < 100; n++) junk.push({ n: n, pad: [n, n + 1] });
  return junk.length;
}
function collect() { $262.gc(); churn(); $262.gc(); churn(); }

function arrays() {
  return [make(0), [make(1)], function () { return "fn"; }, (collect(), make(2)), { k: make(3) }, make(4)];
}
var a = arrays();
assert.sameValue(a[0].tag, 0);
assert.sameValue(a[1][0].tag, 1);
assert.sameValue(a[2](), "fn");
assert.sameValue(a[3].tag, 2);
assert.sameValue(a[4].k.tag, 3);
assert.sameValue(a[5].list[1], 5);

function objects() {
  return {
    first: make(10),
    second: [make(11), make(12)],
    third: (collect(), make(13)),
    fn: function () { return "fn"; },
    method() { return "m"; },
    get g() { return make(14); },
    last: make(15),
  };
}
var o = objects();
assert.sameValue(o.first.tag, 10);
assert.sameValue(o.second[1].tag, 12);
assert.sameValue(o.third.tag, 13);
assert.sameValue(o.fn(), "fn");
assert.sameValue(o.method(), "m");
assert.sameValue(o.g.tag, 14);
assert.sameValue(o.last.list[0], 15);

function chunked() {
  var src = "(function (make, collect) { return [";
  for (var i = 0; i < 300; i++) src += "make(" + i + "), ";
  src += "(collect(), make(300)), ";
  for (var j = 301; j < 600; j++) src += "make(" + j + "), ";
  src += "make(600)]; })";
  var arr = eval(src)(make, collect);
  var osrc = "(function (make, collect) { return {";
  for (var k = 0; k < 300; k++) osrc += "p" + k + ": make(" + k + "), ";
  osrc += "mid: (collect(), make(300)), ";
  for (var m = 301; m < 600; m++) osrc += "p" + m + ": make(" + m + "), ";
  osrc += "end: make(600) }; })";
  var obj = eval(osrc)(make, collect);
  return [arr, obj];
}
var c = chunked();
assert.sameValue(c[0].length, 601);
assert.sameValue(c[0][0].tag, 0);
assert.sameValue(c[0][299].list[1], 300);
assert.sameValue(c[0][300].tag, 300);
assert.sameValue(c[0][600].tag, 600);
assert.sameValue(c[1].p0.tag, 0);
assert.sameValue(c[1].p299.tag, 299);
assert.sameValue(c[1].mid.tag, 300);
assert.sameValue(c[1].p599.tag, 599);
assert.sameValue(c[1].end.tag, 600);

function closuresKeepEnvironments() {
  var fs = [];
  for (var i = 0; i < 4; i++) {
    var held = make(i * 100);
    fs.push(function () { return held.tag; }, [function () { return held.list[1]; }]);
    collect();
  }
  return fs;
}
var fs = closuresKeepEnvironments();
assert.sameValue(fs[0](), 300, "var-captured binding is shared and holds the last value");
assert.sameValue(fs[1][0](), 301);
