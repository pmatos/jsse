show(typeof hoisted, typeof notHoistedFn);
var hoisted = 1;
function hoistedFn() { return "fn"; }
show(hoistedFn(), typeof hoisted);
try { tdz; let tdz = 1; } catch (e) { show(e.name); }
{
  function blockFn() { return "block"; }
}
show(typeof blockFn);
const counter = (() => { let c = 0; return { inc: () => ++c, get: () => c }; })();
counter.inc(); counter.inc();
show(counter.get());
var self = this;
show(typeof globalThis, self === globalThis || self === undefined || typeof self);
function sloppy() { return this === globalThis; }
function strict() { "use strict"; return this; }
show(sloppy(), strict(), eval("var evalVar = 3; evalVar"), typeof evalVar);
with ({ w: "with-scope" }) { show(w); }
const fn = new Function("a", "b", "return a + b");
show(fn(2, 3), fn.length, Function.prototype.toString.call(fn).includes("return a + b"));
