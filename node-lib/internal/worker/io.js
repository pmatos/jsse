'use strict';

// MessageChannel and MessagePort for one thread (#1528). Node's are native
// (src/node_messaging.cc); here a channel is two PortData records that hold
// each other's queue. postMessage structured-clones synchronously (through the
// engine's `structuredClone`, so transferables such as web streams and other
// ports travel as in Node) and delivery is one poll-phase job per message
// (`loopDefer`), so every message sees its own tick and microtask checkpoint
// and runs before the check phase, as in Node. A queued message keeps the loop
// alive even for an unref'd port; Node would let such a port miss a message
// sent as the process exits.
//
// A port keeps the loop alive while it is open and ref'd (`ref()`, or a
// `message` listener), as in Node. A new or transferred port starts unref'd.

const {
  ArrayFrom,
  ArrayPrototypeShift,
  MathMax,
  ObjectDefineProperties,
  Symbol,
  SymbolFor,
  SymbolIterator,
} = primordials;

const { Event, NodeEventTarget, defineEventHandler, kCreateEvent, kNewListener,
        kRemoveListener } = require('internal/event_target');
const { kEnumerableProperty } = require('internal/util');
const { kHybridDispatch } = require('internal/events/symbols');
const { DOMException } = require('internal/per_context/domexception');
const { MessageEvent } = require('internal/worker/message_event');
const { setImmediate } = require('timers');
const { codes: { ERR_INVALID_ARG_TYPE, ERR_MISSING_ARGS } } = require('internal/errors');
const { kDeserialize, kTransfer, markAsUncloneable, markTransferMode, structuredClone } =
  require('internal/worker/js_transferable');

// Without the process floor a port never holds the loop open and a message
// takes an Immediate.
let loopRef = () => {};
let loopDefer = (callback) => setImmediate(callback);
try {
  ({ loopRef, loopDefer } = require('internal/host/child_process'));
} catch {
  // Floor absent.
}

const kMessagesPerTick = 1000;
const kCreate = Symbol('kCreate');
const kCurrentlyReceivingPorts = SymbolFor('nodejs.internal.kCurrentlyReceivingPorts');
const CLOSE = Symbol('close message');

class PortData {
  peer = null;
  owner = null;
  queue = [];
  receiving = false;
  refed = false;
  counted = false;
  closed = false;
  scheduled = false;
  processed = 0;
  limit = 0;
}

function isObjectLike(value) {
  return (typeof value === 'object' && value !== null) || typeof value === 'function';
}

function syncRef(data) {
  const want = data.refed && !data.closed && data.owner !== null;
  if (want === data.counted) return;
  data.counted = want;
  loopRef(want ? 1 : -1);
}

// A port delivers what is queued for it in one batch, as Node does: the
// first message is a poll-phase job, the following ones run in front of the
// other ports' jobs, up to max(1000, the queue at the start); the rest waits
// for the next turn.
function schedule(data, continuing = false) {
  if (data.scheduled || data.owner === null || data.closed) return;
  const next = data.queue[0];
  if (next === undefined || (next !== CLOSE && !data.receiving)) return;
  data.scheduled = true;
  if (!continuing) {
    data.processed = 0;
    data.limit = MathMax(kMessagesPerTick, data.queue.length);
  }
  loopDefer(() => deliver(data), continuing);
}

function deliver(data) {
  data.scheduled = false;
  if (data.closed || data.owner === null) return;
  const next = data.queue[0];
  if (next === CLOSE) {
    ArrayPrototypeShift(data.queue);
    closeData(data);
    return;
  }
  if (next === undefined || !data.receiving) return;
  ArrayPrototypeShift(data.queue);
  const port = data.owner;
  port[kCurrentlyReceivingPorts] = next.ports;
  try {
    port[kHybridDispatch](next.value, 'message', undefined);
  } finally {
    port[kCurrentlyReceivingPorts] = undefined;
  }
  data.processed++;
  schedule(data, data.processed < data.limit);
}

// Close one end: its queue is dropped, the other end is told after the
// messages already sent to it, and 'close' is emitted on a later turn.
function closeData(data) {
  if (data.closed) return;
  data.closed = true;
  data.queue.length = 0;
  const { peer, owner } = data;
  data.peer = null;
  syncRef(data);
  if (peer !== null && !peer.closed) {
    peer.peer = null;
    peer.queue.push(CLOSE);
    schedule(peer);
  }
  if (owner !== null) setImmediate(() => owner.dispatchEvent(new Event('close')));
}

function notIterable(what) {
  const error = new TypeError(`Optional ${what} argument must be an iterable`);
  error.code = 'ERR_INVALID_ARG_TYPE';
  return error;
}

function transferListOf(options) {
  if (options === undefined || options === null) return [];
  if (!isObjectLike(options)) throw notIterable('transferList');
  if (typeof options[SymbolIterator] === 'function') return ArrayFrom(options);
  const { transfer } = options;
  if (transfer === undefined) return [];
  if (!isObjectLike(transfer) || typeof transfer[SymbolIterator] !== 'function') {
    throw notIterable('options.transfer');
  }
  return ArrayFrom(transfer);
}

const transit = new Map();
let nextHandle = 1;
let dataOf;

class MessagePort extends NodeEventTarget {
  #data;

  static {
    dataOf = (port) => port.#data;
  }

  constructor(create) {
    if (create !== kCreate) {
      const error = new TypeError('Constructor cannot be called');
      error.code = 'ERR_CONSTRUCT_CALL_INVALID';
      throw error;
    }
    super();
    markTransferMode(this, false, true);
    this.#data = new PortData();
    this.#data.owner = this;
    this[kCurrentlyReceivingPorts] = undefined;
  }

  postMessage(value) {
    if (arguments.length === 0) throw new ERR_MISSING_ARGS('value');
    const transfer = transferListOf(arguments[1]);
    const data = this.#data;
    if (data === undefined || data.closed || data.peer === null) return;
    const ports = [];
    for (const item of transfer) {
      if (item === this) {
        throw new DOMException('Transfer list contains source port', 'DataCloneError');
      }
      if (item instanceof MessagePort) ports.push(item);
    }
    const message = structuredClone({ value, ports }, { transfer });
    const peer = data.peer;
    peer.queue.push(message);
    schedule(peer);
  }

  start() {
    const data = this.#data;
    if (data === undefined || data.closed) return;
    data.receiving = true;
    schedule(data);
  }

  close(callback) {
    if (typeof callback === 'function') this.once('close', callback);
    if (this.#data !== undefined) closeData(this.#data);
  }

  ref() {
    const data = this.#data;
    if (data === undefined) return;
    data.refed = true;
    syncRef(data);
  }

  unref() {
    const data = this.#data;
    if (data === undefined) return;
    data.refed = false;
    syncRef(data);
  }

  hasRef() {
    const data = this.#data;
    return data !== undefined && !data.closed && data.refed;
  }

  [kNewListener](size, type, ...rest) {
    super[kNewListener](size, type, ...rest);
    if (type === 'message' && size === 1) {
      this.ref();
      this.start();
    }
  }

  [kRemoveListener](size, type, ...rest) {
    if (type === 'message' && size === 0) {
      const data = this.#data;
      if (data !== undefined) data.receiving = false;
      this.unref();
    }
    super[kRemoveListener](size, type, ...rest);
  }

  [kCreateEvent](value, type) {
    if (type !== 'message' && type !== 'messageerror') {
      return super[kCreateEvent](value, type);
    }
    const ports = this[kCurrentlyReceivingPorts];
    this[kCurrentlyReceivingPorts] = undefined;
    return new MessageEvent(type, { data: value, ports });
  }

  [kTransfer]() {
    const data = this.#data;
    if (data === undefined || data.closed) {
      throw new DOMException('MessagePort in transfer list is already detached',
                             'DataCloneError');
    }
    this.#data = undefined;
    data.owner = null;
    data.receiving = false;
    data.refed = false;
    syncRef(data);
    const handle = nextHandle++;
    transit.set(handle, data);
    return {
      data: { handle },
      deserializeInfo: 'internal/worker/io:TransferredMessagePort',
    };
  }

  [kDeserialize]({ handle }) {
    const data = transit.get(handle);
    transit.delete(handle);
    this.#data = data;
    data.owner = this;
    schedule(data);
  }
}

ObjectDefineProperties(MessagePort.prototype, {
  postMessage: kEnumerableProperty,
  start: kEnumerableProperty,
  close: kEnumerableProperty,
});
defineEventHandler(MessagePort.prototype, 'message');
defineEventHandler(MessagePort.prototype, 'messageerror');

// `new Ctor()` of the structured-clone deserializer.
function TransferredMessagePort() {
  return new MessagePort(kCreate);
}
TransferredMessagePort.prototype[kDeserialize] = () => {};

class MessageChannel {
  constructor() {
    const port1 = new MessagePort(kCreate);
    const port2 = new MessagePort(kCreate);
    const a = dataOf(port1);
    const b = dataOf(port2);
    a.peer = b;
    b.peer = a;
    this.port1 = port1;
    this.port2 = port2;
  }
}

function receiveMessageOnPort(port) {
  if (!(port instanceof MessagePort)) {
    throw new ERR_INVALID_ARG_TYPE('port', 'MessagePort', port);
  }
  const data = dataOf(port);
  const next = data?.queue[0];
  if (next === undefined || next === CLOSE) return undefined;
  ArrayPrototypeShift(data.queue);
  return { message: next.value };
}

module.exports = {
  MessageChannel,
  MessagePort,
  MessageEvent,
  TransferredMessagePort,
  receiveMessageOnPort,
  markAsUncloneable,
};
