// Host timer model (jsse#254): distinct cancellable ids, the full
// setTimeout/clearTimeout/setInterval/clearInterval family, and no thread per
// armed timer.
//
// Only synchronous expectations are asserted here: an uncaught throw inside a
// timer callback is swallowed, as it is for an unhandled promise rejection, so
// a failed assertion in a callback would not reach the exit code. Callback
// timing and ordering are covered by the Rust tests in src/interpreter/tests.rs.
//
// Ids are numbers, as on the web platform. Node returns a Timeout object
// instead, so this file is deliberately not Node-portable.

function assert(cond, msg) {
  if (!cond) throw new Error(msg);
}

for (const name of ["setTimeout", "clearTimeout", "setInterval", "clearInterval"]) {
  assert(typeof globalThis[name] === "function", name + " must be a global function");
}

// Ids are distinct and truthy — setTimeout used to return 0 for every call.
const first = setTimeout(() => {}, 1000);
const second = setTimeout(() => {}, 1000);
const repeating = setInterval(() => {}, 1000);
assert(typeof first === "number", "setTimeout must return a number");
assert(first !== 0 && second !== 0 && repeating !== 0, "a timer id must never be 0");
assert(first !== second, "each timer gets its own id");
assert(second !== repeating, "setInterval shares the id space with setTimeout");

clearTimeout(first);
clearTimeout(second);
clearInterval(repeating);

// Clearing something that is not an armed timer is a no-op, not an error.
clearTimeout(0);
clearTimeout(first);
clearTimeout(undefined);
clearInterval("not an id");

// A non-callable callback is a TypeError.
for (const arm of [setTimeout, setInterval]) {
  let threw = false;
  try {
    arm(42, 0);
  } catch (e) {
    threw = e instanceof TypeError;
  }
  assert(threw, arm.name + " must reject a non-callable callback");
}

// The reported failure: many timers armed at once. Under a thread per timer
// this exhausted the OS thread limit and hung, which surfaces here as the
// runner's per-test timeout.
const ids = [];
for (let i = 0; i < 50000; i++) ids.push(setTimeout(() => {}, 50));
for (const id of ids) clearTimeout(id);
