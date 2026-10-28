'use strict';

const {
  ArrayFrom,
  ObjectDefineProperties,
} = primordials;

const {
  codes: {
    ERR_INVALID_THIS,
  },
} = require('internal/errors');
const { validateString } = require('internal/validators');
const { kEnumerableProperty } = require('internal/util');
const {
  kHybridDispatch,
  kIsNodeEventTarget,
  kIsNodeStyleListener,
} = require('internal/events/symbols');
const EventEmitter = require('events');
const { kMaxEventTargetListeners } = EventEmitter;
const {
  EventTarget,
  initEventTarget,
  kEvents,
} = require('internal/event_target');

// NodeEventTarget lives apart from internal/event_target (the 20,000-byte file
// budget); that module re-exports it lazily.
function initNodeEventTarget(self) {
  initEventTarget(self);
}

class NodeEventTarget extends EventTarget {
  static [kIsNodeEventTarget] = true;
  static defaultMaxListeners = 10;

  constructor() {
    super();
    initNodeEventTarget(this);
  }

  /**
   * @param {number} n
   */
  setMaxListeners(n) {
    if (!isNodeEventTarget(this))
      throw new ERR_INVALID_THIS('NodeEventTarget');
    EventEmitter.setMaxListeners(n, this);
  }

  /**
   * @returns {number}
   */
  getMaxListeners() {
    if (!isNodeEventTarget(this))
      throw new ERR_INVALID_THIS('NodeEventTarget');
    return this[kMaxEventTargetListeners];
  }

  /**
   * @returns {string[]}
   */
  eventNames() {
    if (!isNodeEventTarget(this))
      throw new ERR_INVALID_THIS('NodeEventTarget');
    if (this[kEvents] === undefined)
      return [];
    return ArrayFrom(this[kEvents].keys());
  }

  /**
   * @param {string} type
   * @returns {number}
   */
  listenerCount(type) {
    if (!isNodeEventTarget(this))
      throw new ERR_INVALID_THIS('NodeEventTarget');
    if (this[kEvents] === undefined)
      return 0;
    const root = this[kEvents].get(String(type));
    return root !== undefined ? root.size : 0;
  }

  /**
   * @param {string} type
   * @param {EventTargetCallback|EventListener} listener
   * @param {{
   *   capture?: boolean,
   * }} [options]
   * @returns {NodeEventTarget}
   */
  off(type, listener, options) {
    if (!isNodeEventTarget(this))
      throw new ERR_INVALID_THIS('NodeEventTarget');
    this.removeEventListener(type, listener, options);
    return this;
  }

  /**
   * @param {string} type
   * @param {EventTargetCallback|EventListener} listener
   * @param {{
   *   capture?: boolean,
   * }} [options]
   * @returns {NodeEventTarget}
   */
  removeListener(type, listener, options) {
    if (!isNodeEventTarget(this))
      throw new ERR_INVALID_THIS('NodeEventTarget');
    this.removeEventListener(type, listener, options);
    return this;
  }

  /**
   * @param {string} type
   * @param {EventTargetCallback|EventListener} listener
   * @returns {NodeEventTarget}
   */
  on(type, listener) {
    if (!isNodeEventTarget(this))
      throw new ERR_INVALID_THIS('NodeEventTarget');
    this.addEventListener(type, listener, { [kIsNodeStyleListener]: true });
    return this;
  }

  /**
   * @param {string} type
   * @param {EventTargetCallback|EventListener} listener
   * @returns {NodeEventTarget}
   */
  addListener(type, listener) {
    if (!isNodeEventTarget(this))
      throw new ERR_INVALID_THIS('NodeEventTarget');
    this.addEventListener(type, listener, { [kIsNodeStyleListener]: true });
    return this;
  }

  /**
   * @param {string} type
   * @param {any} arg
   * @returns {boolean}
   */
  emit(type, arg) {
    if (!isNodeEventTarget(this))
      throw new ERR_INVALID_THIS('NodeEventTarget');
    validateString(type, 'type');
    const hadListeners = this.listenerCount(type) > 0;
    this[kHybridDispatch](arg, type);
    return hadListeners;
  }

  /**
   * @param {string} type
   * @param {EventTargetCallback|EventListener} listener
   * @returns {NodeEventTarget}
   */
  once(type, listener) {
    if (!isNodeEventTarget(this))
      throw new ERR_INVALID_THIS('NodeEventTarget');
    this.addEventListener(type, listener,
                          { once: true, [kIsNodeStyleListener]: true });
    return this;
  }

  /**
   * @param {string} [type]
   * @returns {NodeEventTarget}
   */
  removeAllListeners(type) {
    if (!isNodeEventTarget(this))
      throw new ERR_INVALID_THIS('NodeEventTarget');
    if (this[kEvents] === undefined)
      return this;
    if (type !== undefined)
      this[kEvents].delete(String(type));
    else
      this[kEvents].clear();

    return this;
  }
}

ObjectDefineProperties(NodeEventTarget.prototype, {
  setMaxListeners: kEnumerableProperty,
  getMaxListeners: kEnumerableProperty,
  eventNames: kEnumerableProperty,
  listenerCount: kEnumerableProperty,
  off: kEnumerableProperty,
  removeListener: kEnumerableProperty,
  on: kEnumerableProperty,
  addListener: kEnumerableProperty,
  once: kEnumerableProperty,
  emit: kEnumerableProperty,
  removeAllListeners: kEnumerableProperty,
});

function isNodeEventTarget(obj) {
  return obj?.constructor?.[kIsNodeEventTarget];
}

module.exports = {
  NodeEventTarget,
  initNodeEventTarget,
};
