// A call used as an assignment target, a for-in/of head or an update operand: V8 parses it
// in sloppy and strict code and throws a ReferenceError when the statement runs (#1523).
// Run verbatim on Node and on `jsse --node`.
var log = [];
function f() { log.push("f"); return {}; }
function show(name, text) { console.log([name, text, log.join()].filter(Boolean).join(" ")); }
function t(name, fn) {
  log = [];
  try { fn(); show(name, "no error"); }
  catch (e) { show(name, e.name + ": " + e.message); }
}
var iterable = {
  [Symbol.iterator]() {
    return { next() { return { value: 1, done: false }; }, return() { log.push("return"); return {}; } };
  },
};
t("assign", () => { f() = 1; });
t("assign strict", function () { "use strict"; f() = 1; });
t("compound", () => { f() += 1; });
t("compound strict", function () { "use strict"; f() *= 1; });
t("paren strict", function () { "use strict"; (f()) = 1; });
t("postfix", () => { f()++; });
t("postfix strict", function () { "use strict"; f()++; });
t("prefix", () => { ++f(); });
t("prefix strict", function () { "use strict"; --f(); });
t("for-of", () => { for (f() of [1]) ; });
t("for-of strict", function () { "use strict"; for (f() of [1]) ; });
t("for-of empty strict", function () { "use strict"; for (f() of []) ; });
t("for-of closes the iterator", function () { "use strict"; for (f() of iterable) ; });
t("for-in", () => { for (f() in { a: 1 }) ; });
t("for-in strict", function () { "use strict"; for (f() in { a: 1 }) ; });
t("for-in empty strict", function () { "use strict"; for (f() in {}) ; });
t("operand first", function () { "use strict"; function g(x) { return {}; } g(log.push("arg")) = 1; });
function* gen() {
  try { for (f() of [3, 4]) yield 0; show("generator", "no error"); }
  catch (e) { show("generator", e.name + ": " + e.message); }
}
log = [];
for (var _ of gen()) ;
async function sloppy() {
  try { for (f() of [1]) await 0; show("async for-of", "no error"); }
  catch (e) { show("async for-of", e.name + ": " + e.message); }
  log = [];
  try { for (f() in { a: 1 }) await 0; show("async for-in", "no error"); }
  catch (e) { show("async for-in", e.name + ": " + e.message); }
  log = [];
  try { for await (f() of [1]) ; show("for await", "no error"); }
  catch (e) { show("for await", e.name + ": " + e.message); }
}
async function strict() {
  "use strict";
  log = [];
  try { for (f() of [1]) await 0; show("strict async for-of", "no error"); }
  catch (e) { show("strict async for-of", e.name + ": " + e.message); }
  log = [];
  try { for await (f() of [1]) await 0; show("strict for await", "no error"); }
  catch (e) { show("strict for await", e.name + ": " + e.message); }
}
sloppy().then(strict);
