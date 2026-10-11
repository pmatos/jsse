// Measured 2026-10-10, jsse HEAD 2b621c5 vs boa 0.22.0, buildbox3 at loadavg ~7, median of 5: jsse 8.05 s, boa 1.17 s, ratio 6.89x; jsse --no-bytecode 8.15 s (median of 3).
// Exercises: a realistic reduction of JetStream "lazy-collections" (Robin
// Malfait's lazy-collections, MIT): pipe()-composed lazy pipelines whose stages
// are `{ *[Symbol.iterator]() { for (const x of data) ... yield ... } }`
// objects, with isIterable()/isAsyncIterable() probes and yield* flattening.
// The async branches of the original are omitted (never executed in the
// benchmark; only the `isAsyncIterable(data) || data instanceof Promise` probe
// remains, as in the original).
function ensureFunction(input) { return typeof input === 'function' ? input : () => input; }
function pipe(...fns) {
  return fns.reduceRight((f, g) => { let g_ = ensureFunction(g); return (...args) => f(g_(...args)); }, ensureFunction(fns.pop()));
}
function isIterable(input) {
  if (typeof input !== 'object' || input === null) return false;
  return input[Symbol.iterator] !== undefined;
}
function isAsyncIterable(input) {
  if (typeof input !== 'object' || input === null) return false;
  return input[Symbol.asyncIterator] !== undefined;
}
function every(predicate) {
  return function everyFn(data) {
    if (data == null) return false;
    if (isAsyncIterable(data) || data instanceof Promise) throw new Error("async");
    let i = 0;
    for (let datum of data) { if (!predicate(datum, i++)) return false; }
    return true;
  };
}
function filter(fn) {
  return function filterFn(data) {
    if (isAsyncIterable(data) || data instanceof Promise) throw new Error("async");
    return { *[Symbol.iterator]() {
      let i = 0;
      for (let datum of data) { if (!fn(datum, i++)) continue; yield datum; }
    } };
  };
}
function flatMap(fn) {
  return function flatMapFn(data) {
    if (data == null) return;
    if (isAsyncIterable(data) || data instanceof Promise) throw new Error("async");
    return { *[Symbol.iterator]() {
      let i = 0;
      for (let datum of data) {
        let result = fn(datum, i++);
        if (isIterable(result)) yield* result; else yield result;
      }
    } };
  };
}
function reduce(fn, initial) {
  return function reduceFn(data) {
    if (data == null) return;
    let acc = initial;
    if (isAsyncIterable(data) || data instanceof Promise) throw new Error("async");
    let i = 0;
    for (let datum of data) acc = fn(acc, datum, i++);
    return acc;
  };
}
function map(fn) {
  return function mapFn(data) {
    if (data == null) return;
    if (isAsyncIterable(data) || data instanceof Promise) throw new Error("async");
    return { *[Symbol.iterator]() { let i = 0; for (let datum of data) yield fn(datum, i++); } };
  };
}
function toArray() {
  return (data) => reduce((acc, current) => { acc.push(current); return acc; }, [])(data);
}
function slice(begin = 0, end = Infinity) {
  return function sliceFn(data) {
    if (isAsyncIterable(data) || data instanceof Promise) throw new Error("async");
    return { *[Symbol.iterator]() {
      let iterator = Array.isArray(data) ? data[Symbol.iterator]() : data;
      let local_begin = begin;
      let local_end = end - local_begin;
      while (local_begin-- > 0) iterator.next();
      for (let datum of iterator) { yield datum; if (--local_end < 0) return; }
    } };
  };
}
function chunk(size) {
  return function chunkFn(data) {
    if (isAsyncIterable(data) || data instanceof Promise) throw new Error("async");
    return { *[Symbol.iterator]() {
      let chunk = [];
      for (let datum of data) {
        chunk.push(datum);
        if (chunk.length === size) { yield chunk; chunk = []; }
      }
      if (chunk.length > 0) yield chunk;
    } };
  };
}
function flatten(options = {}) {
  let { shallow = false } = options;
  return function flattenFn(data) {
    if (data == null) return;
    if (isAsyncIterable(data) || data instanceof Promise) throw new Error("async");
    return { *[Symbol.iterator]() {
      if (!isIterable(data)) { yield data; }
      else {
        for (let datum of data) {
          if (shallow) { if (isIterable(datum)) yield* datum; else yield datum; }
          else yield* flattenFn(datum);
        }
      }
    } };
  };
}
function clamp(input, min = Number.MIN_SAFE_INTEGER, max = Number.MAX_SAFE_INTEGER) {
  return Math.min(max, Math.max(min, input));
}
function* range(lowerbound, upperbound, step = 1) {
  let fixed_step = clamp(lowerbound < upperbound ? (step > 0 ? step : -step) : step > 0 ? -step : step);
  let lowerbound_ = clamp(lowerbound);
  let upperbound_ = clamp(upperbound);
  for (let i = lowerbound_; lowerbound_ < upperbound_ ? i <= upperbound_ : i >= upperbound_; i += fixed_step) {
    yield i;
  }
}
function take(amount) { return slice(0, Math.max(0, amount - 1)); }
function takeWhile(fn) {
  return function takeWhileFn(data) {
    if (isAsyncIterable(data) || data instanceof Promise) throw new Error("async");
    return { *[Symbol.iterator]() {
      let i = 0;
      for (let datum of data) { if (!fn(datum, i++)) return; yield datum; }
    } };
  };
}
function unique() {
  return function uniqueFn(data) {
    let seen = new Set([]);
    if (isAsyncIterable(data) || data instanceof Promise) throw new Error("async");
    return { *[Symbol.iterator]() {
      for (let datum of data) { if (!seen.has(datum)) { seen.add(datum); yield datum; } }
    } };
  };
}
const naturalNumbers = () => range(1, Infinity);
const isPrime = n => every(x => n % x)(range(2, n - 1));
const factors = n => filter(x => !(n % x))(range(1, n));
function* factorial() { let a = 1n, b = 1n; for (;;) { b *= a++; yield b; } }

let totalLength = 0;
const example = pipe(
  range(0, 10_000_000),
  filter(x => x % 100 === 0), filter(x => x % 4 === 0), filter(x => x % 400 === 0),
  takeWhile(x => x < 1_000), toArray());
totalLength += example().length;
totalLength += pipe(factorial(), chunk(3), flatten(), take(100), toArray())().length;
totalLength += pipe(naturalNumbers(), map(x => 2 ** (x + 1) - 1), filter(isPrime), take(5), toArray())().length;
totalLength += pipe(naturalNumbers(), filter(isPrime), take(150), toArray())().length;
totalLength += pipe(naturalNumbers(), flatMap(factors), filter(isPrime), unique(), take(150), toArray())().length;
totalLength += pipe(naturalNumbers(), map(x => { if (!(x % 15)) return "fizzbuzz"; if (!(x % 5)) return "buzz"; if (!(x % 3)) return "fizz"; return x; }), take(100), toArray())().length;
console.log("lazy_collections", totalLength);
