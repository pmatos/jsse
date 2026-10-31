// Stand-in for test/common/index.mjs, test/common/gc.js and node:test for the
// verbatim .mjs fixtures: the runners rewrite those import specifiers to this file.
// Tests run in order once the module graph has evaluated, then expectations
// are verified after a quiet period.
import nodeAssert from 'node:assert';
import { setImmediate as setImmediatePromisified } from 'node:timers/promises';

// Node runs with --expose-gc; jsse exposes the same collection as $262.gc().
if (typeof globalThis.gc !== 'function' && typeof $262 === 'object' && $262 !== null) {
  globalThis.gc = (options) => {
    $262.gc();
    if (options && options.execution === 'async') return Promise.resolve();
  };
}

// test/common/gc.js (Node v26.8.2), the part the fixtures use.
export async function gcUntil(name, condition, maxCount = 10, gcOptions) {
  for (let count = 0; count < maxCount; ++count) {
    await setImmediatePromisified();
    if (gcOptions) {
      await globalThis.gc(gcOptions);
    } else {
      await globalThis.gc();
    }
    if (condition()) {
      return;
    }
  }
  throw new Error(`Test ${name} failed`);
}

const expectations = [];
const tests = [];
const failures = [];
let activity = 0;

export function mustCall(fn, exact = 1) {
  if (typeof fn === 'number') { exact = fn; fn = undefined; }
  const expectation = { name: (fn && fn.name) || '<anonymous>', exact, actual: 0 };
  expectations.push(expectation);
  return function(...args) {
    expectation.actual++;
    activity++;
    return fn ? Reflect.apply(fn, this, args) : undefined;
  };
}

export function mustNotCall(message) {
  return function mustNotCall() {
    activity++;
    const error = new Error(message || 'function should not have been called');
    failures.push(error);
    throw error;
  };
}

export function describe(name, options, fn) {
  (typeof options === 'function' ? options : fn)();
}

// Subtests jsse cannot run yet, skipped on jsse only (they pass on Node).
const jsseSkips = {
  'does not prevent source signal from being GCed if it is short-lived':
    'a closure keeps its whole defining environment, so the last iteration\'s dead `ac` stays reachable from the running setImmediate callback',
};

export function it(name, options, fn) {
  const skip = (typeof options === 'object' && options !== null && (options.skip || options.todo)) ||
    (typeof __host_write === 'function' && Object.hasOwn(jsseSkips, name));
  tests.push({ name, skip, fn: typeof options === 'function' ? options : fn });
}

function runTest(fn) {
  const t = { assert: nodeAssert, diagnostic() {}, after() {} };
  if (fn.length < 2) return fn(t);
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('callback-style test never called done()')), 120000);
    fn(t, (error) => {
      clearTimeout(timer);
      if (error) reject(error);
      else resolve();
    });
  });
}

async function run() {
  for (const { fn, skip } of tests) {
    if (skip) continue;
    activity++;
    try {
      await runTest(fn);
    } catch (error) {
      failures.push(error);
    }
  }
  let quiet = 0;
  let last = -1;
  const deadline = Date.now() + 60000;
  while (quiet < 3 || (expectations.some((e) => e.actual < e.exact) && Date.now() <= deadline)) {
    if (last === activity) quiet++;
    else { quiet = 0; last = activity; }
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  const problems = failures.map((e) => String((e && e.stack) || e));
  for (const e of expectations) {
    if (e.actual !== e.exact) {
      problems.push(`mustCall ${e.name}: expected ${e.exact} calls, got ${e.actual}`);
    }
  }
  if (problems.length) {
    console.error(problems.join('\n'));
    process.exit(1);
  }
  console.log('events-fixture: ok');
  process.exit(0);
}

setTimeout(run, 0);
