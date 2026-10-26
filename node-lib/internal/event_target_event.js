'use strict';

const {
  ArrayPrototypeReduce,
  NumberIsInteger,
  ObjectAssign,
  ObjectDefineProperties,
  ObjectGetOwnPropertyDescriptor,
  Symbol,
  SymbolToStringTag,
  WeakSet: SafeWeakSet,
} = primordials;

const {
  codes: {
    ERR_INVALID_THIS,
    ERR_MISSING_ARGS,
  },
} = require('internal/errors');
const {
  validateObject,
} = require('internal/validators');
const {
  customInspectSymbol,
  kEmptyObject,
  kEnumerableProperty,
} = require('internal/util');
const { inspect } = require('internal/util/inspect');
const { now } = require('internal/perf/utils');

const kIsBeingDispatched = Symbol('kIsBeingDispatched');
const kStop = Symbol('kStop');
const kTarget = Symbol('kTarget');
const kTrustEvent = Symbol('kTrustEvent');
const kType = Symbol('type');
const kDetail = Symbol('detail');
const kInPassiveListener = Symbol('kInPassiveListener');

const isTrustedSet = new SafeWeakSet();
const isTrusted = ObjectGetOwnPropertyDescriptor({
  get isTrusted() {
    return isTrustedSet.has(this);
  },
}, 'isTrusted').get;

const isTrustedDescriptor = {
  __proto__: null,
  configurable: false,
  enumerable: true,
  get: isTrusted,
};

function isEvent(value) {
  return typeof value?.[kType] === 'string';
}

class Event {
  #cancelable = false;
  #bubbles = false;
  #composed = false;
  #defaultPrevented = false;
  #timestamp = now();
  #propagationStopped = false;

  constructor(type, options = undefined) {
    if (arguments.length === 0)
      throw new ERR_MISSING_ARGS('type');
    if (options != null)
      validateObject(options, 'options');
    this.#bubbles = !!options?.bubbles;
    this.#cancelable = !!options?.cancelable;
    this.#composed = !!options?.composed;

    this[kType] = `${type}`;
    if (options?.[kTrustEvent]) {
      isTrustedSet.add(this);
    }

    this[kTarget] = null;
    this[kIsBeingDispatched] = false;
    this[kInPassiveListener] = false;
  }

  initEvent(type, bubbles = false, cancelable = false) {
    if (arguments.length === 0)
      throw new ERR_MISSING_ARGS('type');

    if (this[kIsBeingDispatched]) {
      return;
    }
    this[kType] = `${type}`;
    this.#bubbles = !!bubbles;
    this.#cancelable = !!cancelable;
  }

  [customInspectSymbol](depth, options) {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');
    const name = this.constructor.name;
    if (depth < 0)
      return name;

    const opts = ObjectAssign({}, options, {
      depth: NumberIsInteger(options.depth) ? options.depth - 1 : options.depth,
    });

    return `${name} ${inspect({
      type: this[kType],
      defaultPrevented: this.#defaultPrevented,
      cancelable: this.#cancelable,
      timeStamp: this.#timestamp,
    }, opts)}`;
  }

  stopImmediatePropagation() {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');
    // Spec mention "stopImmediatePropagation should set both "stop propagation"
    // and "stop immediate propagation" flags"
    // cf: from https://dom.spec.whatwg.org/#dom-event-stopimmediatepropagation
    this.stopPropagation();
    this[kStop] = true;
  }

  preventDefault() {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');
    if (!this.#cancelable || this[kInPassiveListener]) return;
    this.#defaultPrevented = true;
  }

  get target() {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');
    return this[kTarget];
  }

  get currentTarget() {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');
    return this[kIsBeingDispatched] ? this[kTarget] : null;
  }

  get srcElement() {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');
    return this[kTarget];
  }

  get type() {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');
    return this[kType];
  }

  get cancelable() {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');
    return this.#cancelable;
  }

  get defaultPrevented() {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');
    return this.#cancelable && this.#defaultPrevented;
  }

  get timeStamp() {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');
    return this.#timestamp;
  }


  // The following are non-op and unused properties/methods from Web API Event.
  // These are not supported in Node.js and are provided purely for
  // API completeness.
  composedPath() {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');
    return this[kIsBeingDispatched] ? [this[kTarget]] : [];
  }

  get returnValue() {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');
    return !this.#cancelable || !this.#defaultPrevented;
  }

  set returnValue(value) {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');

    if (!value) {
      if (!this.#cancelable || this[kInPassiveListener]) return;
      this.#defaultPrevented = true;
    }
  }

  get bubbles() {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');
    return this.#bubbles;
  }

  get composed() {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');
    return this.#composed;
  }

  get eventPhase() {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');
    return this[kIsBeingDispatched] ? Event.AT_TARGET : Event.NONE;
  }

  get cancelBubble() {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');
    return this.#propagationStopped;
  }

  set cancelBubble(value) {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');
    if (value) {
      this.#propagationStopped = true;
    }
  }

  stopPropagation() {
    if (!isEvent(this))
      throw new ERR_INVALID_THIS('Event');
    this.#propagationStopped = true;
  }
}

ObjectDefineProperties(
  Event.prototype, {
    [SymbolToStringTag]: {
      __proto__: null,
      writable: false,
      enumerable: false,
      configurable: true,
      value: 'Event',
    },
    initEvent: kEnumerableProperty,
    stopImmediatePropagation: kEnumerableProperty,
    preventDefault: kEnumerableProperty,
    target: kEnumerableProperty,
    currentTarget: kEnumerableProperty,
    srcElement: kEnumerableProperty,
    type: kEnumerableProperty,
    cancelable: kEnumerableProperty,
    defaultPrevented: kEnumerableProperty,
    timeStamp: kEnumerableProperty,
    composedPath: kEnumerableProperty,
    returnValue: kEnumerableProperty,
    bubbles: kEnumerableProperty,
    composed: kEnumerableProperty,
    eventPhase: kEnumerableProperty,
    cancelBubble: kEnumerableProperty,
    stopPropagation: kEnumerableProperty,
    // Don't conform to the spec with isTrusted. The spec defines it as
    // LegacyUnforgeable but defining it in the constructor has a big
    // performance impact and the property doesn't seem to be useful outside of
    // browsers.
    isTrusted: isTrustedDescriptor,
  });

const staticProps = ['NONE', 'CAPTURING_PHASE', 'AT_TARGET', 'BUBBLING_PHASE'];

ObjectDefineProperties(
  Event,
  ArrayPrototypeReduce(staticProps, (result, staticProp, index = 0) => {
    result[staticProp] = {
      __proto__: null,
      writable: false,
      configurable: false,
      enumerable: true,
      value: index,
    };
    return result;
  }, {}),
);

function isCustomEvent(value) {
  return isEvent(value) && (value?.[kDetail] !== undefined);
}

class CustomEvent extends Event {
  constructor(type, options = kEmptyObject) {
    if (arguments.length === 0)
      throw new ERR_MISSING_ARGS('type');
    super(type, options);
    this[kDetail] = options?.detail ?? null;
  }

  get detail() {
    if (!isCustomEvent(this))
      throw new ERR_INVALID_THIS('CustomEvent');
    return this[kDetail];
  }
}

ObjectDefineProperties(CustomEvent.prototype, {
  [SymbolToStringTag]: {
    __proto__: null,
    writable: false,
    enumerable: false,
    configurable: true,
    value: 'CustomEvent',
  },
  detail: kEnumerableProperty,
});

module.exports = {
  CustomEvent,
  Event,
  isEvent,
  kInPassiveListener,
  kIsBeingDispatched,
  kStop,
  kTarget,
  kTrustEvent,
  kType,
};
