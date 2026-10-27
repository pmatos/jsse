// Stand-in for test/common/index.mjs and node:test for the verbatim .mjs
// fixtures: the runners rewrite those two import specifiers to this file.
// Tests run in order once the module graph has evaluated, then expectations
// are verified after a quiet period.
const expectations = [];
const tests = [];
const failures = [];
let activity = 0;

export function mustCall(fn, exact = 1) {
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

export function it(name, options, fn) {
  tests.push({ name, fn: typeof options === 'function' ? options : fn });
}

async function run() {
  for (const { fn } of tests) {
    activity++;
    try {
      await fn();
    } catch (error) {
      failures.push(error);
    }
  }
  let quiet = 0;
  let last = -1;
  while (quiet < 3) {
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
