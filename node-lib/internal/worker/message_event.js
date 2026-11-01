'use strict';

// The `message` and `messageerror` event of a MessagePort (global
// `MessageEvent`). Node takes it from undici; this one carries the same
// attributes.

const { ArrayFrom, ObjectFreeze } = primordials;

const { Event } = require('internal/event_target');
const { ERR_MISSING_ARGS } = require('internal/errors').codes;

class MessageEvent extends Event {
  #data;
  #origin;
  #lastEventId;
  #source;
  #ports;

  constructor(type, init = {}) {
    if (arguments.length === 0) throw new ERR_MISSING_ARGS('type');
    super(`${type}`, init);
    this.#data = init?.data === undefined ? null : init.data;
    this.#origin = init?.origin === undefined ? '' : `${init.origin}`;
    this.#lastEventId = init?.lastEventId === undefined ? '' : `${init.lastEventId}`;
    this.#source = init?.source ?? null;
    this.#ports = ObjectFreeze(init?.ports === undefined ? [] : ArrayFrom(init.ports));
  }

  get data() { return this.#data; }
  get origin() { return this.#origin; }
  get lastEventId() { return this.#lastEventId; }
  get source() { return this.#source; }
  get ports() { return this.#ports; }
}

for (const name of ['data', 'origin', 'lastEventId', 'source', 'ports']) {
  const descriptor = Object.getOwnPropertyDescriptor(MessageEvent.prototype, name);
  descriptor.enumerable = true;
  Object.defineProperty(MessageEvent.prototype, name, descriptor);
}
Object.defineProperty(MessageEvent.prototype, Symbol.toStringTag, {
  __proto__: null,
  value: 'MessageEvent',
  configurable: true,
});

module.exports = { MessageEvent };
