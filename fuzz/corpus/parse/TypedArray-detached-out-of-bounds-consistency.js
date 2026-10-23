// Every %TypedArray%.prototype method that reads through its receiver's
// backing buffer must reject a detached buffer and a buffer that has been
// resized out from under a length-tracking view identically: throw a
// TypeError before doing any further work. Each method independently
// re-derives this same "is_detached || is_typed_array_out_of_bounds" guard
// (see e.g. ValidateTypedArray, 23.2.4.1 IsTypedArrayOutOfBounds), so a
// method that forgets one half of the check would silently operate on
// invalid backing memory instead of throwing.
// Spec: ECMAScript 2024, sec-validatetypedarray,
//       sec-istypedarrayoutofbounds.

function Test262Error(message) {
  this.message = message || "";
}
Test262Error.prototype.toString = function () {
  return "Test262Error: " + this.message;
};

var METHODS = [
  "at", "set", "slice", "copyWithin", "fill", "indexOf", "lastIndexOf",
  "includes", "reverse", "sort", "join", "toLocaleString", "toReversed",
  "toSorted", "with", "values", "entries", "keys", "forEach", "map",
  "filter", "every", "some", "find", "findIndex", "findLast",
  "findLastIndex", "reduce", "reduceRight"
];

function invoke(view, name) {
  switch (name) {
    case "set": return view.set([1]);
    case "with": return view.with(0, 1);
    case "fill": return view.fill(1);
    case "copyWithin": return view.copyWithin(0, 0);
    case "indexOf": case "lastIndexOf": case "includes": return view[name](1);
    case "forEach": case "map": case "filter": case "every": case "some":
    case "find": case "findIndex": case "findLast": case "findLastIndex":
      return view[name](function () { return true; });
    case "reduce": case "reduceRight":
      return view[name](function (acc) { return acc; }, 0);
    default: return view[name]();
  }
}

function assertThrowsTypeError(view, name, label) {
  var threw = null;
  try {
    invoke(view, name);
  } catch (e) {
    threw = e;
  }
  if (threw === null) {
    throw new Test262Error(label + ": " + name + " did not throw");
  }
  if (!(threw instanceof TypeError)) {
    throw new Test262Error(label + ": " + name + " threw " + threw.constructor.name + ", expected TypeError");
  }
}

// Detached buffer: every method must throw TypeError, not read stale memory.
METHODS.forEach(function (name) {
  var buf = new ArrayBuffer(8);
  var view = new Uint8Array(buf);
  buf.transfer();
  assertThrowsTypeError(view, name, "detached");
});

// Length-tracking view over a resizable buffer that has shrunk below the
// view's byteOffset: the view is "out of bounds" without being detached.
METHODS.forEach(function (name) {
  var rab = new ArrayBuffer(16, { maxByteLength: 16 });
  var view = new Uint8Array(rab, 8);
  rab.resize(4);
  assertThrowsTypeError(view, name, "out-of-bounds");
});

// Sanity: a live, in-bounds view must not be rejected by the same guard.
(function () {
  var buf = new ArrayBuffer(4);
  var view = new Uint8Array(buf);
  view.fill(7);
  if (view[0] !== 7) {
    throw new Test262Error("sanity: fill on a valid view should not throw / should apply");
  }
  view.at(0);
})();
