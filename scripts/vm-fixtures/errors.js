// `vm` error-behaviour fixtures (issue #1238): the SyntaxError header, the
// realm a syntax error is thrown in, and the file names, lines and columns of
// script and compileFunction stack frames. Same contract as probe.js.
"use strict";
const vm = require("node:vm");
const path = require("node:path");

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

// Frames of the script under test only: no Node internals, no fixture frames.
function frames(error, columns = true) {
  const here = path.basename(__filename);
  return String(error.stack)
    .split("\n")
    .filter((l) => /^ {4}at /.test(l) && !/node:|<anonymous>:\d|Module\.|Object\.<anonymous>|show/.test(l) && !l.includes(here))
    .map((l) => (columns ? l : l.replace(/:\d+:\d+\)?$/, "")))
    .join("|");
}
// First lines of a stack up to the message line, minus the arrow Node adds
// above runtime errors (this engine adds it for syntax errors only).
function head(error) {
  const lines = String(error.stack).split("\n");
  return lines.find((l) => l === error.name || l.startsWith(error.name + ":")) || "";
}

// --- error stacks and file names -----------------------------------------
function syntaxCase(label, f) {
  try {
    f();
    log(label, "no throw");
  } catch (e) {
    const lines = String(e.stack).split("\n");
    log(label, json([e.name, e instanceof SyntaxError, lines.slice(0, 3)]));
  }
}
syntaxCase("syntax: Script filename", () => new vm.Script("foo bar", { filename: "x.js" }));
syntaxCase("syntax: Script default filename", () => new vm.Script("foo bar"));
syntaxCase("syntax: offsets", () => new vm.Script("foo bar", { filename: "x.js", lineOffset: 5, columnOffset: 3 }));
syntaxCase("syntax: second line", () => new vm.Script("var a = 1;\nvar b = ;\n", { filename: "y.js" }));
syntaxCase("syntax: realm of this", () => { try { new vm.Script("foo bar"); } catch (e) { if (!(e instanceof SyntaxError)) throw new Error("wrong realm"); throw e; } });
t("syntax: runInContext realm", () => { const c = vm.createContext({}); try { vm.runInContext("foo bar", c, "z.js"); } catch (e) { return json([e instanceof SyntaxError, e.constructor === vm.runInContext("SyntaxError", c), e.name]); } });
t("syntax: runInNewContext realm", () => { try { vm.runInNewContext("foo bar", {}); } catch (e) { return json([e instanceof SyntaxError, e.name]); } });
t("syntax: runInThisContext realm", () => { try { vm.runInThisContext("foo bar"); } catch (e) { return json([e instanceof SyntaxError, e.name]); } });
t("syntax: redeclare", () => { try { vm.runInNewContext("let a; let a", {}); } catch (e) { return e.name; } });
t("syntax: strict with", () => { try { vm.runInNewContext("'use strict'; with (a) {}", {}); } catch (e) { return e.name; } });
syntaxCase("syntax: compileFunction", () => vm.compileFunction("foo bar", [], { filename: "cf.js" }));
syntaxCase("syntax: compileFunction line", () => vm.compileFunction("var a = 1;\nfoo bar", [], { filename: "cf.js", lineOffset: 2 }));

function runtime(label, f, columns = true) {
  try {
    f();
    log(label, "no throw");
  } catch (e) {
    log(label, json([e.name, head(e).split(":")[0], frames(e, columns)]));
  }
}
runtime("stack: throw at top", () => vm.runInThisContext("throw new Error('boom')", { filename: "top.js" }));
runtime("stack: default filename", () => vm.runInThisContext("throw new Error('boom')"));
runtime("stack: offsets", () => vm.runInThisContext("\n\nthrow new Error('boom')", { filename: "off.js", lineOffset: 10, columnOffset: 5 }));
runtime("stack: first line column", () => vm.runInThisContext("throw new Error('boom')", { filename: "col.js", columnOffset: 5 }));
runtime("stack: nested functions", () => vm.runInThisContext("function a() { return new Error('m') }\nfunction b() { return a() }\nthrow b()", { filename: "nest.js" }));
runtime("stack: new context", () => vm.runInNewContext("throw new Error('boom')", {}, "nc.js"));
runtime("stack: context function later", () => {
  const c = vm.createContext({});
  vm.runInContext("function later() { throw new Error('later') }", c, "later.js");
  c.later();
});
runtime("stack: TypeError", () => vm.runInNewContext("(function f() { return null.x })()", {}, "te.js"), false);
runtime("stack: ReferenceError", () => vm.runInNewContext("(function f() { return missing })()", {}, "re.js"), false);
runtime("stack: Script reuse", () => { const s = new vm.Script("throw new Error('again')", { filename: "re.js" }); try { s.runInThisContext(); } catch {} s.runInNewContext({}); });
runtime("stack: compileFunction", () => vm.compileFunction("throw new Error('cf')", [], { filename: "cfn.js" })());
runtime("stack: compileFunction offsets", () => vm.compileFunction("\nthrow new Error('cf')", [], { filename: "cfo.js", lineOffset: 4 })());
runtime("stack: compileFunction columnOffset", () => vm.compileFunction("throw new Error('cf')", [], { filename: "cfc.js", columnOffset: 3 })());
runtime("stack: sourceURL", () => vm.runInThisContext("//# sourceURL=named.js\nthrow new Error('u')", { filename: "ignored.js" }));
runtime("stack: eval in script", () => vm.runInThisContext("eval('throw new Error(\"e\")')", { filename: "ev.js" }));
runtime("stack: captureStackTrace", () => vm.runInNewContext("const o = {}; Error.captureStackTrace(o); throw Object.assign(new Error('c'), { stack: o.stack })", {}, "cs.js"));
t("stack: prepareStackTrace sees file", () => {
  Error.prepareStackTrace = (e, sites) => sites.map((s) => `${s.getFileName()}:${s.getLineNumber()}:${s.getColumnNumber()}`).slice(0, 1).join();
  try { return vm.runInThisContext("new Error('x').stack", { filename: "pp.js", lineOffset: 1 }); } finally { Error.prepareStackTrace = undefined; }
});
t("stack: stackTraceLimit in context", () => vm.runInNewContext("Error.stackTraceLimit = 0; new Error('x').stack", {}));

console.log(out.join("\n"));
