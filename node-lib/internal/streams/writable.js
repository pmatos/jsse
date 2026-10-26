// Copyright Joyent, Inc. and other Node contributors.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to permit
// persons to whom the Software is furnished to do so, subject to the
// following conditions:
//
// The above copyright notice and this permission notice shall be included
// in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN
// NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
// DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
// OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE
// USE OR OTHER DEALINGS IN THE SOFTWARE.

'use strict';

module.exports = Writable;

const {
  FunctionPrototypeSymbolHasInstance,
  ObjectDefineProperty,
  ObjectSetPrototypeOf,
  SymbolHasInstance,
} = primordials;
const Stream = require('internal/streams/legacy').Stream;
const destroyImpl = require('internal/streams/destroy');
const {
  addAbortSignal,
} = require('internal/streams/add-abort-signal');
const {
  kOnConstructed,
} = require('internal/streams/utils');
const {
  assignFunctionName,
} = require('internal/util');
const { WritableState } = require('internal/streams/writable_state');

Writable.WritableState = WritableState;
ObjectSetPrototypeOf(Writable.prototype, Stream.prototype);
ObjectSetPrototypeOf(Writable, Stream);

function Writable(options) {
  if (!(this instanceof Writable))
    return new Writable(options);

  this._events ??= {
    close: undefined,
    error: undefined,
    prefinish: undefined,
    finish: undefined,
    drain: undefined,
    // Skip uncommon events...
    // [destroyImpl.kConstruct]: undefined,
    // [destroyImpl.kDestroy]: undefined,
  };

  this._writableState = new WritableState(options, this, false);

  if (options) {
    if (typeof options.write === 'function')
      this._write = options.write;

    if (typeof options.writev === 'function')
      this._writev = options.writev;

    if (typeof options.destroy === 'function')
      this._destroy = options.destroy;

    if (typeof options.final === 'function')
      this._final = options.final;

    if (typeof options.construct === 'function')
      this._construct = options.construct;

    if (options.signal)
      addAbortSignal(options.signal, this);
  }

  Stream.call(this, options);

  if (this._construct != null) {
    destroyImpl.construct(this, () => {
      this._writableState[kOnConstructed](this);
    });
  }
}

ObjectDefineProperty(Writable, SymbolHasInstance, {
  __proto__: null,
  value: assignFunctionName(SymbolHasInstance, function(instance) {
    if (FunctionPrototypeSymbolHasInstance(this, instance)) return true;
    if (this !== Writable) return false;

    return instance && instance._writableState instanceof WritableState;
  }),
});

require('internal/streams/writable_state');
require('internal/streams/writable_write');
require('internal/streams/writable_end');
