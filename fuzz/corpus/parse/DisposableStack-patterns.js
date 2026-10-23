// Tests DisposableStack, Symbol.dispose, and related patterns.
// Spec: ECMAScript 2024, sec-disposablestack-objects

// Basic DisposableStack
var log = [];
var stack = new DisposableStack();

stack.defer(function() { log.push("deferred1"); });
stack.defer(function() { log.push("deferred2"); });

stack.dispose();

// LIFO order
if (log[0] !== "deferred2" || log[1] !== "deferred1") {
  throw new Test262Error('defer should dispose in LIFO order, got: ' + JSON.stringify(log));
}

// disposed property
if (stack.disposed !== true) {
  throw new Test262Error('stack.disposed should be true after dispose()');
}

// Double dispose is a no-op
stack.dispose();
if (log.length !== 2) {
  throw new Test262Error('double dispose should not re-run callbacks');
}

// use() method with Symbol.dispose
var log2 = [];
var stack2 = new DisposableStack();

var resource = {
  [Symbol.dispose]: function() { log2.push("resource disposed"); }
};
var returned = stack2.use(resource);
if (returned !== resource) {
  throw new Test262Error('use() should return the resource');
}

stack2.dispose();
if (log2[0] !== "resource disposed") {
  throw new Test262Error('use() resource should be disposed, got: ' + JSON.stringify(log2));
}

// adopt() method
var log3 = [];
var stack3 = new DisposableStack();

var handle = { id: 42 };
stack3.adopt(handle, function(h) {
  log3.push("adopted " + h.id);
});

stack3.dispose();
if (log3[0] !== "adopted 42") {
  throw new Test262Error('adopt() callback should receive the value, got: ' + JSON.stringify(log3));
}

// move() method
var log4 = [];
var stack4 = new DisposableStack();
stack4.defer(function() { log4.push("moved"); });

var stack5 = stack4.move();
if (stack4.disposed !== true) {
  throw new Test262Error('source stack should be disposed after move()');
}
if (stack5.disposed !== false) {
  throw new Test262Error('target stack should not be disposed after move()');
}

stack5.dispose();
if (log4[0] !== "moved") {
  throw new Test262Error('moved resource should be disposed from new stack');
}

// Error suppression with SuppressedError
var stack6 = new DisposableStack();
stack6.defer(function() { throw new Error("dispose error"); });

try {
  // Simulate: the body throws, then dispose also throws
  stack6[Symbol.dispose]();
  // If no body error, dispose error propagates directly
} catch (e) {
  if (!(e instanceof Error)) {
    throw new Test262Error('dispose error should propagate as Error');
  }
}

// use(null) and use(undefined) should be accepted
var stack7 = new DisposableStack();
stack7.use(null);
stack7.use(undefined);
stack7.dispose();
