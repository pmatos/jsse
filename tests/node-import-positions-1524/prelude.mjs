Error.stackTraceLimit = 100;
const report = (e) => { const l = String(e.stack).split('\n').find((l) => l.includes('probe.mjs:')); console.log(l ? l.trim().replace(/.*probe\.mjs:/, '') : 'NOFRAME'); };
process.on('unhandledRejection', report);
process.on('uncaughtException', report);
const id = (...a) => a[0], gl = { q: 0 }, o = { m() { return o; }, p: 1, f() { return 1; }, g: { h() { return 1; } } }, arr = { b: () => 1 }, b = 'b', C = class { constructor() {} }, tag = (s) => 1, fn = () => {}, n = 1, v = './nope.mjs', obj = { v: './nope.mjs' };
