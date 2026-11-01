'use strict';

// Transfer protocol for platform objects (#1528), after Node's
// lib/internal/worker/js_transferable.js. `markTransferMode` records the mode
// on the object (invisible to scripts); the engine's `structuredClone` calls
// the hook below for a marked object: `[kTransferList]()`, then `[kTransfer]()`
// or `[kClone]()`, and after cloning the returned `data` it builds the new
// object with `new Ctor()` plus `[kDeserialize](data)`.

const {
  Error,
  StringPrototypeSplit,
  Symbol,
} = primordials;

const {
  structuredClone,
  transferMark,
  setTransferHook,
} = require('internal/host/webstreams');

const kClone = Symbol('messaging_clone_symbol');
const kDeserialize = Symbol('messaging_deserialize_symbol');
const kTransfer = Symbol('messaging_transfer_symbol');
const kTransferList = Symbol('messaging_transfer_list_symbol');

const kTransferable = 1;
const kCloneable = 2;

function isObjectLike(value) {
  return (typeof value === 'object' && value !== null) || typeof value === 'function';
}

function markTransferMode(obj, cloneable = false, transferable = false) {
  if (!isObjectLike(obj)) return;
  let mode = 0;
  if (cloneable) mode |= kCloneable;
  if (transferable) mode |= kTransferable;
  transferMark(obj, mode);
}

function markAsUncloneable(obj) {
  if (!isObjectLike(obj)) return;
  transferMark(obj, transferMark(obj) & ~kCloneable);
}

function deserialize(deserializeInfo, data) {
  const { 0: module, 1: ctor } = StringPrototypeSplit(deserializeInfo, ':', 2);
  const Ctor = require(module)[ctor];
  if (typeof Ctor !== 'function' ||
      typeof Ctor.prototype[kDeserialize] !== 'function') {
    throw new Error(`Unknown deserialize spec ${deserializeInfo}`);
  }
  const object = new Ctor();
  object[kDeserialize](data);
  return object;
}

// Operation codes shared with src/interpreter/builtins/structured_clone/platform.rs.
setTransferHook((op, object, data) => {
  switch (op) {
    case 0:
      return typeof object[kTransferList] === 'function' ?
        object[kTransferList]() : undefined;
    case 1:
      return object[kTransfer]();
    case 2:
      return object[kClone]();
    case 3:
      return deserialize(object, data);
    default:
      return object.constructor?.name ?? 'object';
  }
});

module.exports = {
  markTransferMode,
  markAsUncloneable,
  structuredClone,
  kClone,
  kDeserialize,
  kTransfer,
  kTransferList,
};
