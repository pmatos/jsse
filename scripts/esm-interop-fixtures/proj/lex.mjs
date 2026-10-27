// Differential driver for the export-name detection of CommonJS modules
// imported from ES modules (#1241): each case is written to a temporary
// directory and imported; the names found must match Node (cjs-module-lexer).
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const here = path.dirname(import.meta.filename);
const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'jsse-lex-'));
const cases = {
 eqeq: `exports.a == 1; exports.b = 2;`,
 ws: `exports . spaced = 1; exports [ 'br' ] = 2; module . exports . mod = 3;`,
 comments: `exports/*c*/.cm = 1; exports. /*x*/ cm2 = 2; module.exports // c\n.cm3 = 3;`,
 nested: `function f(){ if (x) { exports.deep = 1; } } const o = { exports: { fake: 1 } }; o.exports.notme = 1; foo.exports.alsonot = 2;`,
 template: "var a = `${exports.inTpl = 1}`; var b = `x${`y${exports.nest = 2}`}`; exports.afterTpl = 3;",
 regex: `var r = /[/]exports.reA = 1/; var s = a / b / c; if (x) /exports.reB = 1/.test(y); exports.afterRe = 1;`,
 division: `var x = a++ / 2; var y = (a) / 2 / exports.q; exports.divd = 1; x = {} / 2;`,
 braces: `var o = {a:1}; if (x) {} /re/.test(1); exports.afterBlock = 1; function g(){} /re2/; exports.afterFn = 1;`,
 reserved: `exports.default = 1; exports.class = 2; exports.__esModule = true; exports.constructor = 3;`,
 unicode: `exports.café = 1; exports['ünï'] = 2; exports.$dollar = 3; exports._u = 4;`,
 stringkeys: `exports['a b'] = 1; exports["q\\"uote"] = 2; exports['esc\\u0041'] = 3;`,
 litkeys: `module.exports = { 'a-b': c, "d": e, f };\nvar c, e, f;`,
 litnest: `module.exports = { a: { b: 1 }, c: 2 };`,
 litget: `var x = 1, y = 2; module.exports = { x, get a() { return 1; }, y };`,
 litgetcall: `var get = 1, y = 2; module.exports = { get, y };`,
 litgetstr: `module.exports = { get 'a'() { return 1; }, y: 1 };`,
 reexnoext: `module.exports = require('./noext-inner');`,
 reextxt: `module.exports = require('./inner.txt');`,
 getorder: `globalThis.__order = []; exports.p1 = 1; exports.p2 = 1; exports.p3 = 1; exports.p4 = 1; exports.p5 = 1; exports.p6 = 1; exports.p7 = 1; exports.p8 = 1; for (const n of Object.keys(exports)) Object.defineProperty(exports, n, { enumerable: true, get() { globalThis.__order.push(n); return 1; } });`,
 litcomma: `module.exports = { a, b, };\nvar a, b;`,
 litspace: `module.exports = { a: b , c: d, e };\nvar a,b,c,d,e;`,
 litspread: `module.exports = { ...a, b };\nvar a, b;`,
 litrequire: `module.exports = { ...require('./dot.cjs'), z };\nvar z;`,
 litcomment: `module.exports = { /* c */ a, // d\n b };\nvar a, b;`,
 exportsAssign: `exports = { a: 1 }; module.exports = { b: 2 };`,
 modAlias: `var m = module; m.exports.viaAlias = 1; exports.direct = 2;`,
 defprop_computed: `var k = 'z'; Object.defineProperty(exports, k, { value: 1 }); Object.defineProperty(exports, 'lit', { value: 2 });`,
 defprop_enum_false: `Object.defineProperty(exports, 'nf', { enumerable: false, value: 1 });`,
 defprop_getter_arrow: `Object.defineProperty(exports, 'arrow', { enumerable: true, get: () => 1 });`,
 defprop_getter_unsafe_then_assign: `Object.defineProperty(exports, 'u', { enumerable: true, get: function () { return f(); } }); exports.u = 1; exports.v = 2;`,
 defprop_two: `Object.defineProperty(exports, 'one', { enumerable: true, value: 1 }); Object.defineProperty(module.exports, 'two', { enumerable: true, value: 2 });`,
 defprop_getter_semis: `var s = {p:1}; Object.defineProperty(exports, 'sg', { enumerable: true, get: function () { return s.p; }, });`,
 defprop_getter_named: `var s = {p:1}; Object.defineProperty(exports, 'ng', { enumerable: true, get: function named() { return s.p; } });`,
 reexport_nested: `module.exports = require('./reexport.cjs');`,
 reexport_after_assign: `exports.first = 1; module.exports = require('./dot.cjs'); exports.second = 2;`,
 reexport_two: `module.exports = { ...require('./dot.cjs'), ...require('./define.cjs') };`,
 reexport_missing: `module.exports = require('./does-not-exist.cjs');`,
 reexport_builtin: `module.exports = require('fs');`,
 reexport_dq: `module.exports = require("./dot.cjs");`,
 reexport_space: `module.exports = require ( './dot.cjs' ) ;`,
 exportstar_member: `var tslib = {}; tslib.__exportStar(require('./dot.cjs'), exports);`,
 export_fn: `function __export(m) { for (var p in m) if (!exports.hasOwnProperty(p)) exports[p] = m[p]; } __export(require('./dot.cjs'));`,
 babel_wildcard: `var _dot = _interopRequireWildcard(require('./dot.cjs'));\nObject.keys(_dot).forEach(function (key) {\n  if (key === "default" || key === "__esModule") return;\n  exports[key] = _dot[key];\n});\nfunction _interopRequireWildcard(x){return x}`,
 babel_not_default: `var _dot = require('./dot.cjs');\nObject.keys(_dot).forEach(function (key) {\n  exports[key] = _dot[key];\n});`,
 babel_hasown: `var _dot = require('./dot.cjs');\nvar _exportNames = {};\nObject.keys(_dot).forEach(function (key) {\n  if (key === "default" || key === "__esModule") return;\n  if (Object.prototype.hasOwnProperty.call(_exportNames, key)) return;\n  if (key in exports && exports[key] === _dot[key]) return;\n  exports[key] = _dot[key];\n});`,
 babel_nested: `function wrap(){ var _dot = require('./dot.cjs');\nObject.keys(_dot).forEach(function (key) {\n  if (key === "default" || key === "__esModule") return;\n  exports[key] = _dot[key];\n}); }`,
 shebang: `#!/usr/bin/env node\nexports.afterShebang = 1;`,
 bom: `﻿exports.afterBom = 1;`,
 asi: `exports.a = 1\nexports.b = 2\nmodule.exports.c = 3`,
 chain: `exports.x = exports.y = exports.z = void 0;`,
 afterUnterminated: `var s = 'abc\nexports.after = 1;`,
 objkeys_other: `Object.keys(foo).forEach(function(k){ exports[k] = 1; }); exports.real = 1;`,
 trailing_dot: `exports.tr = 1; var q = obj.exports.nope = 1; var w = obj?.exports.nope2;`,
 classbody: `class A { m() { return 1 } }\nexports.A = A; var d = 4 / 2;`,
 jsonish: `module.exports = {"a": b, "c": d};\nvar b, d;`,
 numeric: `var n = 1.5 / 2; var h = 0xff / 2; exports.num = 1;`,
 spreadid: `var o = {a:1}; module.exports = { ...o, k: 1 };`,
 defprop_module_getter: `Object.defineProperty(module.exports, 'mg', { enumerable: true, get: function () { return g.h; } }); var g = {h: 1};`,
 spacedkw: `Object . defineProperty ( exports , 'sp' , { enumerable : true , value : 1 } );`,
 lateModule: `var module2 = 1; exports.after_module2 = 1; modulex.exports.nope = 1;`,
};
for (const helper of ['dot', 'define', 'reexport']) {
  fs.copyFileSync(path.join(here, 'cjs', `${helper}.cjs`), path.join(dir, `${helper}.cjs`));
}
fs.writeFileSync(path.join(dir, 'noext-inner'), 'exports.inner = 1;');
fs.writeFileSync(path.join(dir, 'inner.txt'), 'exports.inner2 = 1;');
const out = [];
for (const [name, source] of Object.entries(cases)) {
  fs.writeFileSync(path.join(dir, `${name}.cjs`), source);
  try {
    const ns = await import(path.join(dir, `${name}.cjs`));
    out.push(`${name} ${JSON.stringify(Object.keys(ns).filter((k) => k !== 'default' && k !== 'module.exports'))}`);
  } catch (e) {
    out.push(`${name} threw ${e.name} ${e.code}`);
  }
}
out.push(`order ${globalThis.__order.join(',')}`);
fs.rmSync(dir, { recursive: true, force: true });
console.log(out.join('\n'));
