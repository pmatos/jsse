'use strict';
// Probe for scripts/run-node-perf-oracle.sh (#1227): every line is a
// deterministic fact (a value, a shape, an error message, an ordering) or a
// boolean about a moving value (monotonic, plausible), so jsse --node and Node
// must print identical output.
const util = require('util');
const perfHooks = require('node:perf_hooks');
const { performance, PerformanceObserver, createHistogram, monitorEventLoopDelay } = perfHooks;

function show(name, value) {
  console.log(name, util.inspect(value, { depth: 6, breakLength: Infinity }));
}
function attempt(name, fn) {
  try {
    show(name, fn());
  } catch (e) {
    show(name, [e.constructor.name, e.name, e.code, e.message]);
  }
}
const round = (n) => Math.round(n * 1e6) / 1e6;
const finiteNonNegative = (n) => typeof n === 'number' && Number.isFinite(n) && n >= 0;

// ---- process identity -------------------------------------------------------
show('identity', [process.platform, process.arch, typeof process.pid, typeof process.ppid,
  typeof process.title, process.version === `v${process.versions.node}`]);
show('release', [Object.keys(process.release), process.release.name,
  process.release.sourceUrl === `https://nodejs.org/download/release/${process.version}/node-${process.version}.tar.gz`]);
show('features', [Object.keys(process.features), Object.isFrozen(process.features),
  Object.entries(process.features).filter(([k]) => k !== 'typescript').map(([, v]) => typeof v)]);
show('config', [Object.keys(process.config), Object.isFrozen(process.config),
  Object.isFrozen(process.config.variables),
  process.config.variables.host_arch === process.arch]);
show('versions', [typeof process.versions.node, Object.isFrozen(process.versions)]);
const names = ['version', 'versions', 'arch', 'platform', 'release', 'config', 'features', 'pid', 'ppid',
  'title', 'uptime', 'hrtime', 'memoryUsage', 'cpuUsage', 'threadCpuUsage', 'resourceUsage', 'umask'];
for (const name of names) {
  const d = Object.getOwnPropertyDescriptor(process, name);
  const f = typeof process[name] === 'function' ? [process[name].name, process[name].length] : [];
  show(`prop ${name}`, [d.writable, d.enumerable, d.configurable, 'value' in d, ...f]);
}
show('hrtime.bigint', [process.hrtime.bigint.name, process.hrtime.bigint.length, typeof process.memoryUsage.rss]);

// ---- moving values: monotonic and plausible ------------------------------------
const h1 = process.hrtime();
const b1 = process.hrtime.bigint();
let spin = 0;
for (let i = 0; i < 2e5; i++) spin += i;
const h2 = process.hrtime();
const b2 = process.hrtime.bigint();
const d = process.hrtime(h1);
show('hrtime', [Array.isArray(h1), h1.length, h1.every(Number.isInteger),
  h1[1] < 1e9, b2 > b1, typeof b1,
  h2[0] > h1[0] || (h2[0] === h1[0] && h2[1] >= h1[1]),
  d[0] >= 0 && d[1] >= 0 && d[1] < 1e9, Math.abs(Number(b2) / 1e9 - (h2[0] + h2[1] / 1e9)) < 1]);
const up1 = process.uptime();
const up2 = process.uptime();
show('uptime', [typeof up1, up1 > 0, up2 >= up1, up1 < 600]);
const mem = process.memoryUsage();
show('memoryUsage', [Object.keys(mem), Object.values(mem).every(finiteNonNegative),
  mem.heapUsed <= mem.heapTotal, mem.heapUsed <= mem.rss, mem.rss > 1e6, finiteNonNegative(process.memoryUsage.rss())]);
const junk = [];
for (let i = 0; i < 2e5; i++) junk.push({ i });
show('heap grows', [process.memoryUsage().heapUsed > mem.heapUsed || process.memoryUsage().rss > mem.rss, junk.length]);
const cpu1 = process.cpuUsage();
for (let i = 0; i < 3e6; i++) spin += i;
const cpu2 = process.cpuUsage();
const cpuDiff = process.cpuUsage(cpu1);
show('cpuUsage', [Object.keys(cpu1), Object.values(cpu1).every(finiteNonNegative),
  cpu2.user >= cpu1.user, cpu2.system >= cpu1.system, cpuDiff.user >= 0, cpuDiff.system >= 0,
  Object.keys(process.threadCpuUsage())]);
const ru = process.resourceUsage();
show('resourceUsage', [Object.keys(ru), Object.values(ru).every(finiteNonNegative), ru.maxRSS > 0]);

// ---- argument checks ---------------------------------------------------------
attempt('cpuUsage(1)', () => process.cpuUsage(1));
attempt('cpuUsage neg', () => process.cpuUsage({ user: -1, system: 1 }));
attempt('cpuUsage sys missing', () => process.cpuUsage({ user: 1 }));
attempt('cpuUsage inf', () => process.cpuUsage({ user: 1, system: Infinity }));
attempt('hrtime(1)', () => process.hrtime(1));
attempt('hrtime([1])', () => process.hrtime([1]));
attempt('umask 9', () => process.umask('9'));
attempt('umask -1', () => process.umask(-1));
attempt('umask {}', () => process.umask({}));
attempt('umask 1.5', () => process.umask(1.5));
const oldMask = process.umask();
show('umask', [process.umask(0o77) === oldMask, process.umask(), process.umask('022'), process.umask() === 0o22,
  process.umask(oldMask) === 0o22, process.umask() === oldMask]);

// ---- performance -------------------------------------------------------------
const n1 = performance.now();
const n2 = performance.now();
show('now', [typeof n1, n1 > 0, n2 >= n1, n1 < 600000]);
show('timeOrigin', [typeof performance.timeOrigin,
  Math.abs(performance.timeOrigin + performance.now() - Date.now()) < 100]);
show('performance shape', [Object.prototype.toString.call(performance), Object.keys(perfHooks),
  Object.keys(Object.getOwnPropertyDescriptors(Object.getPrototypeOf(performance))),
  performance === globalThis.performance, globalThis.PerformanceObserver === PerformanceObserver,
  Object.keys(perfHooks.constants)]);
show('nodeTiming', [Object.keys(performance.nodeTiming.toJSON()), performance.nodeTiming.name,
  performance.nodeTiming.entryType, performance.nodeTiming.duration > 0]);
show('toJSON', [Object.keys(performance.toJSON()), Object.keys(performance.eventLoopUtilization())]);

const markA = performance.mark('a', { detail: { x: [1, 2] } });
const markB = performance.mark('b', { startTime: markA.startTime + 5 });
show('mark', [markA.name, markA.entryType, markA.duration, markA.detail, markB.detail,
  round(markB.startTime - markA.startTime), Object.keys(markA.toJSON())]);
show('inspect mark', util.inspect(markA).replace(/startTime: [\d.e+-]+/, 'startTime: N'));
const measure = performance.measure('a-b', 'a', 'b');
show('measure', [measure.name, measure.entryType, Math.abs(measure.duration - 5) < 1e-6, measure.detail,
  measure.constructor.name]);
const m2 = performance.measure('opt', { start: 'a', duration: 3, detail: 'd' });
const m3 = performance.measure('opt2', { end: 'b', duration: 2 });
const m4 = performance.measure('since-origin');
show('measure options', [round(m2.duration), m2.detail, round(m3.duration), m3.startTime === markB.startTime - 2,
  m4.startTime, m4.duration > 0]);
attempt('mark()', () => performance.mark());
attempt('mark neg', () => performance.mark('x', { startTime: -1 }));
attempt('mark startTime str', () => performance.mark('x', { startTime: 'a' }));
attempt('measure()', () => performance.measure());
attempt('measure missing', () => performance.measure('x', 'nope'));
attempt('measure bad opts', () => performance.measure('x', { start: 1, end: 2, duration: 3 }));
attempt('measure endMark+opts', () => performance.measure('x', { start: 1 }, 'a'));
attempt('measure empty opts', () => performance.measure('x', { detail: 1 }, 'a').duration >= 0);
attempt('mark nodeStart', () => performance.mark('nodeStart'));
attempt('new PerformanceEntry', () => new perfHooks.PerformanceEntry());
attempt('brand', () => performance.now.call({}));
show('entries', [performance.getEntries().map((e) => e.name),
  performance.getEntriesByName('a').length, performance.getEntriesByName('a', 'measure').length,
  performance.getEntriesByType('mark').map((e) => e.name), performance.getEntriesByType('nope')]);
attempt('getEntriesByName()', () => performance.getEntriesByName());
performance.clearMarks('a');
performance.clearMeasures('opt');
show('cleared', performance.getEntries().map((e) => `${e.entryType}:${e.name}`));
performance.clearMarks();
performance.clearMeasures();
show('cleared all', performance.getEntries().length);

// ---- PerformanceObserver ---------------------------------------------------------
show('supportedEntryTypes', PerformanceObserver.supportedEntryTypes);
attempt('observer()', () => new PerformanceObserver());
attempt('observe()', () => new PerformanceObserver(() => {}).observe());
attempt('observe both', () => new PerformanceObserver(() => {}).observe({ entryTypes: ['mark'], type: 'mark' }));
attempt('observe non-array', () => new PerformanceObserver(() => {}).observe({ entryTypes: 'mark' }));
const seen = [];
const observer = new PerformanceObserver((list, obs) => {
  seen.push(['entries', list.getEntries().map((e) => `${e.entryType}:${e.name}`),
    list.getEntriesByType('mark').length, list.getEntriesByName('m1').length, obs === observer]);
});
observer.observe({ entryTypes: ['mark', 'measure'] });
performance.mark('m1');
performance.mark('m2');
performance.measure('me', 'm1', 'm2');
show('takeRecords', observer.takeRecords().length);
performance.mark('m3');
performance.mark('m4');
seen.push(['sync', 'after queue']);

const fn = perfHooks.timerify(function work(x) { return x * 2; });
show('timerify', [fn.name, fn.length, fn(4)]);
const fnSeen = [];
const fnObserver = new PerformanceObserver((list) => {
  fnSeen.push(list.getEntries().map((e) => [e.name, e.entryType, e.duration >= 0, e.detail]));
});
fnObserver.observe({ type: 'function' });
fn(5);

// ---- histograms --------------------------------------------------------------------
function dump(name, h) {
  show(name, [h.min, h.max, h.mean, h.stddev, h.count, h.exceeds, h.percentiles,
    h.minBigInt, h.maxBigInt, h.countBigInt, h.exceedsBigInt, h.percentilesBigInt]);
}
const hist = createHistogram();
for (const v of [1, 2, 3, 4, 5, 100, 2000, 2001, 100000, 1234567]) hist.record(v);
dump('hist', hist);
show('hist percentile', [1, 25, 50, 75, 99, 100].map((p) => [hist.percentile(p), hist.percentileBigInt(p)]));
dump('hist empty', createHistogram());
const small = createHistogram({ lowest: 1, highest: 100, figures: 1 });
small.record(150);
small.record(7);
small.record(7n);
dump('hist small', small);
const other = createHistogram();
other.record(9);
hist.add(other);
show('hist add', [hist.count, hist.max]);
hist.reset();
dump('hist reset', hist);
const wide = createHistogram({ lowest: 1000, highest: 1e9, figures: 3 });
for (let i = 1; i <= 50; i++) wide.record(i * 12345);
dump('hist wide', wide);
attempt('record 0', () => hist.record(0));
attempt('record 1.5', () => hist.record(1.5));
attempt('percentile 0', () => hist.percentile(0));
attempt('percentile 101', () => hist.percentile(101));
attempt('createHistogram bad', () => createHistogram({ highest: 1 }));
attempt('createHistogram figures', () => createHistogram({ figures: 6 }));
attempt('new Histogram', () => new (Object.getPrototypeOf(Object.getPrototypeOf(hist)).constructor)());
hist.recordDelta();
show('recordDelta first', hist.count);
show('hist JSON keys', Object.keys(hist.toJSON()).filter((k) => ['count', 'min', 'max', 'mean', 'exceeds', 'stddev', 'percentiles'].includes(k)));

// ---- monitorEventLoopDelay ---------------------------------------------------------
attempt('eld bad', () => monitorEventLoopDelay({ resolution: 0 }));
const eld = monitorEventLoopDelay({ resolution: 5 });
show('eld', [eld.constructor.name, eld.enable(), eld.enable(), eld.count]);

setTimeout(() => {
  show('eld after', [eld.disable(), eld.disable(), eld.count > 0, eld.min > 0, eld.max >= eld.min, eld.mean > 0]);
  show('observer calls', seen);
  show('function observer', fnSeen);
  const buffered = new PerformanceObserver((list) => {
    show('buffered', list.getEntries().map((e) => e.name));
  });
  buffered.observe({ type: 'mark', buffered: true });
  setImmediate(() => buffered.disconnect());
}, 60);
