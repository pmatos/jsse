// ECMA-262 §§10.1.2.1, 10.1.7-10.1.9, and 10.5 permit an ordinary
// prototype cycle routed through a Proxy. Chain-walking internal methods must
// turn resource exhaustion into a catchable RangeError instead of overflowing
// the native Rust stack or looping forever (jsse#512).

function makeCycle() {
  var target = { present: 1 };
  var middle = {};
  var proxy = new Proxy(target, {});
  Object.setPrototypeOf(target, middle);
  Object.setPrototypeOf(middle, proxy);
  return { target: target, proxy: proxy };
}

function assertStackRangeError(label, operation) {
  var error;
  try {
    operation();
  } catch (caught) {
    error = caught;
  }

  if (!(error instanceof RangeError)) {
    throw new Error(label + " should throw a RangeError");
  }
  if (!/stack/i.test(String(error.message))) {
    throw new Error(label + " should report stack exhaustion: " + error.message);
  }
}

var cycle = makeCycle();

// Own descriptors short-circuit before the cycle is traversed.
if (cycle.target.present !== 1 || !("present" in cycle.target)) {
  throw new Error("own get/has should not traverse the prototype cycle");
}
cycle.target.present = 2;
if (cycle.target.present !== 2) {
  throw new Error("own set should not traverse the prototype cycle");
}

assertStackRangeError("[[Get]]", function () {
  return cycle.target.missing;
});

assertStackRangeError("[[Set]]", function () {
  cycle.target.missing = 1;
});
if (Object.prototype.hasOwnProperty.call(cycle.target, "missing")) {
  throw new Error("failed cyclic [[Set]] should not create a property");
}

assertStackRangeError("[[HasProperty]]", function () {
  return "missing" in cycle.target;
});

function Unrelated() {}
assertStackRangeError("OrdinaryHasInstance", function () {
  return cycle.target instanceof Unrelated;
});

assertStackRangeError("Proxy-started for-in", function () {
  for (var key in cycle.proxy) {
    // Enumeration must finish its prototype walk before the body can run.
  }
});

// Repeated identity is not itself an error: Proxy hooks may mutate a dynamic
// chain before an otherwise missing trap forwards to the target.
var dynamicTarget = {};
var handlerReads = 0;
var dynamicHandler = {};
Object.defineProperty(dynamicHandler, "get", {
  configurable: true,
  get: function () {
    handlerReads++;
    Object.setPrototypeOf(dynamicTarget, null);
    return undefined;
  },
});
var dynamicProxy = new Proxy(dynamicTarget, dynamicHandler);
Object.setPrototypeOf(dynamicTarget, dynamicProxy);
if (dynamicProxy.missing !== undefined || handlerReads !== 1) {
  throw new Error("a handler getter should be able to terminate the chain");
}

var prototypeTrapCalls = 0;
var dynamicPrototypeProxy;
dynamicPrototypeProxy = new Proxy({}, {
  getPrototypeOf: function () {
    return prototypeTrapCalls++ === 0 ? dynamicPrototypeProxy : null;
  },
});
if (dynamicPrototypeProxy instanceof Unrelated || prototypeTrapCalls !== 2) {
  throw new Error("a temporarily repeated dynamic prototype should terminate");
}

// A caught resource error must not poison later property operations.
var recovered = { value: 42 };
if (recovered.value !== 42 || !("value" in recovered)) {
  throw new Error("property operations did not recover after cyclic traversal");
}
