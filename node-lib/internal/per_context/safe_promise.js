'use strict';

// The SafePromise helpers of Node's lib/internal/per_context/primordials.js
// that the web streams and their adapters use, reached lazily through the
// `primordials` table of the builtin loader (SafePromiseAll and friends).

const {
  ArrayPrototypeForEach,
  ArrayPrototypeMap,
  ObjectFreeze,
  ObjectSetPrototypeOf,
  Promise,
  PromisePrototypeThen,
  PromiseResolve,
  ReflectDefineProperty,
  ReflectGetOwnPropertyDescriptor,
  ReflectOwnKeys,
  SafeArrayIterator,
} = primordials;

function copyProps(src, dest) {
  ArrayPrototypeForEach(ReflectOwnKeys(src), (key) => {
    if (!ReflectGetOwnPropertyDescriptor(dest, key)) {
      ReflectDefineProperty(dest, key, {
        __proto__: null,
        ...ReflectGetOwnPropertyDescriptor(src, key),
      });
    }
  });
}

class SafePromise extends Promise {}
copyProps(Promise.prototype, SafePromise.prototype);
copyProps(Promise, SafePromise);
ObjectSetPrototypeOf(SafePromise.prototype, null);
ObjectFreeze(SafePromise.prototype);
ObjectFreeze(SafePromise);

const arrayToSafePromiseIterable = (promises, mapFn) =>
  new SafeArrayIterator(
    ArrayPrototypeMap(
      promises,
      (promise, i) =>
        new SafePromise((a, b) => PromisePrototypeThen(mapFn == null ? promise : mapFn(promise, i), a, b)),
    ),
  );

const SafePromisePrototypeFinally = (thisPromise, onFinally) =>
  new Promise((a, b) =>
    new SafePromise((a, b) => PromisePrototypeThen(thisPromise, a, b))
      .finally(onFinally)
      .then(a, b),
  );

const SafePromiseAll = (promises, mapFn) =>
  new Promise((a, b) =>
    SafePromise.all(arrayToSafePromiseIterable(promises, mapFn)).then(a, b),
  );

const SafePromiseAllReturnVoid = (promises, mapFn) =>
  new Promise((resolve, reject) => {
    let pendingPromises = promises.length;
    if (pendingPromises === 0) resolve();
    const onFulfilled = () => {
      if (--pendingPromises === 0) {
        resolve();
      }
    };
    for (let i = 0; i < promises.length; i++) {
      const promise = mapFn != null ? mapFn(promises[i], i) : promises[i];
      PromisePrototypeThen(PromiseResolve(promise), onFulfilled, reject);
    }
  });

module.exports = {
  SafePromiseAll,
  SafePromiseAllReturnVoid,
  SafePromisePrototypeFinally,
};
