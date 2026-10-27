// `vm` fixture matrix (issue #1238). Prints one line per scenario; the same
// file runs on Node and on `jsse --node` and the output must be identical
// (scripts/run-node-vm-oracle.sh). Everything that is known to differ is left
// out on purpose and listed in docs/adr/2026-10-26-2300-node-host-vm.md.
"use strict";
const vm = require("node:vm");

const out = [];
const log = (label, value) => out.push(`${label}: ${value}`);
const t = (label, f) => {
  try {
    log(label, f());
  } catch (e) {
    log(label, `THROW ${e && e.name} ${e && e.code}`);
  }
};
const json = (v) => JSON.stringify(v);

// --- surface --------------------------------------------------------------
t("exports", () => Object.keys(vm).join());
t("constants", () => json(Object.keys(vm.constants)) + typeof vm.constants.DONT_CONTEXTIFY);
t("constants frozen", () => Object.isFrozen(vm.constants) + String(Object.getPrototypeOf(vm.constants)));
t("alias", () => vm === require("vm"));
t("Script.length", () => `${vm.Script.length} ${vm.Script.name} ${vm.compileFunction.length} ${vm.runInContext.length}`);
t("Script.prototype", () => Object.getOwnPropertyNames(vm.Script.prototype).join());

// --- runInThisContext -----------------------------------------------------
t("this: value", () => vm.runInThisContext("1 + 1"));
t("this: no locals", () => vm.runInThisContext("typeof require + typeof module + typeof __filename"));
t("this: is global", () => vm.runInThisContext("this === globalThis") + " " + (vm.runInThisContext("globalThis") === globalThis));
t("this: var", () => {
  vm.runInThisContext("var vmFixtureVar = 7");
  const d = Object.getOwnPropertyDescriptor(globalThis, "vmFixtureVar");
  return json([d.value, d.writable, d.enumerable, d.configurable]);
});
t("this: let persists", () => {
  vm.runInThisContext("let vmFixtureLet = 2");
  return `${vm.runInThisContext("vmFixtureLet")} ${typeof vmFixtureLet} ${"vmFixtureLet" in globalThis}`;
});
t("this: let redeclare", () => vm.runInThisContext("let vmFixtureLet = 3"));
t("this: function", () => {
  vm.runInThisContext("function vmFixtureFn() { return 5 }");
  return vmFixtureFn();
});
t("this: sloppy this", () => vm.runInThisContext("(function () { return this })()") === globalThis);
t("this: strict this", () => String(vm.runInThisContext("'use strict'; (function () { return this })()")));
t("this: strict does not stick", () => vm.runInThisContext("(function () { return this })()") === globalThis);
t("this: completion", () => json([vm.runInThisContext(""), vm.runInThisContext("var q = 1"), vm.runInThisContext("if (true) { 3 }"), vm.runInThisContext("({ a: 1 })")]));
t("this: same realm", () => vm.runInThisContext("[]") instanceof Array);
t("this: caller of top-level function", () => vm.runInThisContext("function callerProbe() { return callerProbe.caller } callerProbe()"));
t("this: throw", () => vm.runInThisContext("throw 5"));
t("this: throw object", () => { try { vm.runInThisContext("throw new RangeError('r')"); } catch (e) { return `${e instanceof RangeError} ${e.message}`; } });
t("this: string options", () => vm.runInThisContext("new Error('x').stack.split('\\n')[1].trim()", "named.js"));

// --- contexts -------------------------------------------------------------
const ctx = vm.createContext({ a: 1, console });
t("isContext", () => json([vm.isContext(ctx), vm.isContext({}), vm.isContext(globalThis), vm.isContext(Object.create(ctx))]));
t("createContext identity", () => vm.createContext(ctx) === ctx);
t("createContext default", () => typeof vm.createContext() + " " + vm.isContext(vm.createContext()));
t("createContext bad", () => vm.createContext(5));
t("isContext bad", () => vm.isContext(5));
t("runInContext bad", () => vm.runInContext("1", {}));
t("ctx: read", () => vm.runInContext("a + 1", ctx));
t("ctx: write back", () => { vm.runInContext("a = 10; fresh = 3", ctx); return json([ctx.a, ctx.fresh]); });
t("ctx: host edits visible", () => { ctx.b = 2; ctx.a = 1; return vm.runInContext("a + b", ctx); });
t("ctx: host edits seen by live functions", () => { const f = vm.runInContext("(function () { return a + b })", ctx); ctx.b = 40; return f(); });
t("ctx: var", () => { vm.runInContext("var declared = 4; function decl() { return 6 }", ctx); return json([ctx.declared, typeof ctx.decl, ctx.decl(), ctx.decl === vm.runInContext("decl", ctx)]); });
t("ctx: var descriptor", () => json(Object.getOwnPropertyDescriptor(ctx, "declared")));
t("ctx: bare var", () => { vm.runInContext("var onlyDeclared", ctx); return "onlyDeclared" in ctx; });
t("ctx: let/const", () => { vm.runInContext("let L = 3; const C = 4; class K {}", ctx); return json([vm.runInContext("L + C + typeof K", ctx), "L" in ctx, "K" in ctx]); });
t("ctx: let redeclare", () => vm.runInContext("let L = 4", ctx));
t("ctx: const assign", () => vm.runInContext("C = 5", ctx));
t("ctx: this and globalThis", () => json([vm.runInContext("this", ctx) === ctx, vm.runInContext("globalThis", ctx) === ctx, vm.runInContext("this === globalThis", ctx)]));
t("ctx: not main realm", () => json([vm.runInContext("Array", ctx) === Array, vm.runInContext("[]", ctx) instanceof Array, vm.runInContext("Object.getPrototypeOf([]) === Array.prototype", ctx)]));
t("ctx: realms differ", () => vm.runInContext("Array", ctx) === vm.runInContext("Array", vm.createContext()));
t("ctx: errors are the context's", () => { try { vm.runInContext("null.x", ctx); } catch (e) { return `${e instanceof TypeError} ${e.constructor === vm.runInContext("TypeError", ctx)} ${e.name}`; } });
t("ctx: no node globals", () => vm.runInNewContext("[typeof require, typeof process, typeof Buffer, typeof setTimeout, typeof structuredClone, typeof atob, typeof $262, typeof print, typeof URL, typeof queueMicrotask].join()"));
t("ctx: has console", () => vm.runInNewContext("typeof console + typeof console.log + typeof console.log('hidden')"));
t("ctx: builtins", () => vm.runInNewContext("[typeof JSON, typeof Math, typeof Promise, typeof Symbol, typeof Proxy, typeof Reflect, typeof Intl, typeof Atomics, typeof eval, typeof escape, typeof globalThis].join()"));
t("ctx: overwrite builtin", () => { const c = vm.createContext({}); vm.runInContext("Array = 5", c); return json([c.Array, vm.runInContext("typeof Array", c)]); });
t("ctx: delete", () => { const c = vm.createContext({ gone: 1 }); vm.runInContext("delete gone", c); return json(["gone" in c, vm.runInContext("typeof gone", c)]); });
t("ctx: host delete read-only", () => { const c = vm.createContext({}); Object.defineProperty(c, "ro", { value: 1, enumerable: true, configurable: true }); vm.runInContext("ro", c); delete c.ro; return vm.runInContext("typeof ro", c); });
t("ctx: host delete", () => { const c = vm.createContext({ x: 1 }); vm.runInContext("x", c); delete c.x; return vm.runInContext("typeof x", c); });
t("ctx: restore builtin", () => { const c = vm.createContext({ JSON: 1 }); const a = vm.runInContext("JSON", c); delete c.JSON; return `${a} ${vm.runInContext("typeof JSON.parse", c)}`; });
t("ctx: accessor", () => {
  const c = vm.createContext({ get g() { return 7; }, set g(v) { this._s = v; } });
  return json([vm.runInContext("g", c), vm.runInContext("g = 9; g", c), c._s]);
});
t("ctx: getter this", () => { const c = vm.createContext({ get me() { return this === c; } }); return vm.runInContext("me", c); });
t("ctx: read-only", () => { const c = vm.createContext({}); Object.defineProperty(c, "ro", { value: 1, enumerable: true }); return json([vm.runInContext("ro = 2; ro", c), c.ro]); });
t("ctx: read-only strict", () => { const c = vm.createContext({}); Object.defineProperty(c, "ro", { value: 1 }); return vm.runInContext("'use strict'; ro = 2", c); });
t("ctx: undefined assign strict", () => vm.runInNewContext("'use strict'; undefined = 2", {}));
t("ctx: strict undeclared", () => vm.runInNewContext("'use strict'; undeclared = 2", {}));
t("ctx: typeof undeclared", () => vm.runInNewContext("typeof nothing", {}));
t("ctx: not defined", () => vm.runInNewContext("nothing", {}));
t("ctx: symbols", () => { const s = Symbol("s"); const c = vm.createContext({ [s]: 1 }); return json([vm.runInContext("1", c), c[s]]); });
t("ctx: inherited", () => { const c = vm.createContext(Object.create({ inherited: 7 })); return vm.runInContext("inherited", c); });
t("ctx: keys", () => { const c = vm.createContext({ z: 1 }); vm.runInContext("var a = 1; b = 2; this.c = 3", c); return json(Object.keys(c).sort()); });
t("ctx: Object.keys(this)", () => json(vm.runInNewContext("var a = 1; function b() {}; c = 3; Object.keys(this)", { z: 1 }).sort()));
t("ctx: globalThis props", () => json(vm.runInNewContext("globalThis.x = 1; this.y = 2; [x, y]", {})));
t("ctx: frozen sandbox", () => { const c = vm.createContext(Object.freeze({ fz: 1 })); return json([vm.runInContext("fz = 5; fz", c), vm.runInContext("var nv = 1; typeof nv", c)]); });
t("ctx: indirect eval", () => { const c = vm.createContext({}); return json([vm.runInContext("(0, eval)('var ev = 1; this') === globalThis", c), c.ev]); });
t("ctx: eval var", () => { const c = vm.createContext({}); vm.runInContext("eval('var ev2 = 2')", c); return c.ev2; });
t("ctx: Function", () => vm.runInNewContext("new Function('return this')() === globalThis", {}));
t("ctx: functions see later edits", () => { const c = vm.createContext({ n: 1 }); const f = vm.runInContext("(function () { return n })", c); c.n = 2; return f(); });
t("ctx: callback writes", () => { const c = vm.createContext({ n: 0, bump(v) { this.n += v; } }); vm.runInContext("bump(2); n += 3", c); return c.n; });
t("ctx: callback sees script write", () => { const c = vm.createContext({ seen: null, peek() { this.seen = this.n; } }); vm.runInContext("n = 8; peek()", c); return c.seen; });
t("ctx: promise job", () => { const c = vm.createContext({ log: [] }); vm.runInContext("Promise.resolve(1).then((v) => log.push(v))", c); return "queued"; });
t("ctx: result realm", () => { const r = vm.runInNewContext("({ a: [1] })", {}); return json([r.constructor === Object, Array.isArray(r.a), r.a instanceof Array]); });
t("ctx: instance of context type", () => vm.runInNewContext("[] instanceof Array && ({}) instanceof Object && /a/ instanceof RegExp", {}));
t("ctx: Error API", () => vm.runInNewContext("typeof Error.stackTraceLimit + typeof Error.captureStackTrace", {}));
t("ctx: DONT_CONTEXTIFY", () => { const g = vm.createContext(vm.constants.DONT_CONTEXTIFY); vm.runInContext("var dc = 1; x = 2", g); return json([vm.isContext(g), g.dc, g.x, vm.runInContext("this", g) === g, g === globalThis]); });
t("runInNewContext", () => { const o = { n: 1 }; vm.runInNewContext("n++; m = 2", o); return json(o); });
t("runInNewContext no sandbox", () => vm.runInNewContext("1 + 2"));
t("microtaskMode", () => { vm.createContext({}, { microtaskMode: "afterEvaluate" }); return "ok"; });
t("microtaskMode bad", () => vm.createContext({}, { microtaskMode: "x" }));
t("name option", () => { vm.createContext({}, { name: "ctx", origin: "http://a" }); return "ok"; });

// --- Script ---------------------------------------------------------------
t("Script: instance", () => { const s = new vm.Script("1"); return json([typeof s.runInContext, Reflect.ownKeys(s), s instanceof vm.Script, String(Object.prototype.toString.call(s))]); });
t("Script: createScript", () => vm.createScript("2").runInThisContext());
t("Script: reuse", () => { const s = new vm.Script("n = (typeof n === 'number' ? n : 0) + 1"); const c = vm.createContext({}); s.runInContext(c); s.runInContext(c); return c.n; });
t("Script: other contexts", () => { const s = new vm.Script("typeof x"); return json([s.runInThisContext(), s.runInNewContext({ x: 1 }), s.runInContext(vm.createContext({ x: 1 }))]); });
t("Script: runInNewContext", () => new vm.Script("a + 1").runInNewContext({ a: 4 }));
t("Script: coerces code", () => new vm.Script(1 + 1).runInThisContext());
t("Script: options string", () => new vm.Script("1", "f.js").runInThisContext());
t("Script: options bad", () => new vm.Script("1", 5));
t("Script: filename bad", () => new vm.Script("1", { filename: 5 }));
t("Script: lineOffset bad", () => new vm.Script("1", { lineOffset: 1.5 }));
t("Script: run options bad", () => new vm.Script("1").runInThisContext({ timeout: -1 }));
t("Script: displayErrors bad", () => new vm.Script("1").runInThisContext({ displayErrors: 1 }));
t("Script: cachedData bad", () => new vm.Script("1", { cachedData: "x" }));
t("Script: produceCachedData", () => { const s = new vm.Script("1", { produceCachedData: true }); return json([Buffer.isBuffer(s.cachedData), s.cachedDataProduced, Reflect.ownKeys(s)]); });
t("Script: cachedData accepted", () => { const s = new vm.Script("1"); const c = new vm.Script("1", { cachedData: s.createCachedData() }); return json([Buffer.isBuffer(s.createCachedData()), c.cachedDataRejected]); });
t("Script: importModuleDynamically", () => new vm.Script("1", { importModuleDynamically: () => {} }).runInThisContext());
t("Script: importModuleDynamically bad", () => new vm.Script("1", { importModuleDynamically: 5 }));
t("Script: USE_MAIN_CONTEXT_DEFAULT_LOADER", () => new vm.Script("1", { importModuleDynamically: vm.constants.USE_MAIN_CONTEXT_DEFAULT_LOADER }).runInThisContext());
t("Script: sourceURL", () => { const s = new vm.Script("1\n//# sourceURL=a.js\n//# sourceMappingURL=a.map"); return json([s.sourceURL, s.sourceMapURL]); });
t("Script: no directives", () => { const s = new vm.Script("1"); return json([s.sourceURL, s.sourceMapURL, "sourceURL" in s]); });
t("Script: directive in a string", () => { const s = new vm.Script("'//# sourceURL=x.js'"); return json([s.sourceURL]); });
t("Script: hashbang", () => new vm.Script("#!/usr/bin/env node\n3").runInThisContext());
t("Script: unpaired surrogate", () => new vm.Script("'\\ud800'.length + '\ud800'.length").runInThisContext());
t("Script: lone surrogate source", () => json(new vm.Script("'\ud83d'").runInThisContext().charCodeAt(0)));

// --- compileFunction ------------------------------------------------------
t("compileFunction: basic", () => { const f = vm.compileFunction("return a + b", ["a", "b"]); return json([f(1, 2), f.name, f.length, f.toString()]); });
t("compileFunction: no params", () => vm.compileFunction("return typeof a")());
t("compileFunction: this and globals", () => vm.compileFunction("return [this === globalThis, typeof require]")().join());
t("compileFunction: strict", () => vm.compileFunction("'use strict'; return this")());
t("compileFunction: contextExtensions", () => vm.compileFunction("return x + y", [], { contextExtensions: [{ x: 1 }, { y: 2 }] })());
t("compileFunction: extension shadows", () => vm.compileFunction("return x", [], { contextExtensions: [{ x: 1 }, { x: 2 }] })());
t("compileFunction: parsingContext", () => { const c = vm.createContext({ zz: 1 }); const f = vm.compileFunction("return [typeof zz, [] instanceof Array, this === undefined]", [], { parsingContext: c }); return f().join(); });
t("compileFunction: parsingContext realm", () => { const c = vm.createContext({}); return vm.compileFunction("return Array", [], { parsingContext: c })() === vm.runInContext("Array", c); });
t("compileFunction: writes parsingContext", () => { const c = vm.createContext({}); vm.compileFunction("implicit = 4", [], { parsingContext: c })(); vm.runInContext("1", c); return c.implicit; });
t("compileFunction: min lineOffset", () => vm.compileFunction("return 1", [], { lineOffset: -2147483648 })());
t("compileFunction: closes wrapper", () => vm.compileFunction("}); (function () {", []));
t("compileFunction: bad params", () => vm.compileFunction("1", "a"));
t("compileFunction: bad code", () => vm.compileFunction(1));
t("compileFunction: bad parsingContext", () => vm.compileFunction("1", [], { parsingContext: {} }));
t("compileFunction: bad extensions", () => vm.compileFunction("1", [], { contextExtensions: [1] }));
t("compileFunction: cachedData", () => { const f = vm.compileFunction("return 1", [], { produceCachedData: true }); return json([Buffer.isBuffer(f.cachedData), f.cachedDataProduced]); });

// --- measureMemory --------------------------------------------------------
t("measureMemory: shape", () => vm.measureMemory() instanceof Promise);
t("measureMemory: bad", () => vm.measureMemory({ mode: "x" }));

console.log(out.join("\n"));
