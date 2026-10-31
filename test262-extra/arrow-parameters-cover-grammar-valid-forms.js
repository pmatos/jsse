// Copyright (C) 2026 Paulo Matos. All rights reserved.
// This code is governed by the BSD license found in the LICENSE file.
/*---
esid: sec-arrow-function-definitions
description: forms next to the rejected cover-grammar sources stay valid
info: |
  ArrowParameters may end in a trailing comma, take binding patterns with
  defaults and computed keys that contain parenthesized or member expressions,
  and a catch parameter that is a plain BindingIdentifier may be redeclared by
  a var (B.3.4). The rest property of an assignment pattern may be any simple
  assignment target, and the rest element of an array pattern any pattern.
features: [async-functions, object-rest, destructuring-binding]
---*/

function accepts(source) {
  new Function(source);
}

accepts("(a, ) => 1");
accepts("(a, b, ) => 1");
accepts("async (a, ) => 1");
accepts("async (a, b = (1)) => 1");
accepts("({a: b, c = 1, ...d}, [e, ...[f]], g = (1)) => 1");
accepts("({[(k)]: v}) => v");
accepts("({[o.p]: v}) => v");
accepts("(a = o.p, b = (1, 2)) => 1");
accepts("(a = (b) => c) => a");
accepts("(a = (b = (c) => 1) => 2) => a");
accepts("(a = /x)/g, b = `t${a}${`n${1}`}`) => 1");
accepts("async (a = /x)/g, b = `t${a}`) => 1");
accepts("(a = {b: (c) => `${c}`, d: /e/}) => a");
accepts("([a = (1)]) => a");
accepts("({a = (1)}) => a");
accepts("async (a = (1)) => a");
accepts("x = (a, b)");
accepts("x = ((a), (b))");
accepts("async(a.b, (c), ...d)");
accepts("async(a, )");

accepts("try {} catch (e) { var e; }");
accepts("try {} catch (e) { for (var e of []); }");
accepts("try {} catch (e) { for (var e in {}); }");
accepts("try {} catch ([e]) { function f() { var e; } }");
accepts("try {} catch ({e}) { (function () { var e; }); }");

accepts("({...a.b} = {})");
accepts("({...a} = {})");
accepts("({...(a)} = {})");
accepts("({...(a.b)} = {})");
accepts("[...{a}] = []");
accepts("[...[a]] = []");
accepts("for (var [...{a}] of []);");
accepts("var [...{a}] = []");
accepts("var {...a} = {}");

var tail = ((x, y) => x + y)(1, 2);
assert.sameValue(tail, 3);

var o = {};
var sink = ({...o.p} = {q: 1});
assert.sameValue(o.p.q, 1);
sink = [...[o.r]] = [2];
assert.sameValue(o.r, 2);
