'use strict';

const {
  Boolean,
  Error,
  FunctionPrototypeCall,
  ObjectDefineProperties,
  ObjectDefineProperty,
  ReflectApply,
  Map: SafeMap,
  WeakMap: SafeWeakMap,
  WeakRef: SafeWeakRef,
  Symbol,
  SymbolFor,
  SymbolToStringTag,
} = primordials;
const SafeFinalizationRegistry = FinalizationRegistry;

const {
  codes: {
    ERR_EVENT_RECURSION,
    ERR_INVALID_ARG_TYPE,
    ERR_INVALID_THIS,
    ERR_MISSING_ARGS,
  },
} = require('internal/errors');
const {
  validateAbortSignal,
  validateObject,
  validateThisInternalField,
  kValidateObjectAllowObjects,
} = require('internal/validators');
const {
  customInspectSymbol,
  kEmptyObject,
  kEnumerableProperty,
} = require('internal/util');
const { inspect } = require('internal/util/inspect');
const webidl = require('internal/webidl');
const { now } = require('internal/perf/utils');
const { emitWarning } = require('internal/process/warning');
const {
  CustomEvent,
  Event,
  kInPassiveListener,
  kIsBeingDispatched,
  kStop,
  kTarget,
  kTrustEvent,
} = require('internal/event_target_event');

const kIsEventTarget = SymbolFor('nodejs.event_target');

const EventEmitter = require('events');
const {
  kMaxEventTargetListeners,
  kMaxEventTargetListenersWarned,
} = EventEmitter;

const kEvents = Symbol('kEvents');
const kHandlers = Symbol('kHandlers');
const kWeakHandler = Symbol('kWeak');
const kResistStopPropagation = Symbol('kResistStopPropagation');

const kHybridDispatch = SymbolFor('nodejs.internal.kHybridDispatch');
const kRemoveWeakListenerHelper = Symbol('kRemoveWeakListenerHelper');
const kCreateEvent = Symbol('kCreateEvent');
const kNewListener = Symbol('kNewListener');
const kRemoveListener = Symbol('kRemoveListener');
const kIsNodeStyleListener = Symbol('kIsNodeStyleListener');

// Weak listener cleanup
// This has to be lazy for snapshots to work
let weakListenersState = null;
// The resource needs to retain the callback so that it doesn't
// get garbage collected now that it's weak.
let objectToWeakListenerMap = null;
function weakListeners() {
  weakListenersState ??= new SafeFinalizationRegistry(
    ({ eventTarget, listener, eventType }) => eventTarget.deref()?.[kRemoveWeakListenerHelper](eventType, listener),
  );
  objectToWeakListenerMap ??= new SafeWeakMap();
  return { registry: weakListenersState, map: objectToWeakListenerMap };
}

const kFlagOnce = 1 << 0;
const kFlagCapture = 1 << 1;
const kFlagPassive = 1 << 2;
const kFlagNodeStyle = 1 << 3;
const kFlagWeak = 1 << 4;
const kFlagRemoved = 1 << 5;
const kFlagResistStopPropagation = 1 << 6;

// The listeners for an EventTarget are maintained as a linked list.
// Unfortunately, the way EventTarget is defined, listeners are accounted
// using the tuple [handler,capture], and even if we don't actually make
// use of capture or bubbling, in order to be spec compliant we have to
// take on the additional complexity of supporting it. Fortunately, using
// the linked list makes dispatching faster, even if adding/removing is
// slower.
class Listener {
  constructor(eventTarget, eventType, previous, listener, once, capture, passive,
              isNodeStyleListener, weak, resistStopPropagation) {
    this.next = undefined;
    if (previous !== undefined)
      previous.next = this;
    this.previous = previous;
    this.listener = listener;

    let flags = 0b0;
    if (once)
      flags |= kFlagOnce;
    if (capture)
      flags |= kFlagCapture;
    if (passive)
      flags |= kFlagPassive;
    if (isNodeStyleListener)
      flags |= kFlagNodeStyle;
    if (weak)
      flags |= kFlagWeak;
    if (resistStopPropagation)
      flags |= kFlagResistStopPropagation;
    this.flags = flags;

    this.removed = false;

    if (this.weak) {
      this.callback = new SafeWeakRef(listener);
      weakListeners().registry.register(listener, {
        __proto__: null,
        // Weak ref so the listener won't hold the eventTarget alive
        eventTarget: new SafeWeakRef(eventTarget),
        listener: this,
        eventType,
      }, this);
      // Make the retainer retain the listener in a WeakMap
      weakListeners().map.set(weak, listener);
      this.listener = this.callback;
    } else if (typeof listener === 'function') {
      this.callback = listener;
      this.listener = listener;
    } else {
      this.callback = async (...args) => {
        if (listener.handleEvent)
          await ReflectApply(listener.handleEvent, listener, args);
      };
      this.listener = listener;
    }
  }

  get once() {
    return Boolean(this.flags & kFlagOnce);
  }
  get capture() {
    return Boolean(this.flags & kFlagCapture);
  }
  get passive() {
    return Boolean(this.flags & kFlagPassive);
  }
  get isNodeStyleListener() {
    return Boolean(this.flags & kFlagNodeStyle);
  }
  get weak() {
    return Boolean(this.flags & kFlagWeak);
  }
  get resistStopPropagation() {
    return Boolean(this.flags & kFlagResistStopPropagation);
  }
  get removed() {
    return Boolean(this.flags & kFlagRemoved);
  }
  set removed(value) {
    if (value)
      this.flags |= kFlagRemoved;
    else
      this.flags &= ~kFlagRemoved;
  }

  same(listener, capture) {
    const myListener = this.weak ? this.listener.deref() : this.listener;
    return myListener === listener && this.capture === capture;
  }

  remove() {
    if (this.previous !== undefined)
      this.previous.next = this.next;
    if (this.next !== undefined)
      this.next.previous = this.previous;
    this.removed = true;
    if (this.weak)
      weakListeners().registry.unregister(this);
  }
}

function initEventTarget(self) {
  self[kEvents] = undefined;
  self[kMaxEventTargetListeners] = EventEmitter.defaultMaxListeners;
  self[kMaxEventTargetListenersWarned] = false;
  self[kHandlers] = undefined;
}

function getEventsMap(self) {
  if (self[kEvents] === undefined)
    self[kEvents] = new SafeMap();
  return self[kEvents];
}

class EventTarget {
  // Used in checking whether an object is an EventTarget. This is a well-known
  // symbol as EventTarget may be used cross-realm.
  // Ref: https://github.com/nodejs/node/pull/33661
  static [kIsEventTarget] = true;

  constructor() {
    initEventTarget(this);
  }

  [kNewListener](size, type, listener, once, capture, passive, weak) {
    if (this[kMaxEventTargetListeners] > 0 &&
        size > this[kMaxEventTargetListeners] &&
        !this[kMaxEventTargetListenersWarned]) {
      this[kMaxEventTargetListenersWarned] = true;
      // No error code for this since it is a Warning
      // eslint-disable-next-line no-restricted-syntax
      const w = new Error('Possible EventTarget memory leak detected. ' +
                          `${size} ${type} listeners ` +
                          `added to ${inspect(this, { depth: -1 })}. MaxListeners is ${this[kMaxEventTargetListeners]}. Use ` +
                          'events.setMaxListeners() to increase limit');
      w.name = 'MaxListenersExceededWarning';
      w.target = this;
      w.type = type;
      w.count = size;
      emitWarning(w);
    }
  }
  [kRemoveListener](size, type, listener, capture) {}



  addEventListener(type, listener, options = kEmptyObject) {
    if (!isEventTarget(this))
      throw new ERR_INVALID_THIS('EventTarget');
    if (arguments.length < 2)
      throw new ERR_MISSING_ARGS('type', 'listener');

    let once = false;
    let capture = false;
    let passive = false;
    let isNodeStyleListener = false;
    let weak = false;
    let resistStopPropagation = false;

    if (options !== kEmptyObject) {
      // We validateOptions before the validateListener check because the spec
      // requires us to hit getters.
      options = validateEventListenerOptions(options);

      once = options.once;
      capture = options.capture;
      passive = options.passive;
      isNodeStyleListener = options.isNodeStyleListener;
      weak = options.weak;
      resistStopPropagation = options.resistStopPropagation;

      const signal = options.signal;

      validateAbortSignal(signal, 'options.signal');

      if (signal) {
        if (signal.aborted) {
          return;
        }
        signal.addEventListener('abort', () => {
          this.removeEventListener(type, listener, options);
        }, { __proto__: null, once: true, [kWeakHandler]: this, [kResistStopPropagation]: true });
      }
    }

    if (!validateEventListener(listener)) {
      // The DOM silently allows passing undefined as a second argument
      // No error code for this since it is a Warning
      // eslint-disable-next-line no-restricted-syntax
      const w = new Error(`addEventListener called with ${listener}` +
                          ' which has no effect.');
      w.name = 'AddEventListenerArgumentTypeWarning';
      w.target = this;
      w.type = type;
      emitWarning(w);
      return;
    }

    type = webidl.converters.DOMString(type);

    const eventsMap = getEventsMap(this);
    let root = eventsMap.get(type);

    if (root === undefined) {
      root = { size: 1, next: undefined, resistStopPropagation: Boolean(resistStopPropagation) };
      // This is the first handler in our linked list.
      new Listener(this, type, root, listener, once, capture, passive,
                   isNodeStyleListener, weak, resistStopPropagation);
      this[kNewListener](
        root.size,
        type,
        listener,
        once,
        capture,
        passive,
        weak);
      eventsMap.set(type, root);
      return;
    }

    let handler = root.next;
    let previous = root;

    // We have to walk the linked list to see if we have a match
    while (handler !== undefined && !handler.same(listener, capture)) {
      previous = handler;
      handler = handler.next;
    }

    if (handler !== undefined) { // Duplicate! Ignore
      return;
    }

    new Listener(this, type, previous, listener, once, capture, passive,
                 isNodeStyleListener, weak, resistStopPropagation);
    root.size++;
    root.resistStopPropagation ||= Boolean(resistStopPropagation);
    this[kNewListener](root.size, type, listener, once, capture, passive, weak);
  }

  removeEventListener(type, listener, options = kEmptyObject) {
    if (!isEventTarget(this))
      throw new ERR_INVALID_THIS('EventTarget');
    if (arguments.length < 2)
      throw new ERR_MISSING_ARGS('type', 'listener');
    if (!validateEventListener(listener))
      return;

    type = webidl.converters.DOMString(type);
    const capture = options?.capture === true;

    if (this[kEvents] === undefined)
      return;

    const root = this[kEvents].get(type);
    if (root === undefined || root.next === undefined)
      return;

    let handler = root.next;
    while (handler !== undefined) {
      if (handler.same(listener, capture)) {
        handler.remove();
        root.size--;
        if (root.size === 0)
          this[kEvents].delete(type);
        this[kRemoveListener](root.size, type, listener, capture);
        break;
      }
      handler = handler.next;
    }
  }

  [kRemoveWeakListenerHelper](type, listener) {
    if (this[kEvents] === undefined)
      return;

    const root = this[kEvents].get(type);
    if (root === undefined || root.next === undefined)
      return;

    const capture = listener.capture === true;

    let handler = root.next;
    while (handler !== undefined) {
      if (handler === listener) {
        handler.remove();
        root.size--;
        if (root.size === 0)
          this[kEvents].delete(type);
        // Undefined is passed as the listener as the listener was GCed
        this[kRemoveListener](root.size, type, undefined, capture);
        break;
      }
      handler = handler.next;
    }
  }

  dispatchEvent(event) {
    if (!isEventTarget(this))
      throw new ERR_INVALID_THIS('EventTarget');
    if (arguments.length < 1)
      throw new ERR_MISSING_ARGS('event');

    if (!(event instanceof Event))
      throw new ERR_INVALID_ARG_TYPE('event', 'Event', event);

    if (event[kIsBeingDispatched])
      throw new ERR_EVENT_RECURSION(event.type);

    this[kHybridDispatch](event, event.type, event);
    return event.defaultPrevented !== true;
  }

  [kHybridDispatch](nodeValue, type, event) {
    if (event !== undefined) {
      event[kTarget] = this;
      event[kIsBeingDispatched] = true;
    }

    const root = this[kEvents]?.get(type);
    if (root === undefined || root.next === undefined) {
      if (event !== undefined)
        event[kIsBeingDispatched] = false;
      return true;
    }

    let handler = root.next;
    let next;

    while (handler !== undefined &&
           (root.resistStopPropagation || handler.passive || event?.[kStop] !== true)) {
      // Cache the next item in case this iteration removes the current one
      next = handler.next;

      if (handler.removed || (event?.[kStop] === true && !handler.resistStopPropagation)) {
        // Deal with the case an event is removed while event handlers are
        // Being processed (removeEventListener called from a listener)
        // And the case of event.stopImmediatePropagation() being called
        // For events not flagged as resistStopPropagation
        handler = next;
        continue;
      }
      if (handler.once) {
        handler.remove();
        root.size--;
        const { listener, capture } = handler;
        this[kRemoveListener](root.size, type, listener, capture);
      }

      let arg;
      try {
        if (handler.isNodeStyleListener) {
          arg = nodeValue;
        } else {
          if (event === undefined) {
            event = this[kCreateEvent](nodeValue, type);
            event[kTarget] = this;
            event[kIsBeingDispatched] = true;
          }
          arg = event;
        }
        const callback = handler.weak ?
          handler.callback.deref() : handler.callback;
        let result;
        if (callback) {
          if (handler.passive && !handler.isNodeStyleListener) {
            arg[kInPassiveListener] = true;
          }
          result = FunctionPrototypeCall(callback, this, arg);
          if (!handler.isNodeStyleListener) {
            arg[kIsBeingDispatched] = false;
          }
        }
        if (result !== undefined && result !== null)
          addCatch(result);
      } catch (err) {
        emitUncaughtException(err);
      } finally {
        if (arg?.[kInPassiveListener])
          arg[kInPassiveListener] = false;
      }

      handler = next;
    }

    if (event !== undefined)
      event[kIsBeingDispatched] = false;
  }

  [kCreateEvent](nodeValue, type) {
    return new CustomEvent(type, { detail: nodeValue });
  }
  [customInspectSymbol](depth, options) {
    const name = this.constructor.name;
    if (depth < 0)
      return name;

    const opts = ObjectAssign({}, options, {
      depth: NumberIsInteger(options.depth) ? options.depth - 1 : options.depth,
    });

    return `${name} ${inspect({}, opts)}`;
  }
}

ObjectDefineProperties(EventTarget.prototype, {
  addEventListener: kEnumerableProperty,
  removeEventListener: kEnumerableProperty,
  dispatchEvent: kEnumerableProperty,
  [SymbolToStringTag]: {
    __proto__: null,
    writable: false,
    enumerable: false,
    configurable: true,
    value: 'EventTarget',
  },
});

// EventTarget API

function validateEventListener(listener) {
  if (typeof listener === 'function' ||
      typeof listener?.handleEvent === 'function') {
    return true;
  }

  if (listener == null)
    return false;

  if (typeof listener === 'object') {
    // Require `handleEvent` lazily.
    return true;
  }

  throw new ERR_INVALID_ARG_TYPE('listener', 'EventListener', listener);
}

function validateEventListenerOptions(options) {
  if (typeof options === 'boolean')
    return { capture: options };

  if (options === null)
    return kEmptyObject;
  validateObject(options, 'options', kValidateObjectAllowObjects);
  return {
    once: Boolean(options.once),
    capture: Boolean(options.capture),
    passive: Boolean(options.passive),
    signal: options.signal,
    weak: options[kWeakHandler],
    resistStopPropagation: options[kResistStopPropagation] ?? false,
    isNodeStyleListener: Boolean(options[kIsNodeStyleListener]),
  };
}

// Test whether the argument is an event object. This is far from a fool-proof
// test, for example this input will result in a false positive:
// > isEventTarget({ constructor: EventTarget })
// It stands in its current implementation as a compromise.
// Ref: https://github.com/nodejs/node/pull/33661
function isEventTarget(obj) {
  return obj?.constructor?.[kIsEventTarget];
}

function addCatch(promise) {
  const then = promise.then;
  if (typeof then === 'function') {
    FunctionPrototypeCall(then, promise, undefined, function(err) {
      // The callback is called with nextTick to avoid a follow-up
      // rejection from this promise.
      emitUncaughtException(err);
    });
  }
}

function emitUncaughtException(err) {
  process.nextTick(() => { throw err; });
}

function makeEventHandler(handler) {
  // Event handlers are dispatched in the order they were first set
  // See https://github.com/nodejs/node/pull/35949#issuecomment-722496598
  function eventHandler(...args) {
    if (typeof eventHandler.handler !== 'function') {
      return;
    }
    return ReflectApply(eventHandler.handler, this, args);
  }
  eventHandler.handler = handler;
  return eventHandler;
}

function defineEventHandler(emitter, name, event = name) {
  // 8.1.5.1 Event handlers - basically `on[eventName]` attributes
  const propName = `on${name}`;
  function get() {
    validateThisInternalField(this, kHandlers, 'EventTarget');
    return this[kHandlers]?.get(event)?.handler ?? null;
  }
  ObjectDefineProperty(get, 'name', {
    __proto__: null,
    value: `get ${propName}`,
  });

  function set(value) {
    validateThisInternalField(this, kHandlers, 'EventTarget');
    if (this[kHandlers] === undefined)
      this[kHandlers] = new SafeMap();
    let wrappedHandler = this[kHandlers].get(event);
    if (wrappedHandler) {
      const eventsMap = getEventsMap(this);
      if (typeof wrappedHandler.handler === 'function') {
        const root = eventsMap.get(event);
        if (root !== undefined) {
          root.size--;
          this[kRemoveListener](root.size, event, wrappedHandler.handler, false);
        }
      }
      wrappedHandler.handler = value;
      if (typeof wrappedHandler.handler === 'function') {
        const root = eventsMap.get(event);
        if (root !== undefined) {
          root.size++;
          this[kNewListener](root.size, event, value, false, false, false, false);
        } else {
          // The wrapper was externally removed from kEvents (e.g. via
          // removeEventListener); re-register it so the handler fires.
          this.addEventListener(event, wrappedHandler);
        }
      }
    } else {
      wrappedHandler = makeEventHandler(value);
      this.addEventListener(event, wrappedHandler);
    }
    this[kHandlers].set(event, wrappedHandler);
  }
  ObjectDefineProperty(set, 'name', {
    __proto__: null,
    value: `set ${propName}`,
  });

  ObjectDefineProperty(emitter, propName, {
    __proto__: null,
    get,
    set,
    configurable: true,
    enumerable: true,
  });
}

module.exports = {
  Event,
  CustomEvent,
  EventTarget,
  defineEventHandler,
  initEventTarget,
  kCreateEvent,
  kNewListener,
  kTrustEvent,
  kRemoveListener,
  kEvents,
  kWeakHandler,
  kResistStopPropagation,
  isEventTarget,
};
