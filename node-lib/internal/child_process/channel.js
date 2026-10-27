'use strict';

// The IPC channel of Node's lib/internal/child_process.js (MIT): `setupChannel`
// gives a ChildProcess (the parent end) or `process` (the child end) `send`,
// `disconnect`, `connected` and `channel`, over an IpcPipe. Messages are
// newline-delimited JSON, byte for byte what Node writes, or length-framed
// 'advanced' frames (internal/child_process/serialization); the native channel
// does the framing, so `send` hands over text and `onread` gets texts. Handle
// passing is not supported: a handle argument is ERR_INVALID_HANDLE_TYPE.

const {
  ArrayPrototypePush,
  ArrayPrototypeShift,
  JSONParse,
  JSONStringify,
  ObjectDefineProperty,
  StringPrototypeStartsWith,
} = primordials;

const EventEmitter = require('events');
const {
  codes: {
    ERR_INVALID_ARG_TYPE,
    ERR_INVALID_HANDLE_TYPE,
    ERR_IPC_CHANNEL_CLOSED,
    ERR_IPC_DISCONNECTED,
    ERR_MISSING_ARGS,
  },
} = require('internal/errors');
const { validateObject } = require('internal/validators');
const { ErrnoException } = require('internal/child_process/support');
const { deserialize, serialize } = require('internal/child_process/serialization');

const kPendingMessages = Symbol('kPendingMessages');
const kChannelHandle = Symbol('kChannelHandle');
const kFlushPending = Symbol('kFlushPending');

// Counted refs keep the channel alive for each message listener and write in
// flight, until the user takes over with an explicit ref() or unref().
class Control extends EventEmitter {
  #channel;
  #refs = 0;
  #refExplicitlySet = false;

  constructor(channel) {
    super();
    this.#channel = channel;
    this[kPendingMessages] = [];
  }

  refCounted() {
    if (++this.#refs === 1 && !this.#refExplicitlySet) this.#channel.ref();
  }

  unrefCounted() {
    if (--this.#refs === 0 && !this.#refExplicitlySet) {
      this.#channel.unref();
      this.emit('unref');
    }
  }

  ref() {
    this.#refExplicitlySet = true;
    this.#channel.ref();
  }

  unref() {
    this.#refExplicitlySet = true;
    this.#channel.unref();
    this.emit('unref');
  }

  get fd() {
    return this.#channel ? this.#channel.fd : undefined;
  }
}

function isInternal(message) {
  return message !== null && typeof message === 'object' &&
    typeof message.cmd === 'string' && StringPrototypeStartsWith(message.cmd, 'NODE_');
}

function maybeClose(subprocess) {
  subprocess._closesGot++;
  if (subprocess._closesGot === subprocess._closesNeeded) {
    subprocess.emit('close', subprocess.exitCode, subprocess.signalCode);
  }
}

function nop() {}

function setupChannel(target, channel, serializationMode) {
  const control = new Control(channel);
  target.channel = control;
  target[kChannelHandle] = channel;
  ObjectDefineProperty(target, '_channel', {
    __proto__: null,
    get() { return target.channel; },
    set(value) { target.channel = value; },
    configurable: true,
    enumerable: false,
  });

  target._handleQueue = null;
  target._pendingMessage = null;

  const parse = serializationMode === 'advanced' ? deserialize : JSONParse;

  function flushPending() {
    if (!target.channel || target.listenerCount('message') === 0) return;
    const pending = target.channel[kPendingMessages];
    while (pending.length > 0 && target.listenerCount('message') > 0) {
      const [event, message, handle] = ArrayPrototypeShift(pending);
      target.emit(event, message, handle);
    }
    if (pending.length === 0) target.removeListener('newListener', onNewListener);
  }

  function onNewListener(name) {
    if (name === 'message') process.nextTick(flushPending);
  }

  function emitMessage(event, message, handle) {
    if (event === 'internalMessage' || target.listenerCount('message') > 0) {
      target.emit(event, message, handle);
    } else if (target.channel) {
      if (target.channel[kPendingMessages].length === 0) target.on('newListener', onNewListener);
      ArrayPrototypePush(target.channel[kPendingMessages], [event, message, handle]);
    }
  }

  function handleMessage(message, handle, internal) {
    if (!target.channel) return;
    process.nextTick(emitMessage, internal ? 'internalMessage' : 'message', message, handle);
  }

  channel.onread = function(items) {
    if (items) {
      for (let i = 0; i < items.length; i++) {
        const message = parse(items[i]);
        handleMessage(message, undefined, isInternal(message));
      }
    } else {
      target.disconnect();
      channel.onread = nop;
      channel.close();
      target[kChannelHandle] = null;
      maybeClose(target);
    }
  };

  target.send = function(message, handle, options, callback) {
    if (typeof handle === 'function') {
      callback = handle;
      handle = undefined;
      options = undefined;
    } else if (typeof options === 'function') {
      callback = options;
      options = undefined;
    } else if (options !== undefined) {
      validateObject(options, 'options');
    }

    options = { swallowErrors: false, ...options };

    if (this.connected) {
      return this._send(message, handle, options, callback);
    }
    const ex = new ERR_IPC_CHANNEL_CLOSED();
    if (typeof callback === 'function') {
      process.nextTick(callback, ex);
    } else {
      process.nextTick(() => this.emit('error', ex));
    }
    return false;
  };

  target._send = function(message, handle, options, callback) {
    if (message === undefined) throw new ERR_MISSING_ARGS('message');

    if (typeof message !== 'string' && typeof message !== 'object' &&
        typeof message !== 'number' && typeof message !== 'boolean') {
      throw new ERR_INVALID_ARG_TYPE('message', ['string', 'object', 'number', 'boolean'], message);
    }

    if (typeof options === 'boolean') options = { swallowErrors: options };
    if (handle) throw new ERR_INVALID_HANDLE_TYPE();

    const text = serializationMode === 'advanced' ? serialize(message) : JSONStringify(message);
    const err = channel.write(text, (status) => {
      control.unrefCounted();
      if (typeof callback === 'function') callback(status === 0 ? null : new ErrnoException(status, 'write'));
    });

    if (err === 0) {
      if (channel.lastWriteWasAsync) {
        control.refCounted();
      } else if (typeof callback === 'function') {
        process.nextTick(callback, null);
      }
    } else if (!options.swallowErrors) {
      const ex = new ErrnoException(err, 'write');
      if (typeof callback === 'function') {
        process.nextTick(callback, ex);
      } else {
        process.nextTick(() => this.emit('error', ex));
      }
    }

    return channel.writeQueueSize < (65536 * 2);
  };

  target.connected = true;

  target.disconnect = function() {
    if (!this.connected) {
      this.emit('error', new ERR_IPC_DISCONNECTED());
      return;
    }
    this.connected = false;
    if (!this._handleQueue) this._disconnect();
  };

  target._disconnect = function() {
    this.channel = null;
    this[kChannelHandle] = null;

    let fired = false;
    function finish() {
      if (fired) return;
      fired = true;
      channel.close();
      target.emit('disconnect');
    }

    process.nextTick(finish);
  };

  control[kFlushPending] = flushPending;
  channel.readStart();
  return control;
}

const EMITTER_METHODS = [
  'on', 'addListener', 'once', 'prependListener', 'prependOnceListener',
  'off', 'removeListener', 'removeAllListeners', 'emit',
];

// In Node the child's channel is ref'd while `process` has a 'message' or
// 'disconnect' listener, tracked through 'newListener'/'removeListener'.
// jsse's `process` emitter has neither event, so the emitter methods are
// wrapped to compare the listener counts after each call instead; the same
// wrappers deliver messages that arrived before the first 'message' listener.
function watchProcessListeners(proc, control) {
  let held = false;
  function sync() {
    const want = proc.listenerCount('message') > 0 || proc.listenerCount('disconnect') > 0;
    if (want === held) return;
    held = want;
    if (want) control.refCounted();
    else control.unrefCounted();
  }
  for (const name of EMITTER_METHODS) {
    const method = proc[name];
    ObjectDefineProperty(proc, name, {
      __proto__: null,
      value: function(type, ...rest) {
        const result = method.call(this, type, ...rest);
        if (type === 'message' || type === 'disconnect' || name === 'removeAllListeners') sync();
        if (type === 'message' && name !== 'emit') process.nextTick(control[kFlushPending]);
        return result;
      },
      writable: true,
      enumerable: false,
      configurable: true,
    });
  }
}

module.exports = { setupChannel, watchProcessListeners };
