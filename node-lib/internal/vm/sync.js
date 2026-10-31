'use strict';

// Keeps a contextified object and its realm's global object in step. Node does
// this with named-property interceptors; the engine's global environment
// reads and writes the global object's own properties directly, so a Proxy
// global would not see declarations or implicit globals. Instead the global
// object stays ordinary and mirrors the object:
//
//  - a property the object has when a script starts becomes a forwarding
//    accessor on the global (live in both directions while it runs and for
//    functions called later), or a plain copy when it must stay data
//    (read-only properties, names the global already holds as non-configurable);
//  - what the script leaves on the global (var, function, implicit globals,
//    `this.x`) is pushed back to the object when it ends;
//  - a name the script deleted is deleted from the object.
//
// Properties the script creates are therefore visible to the object only
// once the run that made them has ended.

const {
  ObjectIs,
  ObjectPrototype,
  ReflectDefineProperty,
  ReflectDeleteProperty,
  ReflectGet,
  ReflectGetOwnPropertyDescriptor,
  ReflectGetPrototypeOf,
  ReflectOwnKeys,
  ReflectSet,
  SafeMap,
  SafeSet,
} = primordials;

const kForward = 0;
const kCopy = 1;

function isData(desc) {
  return 'value' in desc;
}

function sameDescriptor(a, b) {
  if (isData(a) !== isData(b)) return false;
  if (a.enumerable !== b.enumerable || a.configurable !== b.configurable) {
    return false;
  }
  return isData(a) ?
    a.writable === b.writable && ObjectIs(a.value, b.value) :
    a.get === b.get && a.set === b.set;
}

// What a script sees of the object: its own properties and those inherited
// from prototypes other than the main Object.prototype.
function visibleKeys(object) {
  const keys = ReflectOwnKeys(object);
  let seen;
  for (let o = ReflectGetPrototypeOf(object); o !== null && o !== ObjectPrototype;
       o = ReflectGetPrototypeOf(o)) {
    seen ??= new SafeSet(keys);
    const inherited = ReflectOwnKeys(o);
    for (let i = 0; i < inherited.length; i++) {
      if (!seen.has(inherited[i])) {
        seen.add(inherited[i]);
        keys[keys.length] = inherited[i];
      }
    }
  }
  return keys;
}

function lookup(object, key) {
  for (let o = object; o !== null && o !== ObjectPrototype;
       o = ReflectGetPrototypeOf(o)) {
    const desc = ReflectGetOwnPropertyDescriptor(o, key);
    if (desc !== undefined) return desc;
  }
  return undefined;
}

function createSync(sandbox, global) {
  const baseline = new SafeMap();
  const tracked = new SafeMap();
  const own = ReflectOwnKeys(global);
  for (let i = 0; i < own.length; i++) {
    baseline.set(own[i], ReflectGetOwnPropertyDescriptor(global, own[i]));
  }

  function forwarder(key, enumerable) {
    const entry = {
      mode: kForward,
      get() {
        const value = ReflectGet(sandbox, key);
        return value === sandbox ? global : value;
      },
      set(value) {
        ReflectSet(sandbox, key, value);
      },
    };
    ReflectDefineProperty(global, key, {
      __proto__: null,
      get: entry.get,
      set: entry.set,
      enumerable,
      configurable: true,
    });
    return entry;
  }

  function restore(key) {
    const original = baseline.get(key);
    if (original !== undefined) {
      ReflectDefineProperty(global, key, original);
    } else {
      ReflectDeleteProperty(global, key);
    }
  }

  function adopt(key, desc) {
    const held = ReflectGetOwnPropertyDescriptor(global, key);
    const readOnly = isData(desc) && !desc.writable;
    if (held !== undefined && !held.configurable) {
      if (isData(desc) && isData(held) && held.writable) {
        global[key] = desc.value;
      }
      tracked.set(key, { mode: kCopy, last: desc.value });
    } else if (readOnly) {
      ReflectDefineProperty(global, key, {
        __proto__: null,
        value: desc.value,
        writable: false,
        enumerable: desc.enumerable,
        configurable: true,
      });
      tracked.set(key, { mode: kCopy, last: desc.value, restorable: true });
    } else {
      tracked.set(key, forwarder(key, desc.enumerable));
    }
  }

  // Before a script runs: pick up what the object gained or lost.
  function enter() {
    for (const { 0: key, 1: entry } of tracked) {
      if (lookup(sandbox, key) !== undefined) continue;
      if (entry.mode === kForward || entry.restorable) restore(key);
      tracked.delete(key);
    }
    const keys = visibleKeys(sandbox);
    for (let i = 0; i < keys.length; i++) {
      const key = keys[i];
      const entry = tracked.get(key);
      const desc = lookup(sandbox, key);
      if (desc === undefined) continue;
      if (entry === undefined) {
        adopt(key, desc);
      } else if (entry.mode === kCopy && isData(desc) &&
                 !ObjectIs(desc.value, entry.last)) {
        ReflectDefineProperty(global, key, { __proto__: null, value: desc.value });
        entry.last = desc.value;
      }
    }
  }

  function push(key, desc) {
    const onSandbox = { __proto__: null, ...desc };
    if (isData(desc) && desc.writable) {
      if (desc.enumerable && !desc.configurable) onSandbox.configurable = true;
      if (desc.configurable && baseline.has(key)) onSandbox.enumerable = true;
    }
    if (!ReflectDefineProperty(sandbox, key, onSandbox)) return;
    if (desc.configurable) {
      tracked.set(key, forwarder(key, desc.enumerable));
    } else if (isData(desc)) {
      tracked.set(key, { mode: kCopy, last: desc.value });
    }
  }

  // After a script ran: push what it left on the global back to the object.
  function leave() {
    const keys = ReflectOwnKeys(global);
    const present = new SafeMap();
    for (let i = 0; i < keys.length; i++) {
      const key = keys[i];
      present.set(key, true);
      const desc = ReflectGetOwnPropertyDescriptor(global, key);
      let entry = tracked.get(key);
      if (entry !== undefined && entry.mode === kForward) {
        if (desc.get === entry.get) continue;
        tracked.delete(key);
        entry = undefined;
      }
      if (entry !== undefined) {
        if (isData(desc) && !ObjectIs(desc.value, entry.last)) {
          ReflectSet(sandbox, key, desc.value);
          entry.last = desc.value;
        }
        continue;
      }
      const original = baseline.get(key);
      if (original !== undefined && sameDescriptor(original, desc)) continue;
      if (original === undefined && isData(desc) && desc.value === undefined) {
        continue;
      }
      push(key, desc);
    }
    for (const { 0: key } of tracked) {
      if (present.has(key)) continue;
      ReflectDeleteProperty(sandbox, key);
      tracked.delete(key);
    }
  }

  return { enter, leave };
}

module.exports = { createSync };
