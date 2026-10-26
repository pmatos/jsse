'use strict';

// Web streams (ReadableStream and friends) do not exist in jsse yet, so the
// stream <-> web stream interop entry points of Readable, Writable and Duplex
// throw instead of adapting.

const {
  codes: {
    ERR_METHOD_NOT_IMPLEMENTED,
  },
} = require('internal/errors');

function unavailable(name) {
  return function() {
    throw new ERR_METHOD_NOT_IMPLEMENTED(`${name} (web streams are unavailable)`);
  };
}

module.exports = {
  newStreamReadableFromReadableStream: unavailable('Readable.fromWeb()'),
  newReadableStreamFromStreamReadable: unavailable('Readable.toWeb()'),
  newStreamWritableFromWritableStream: unavailable('Writable.fromWeb()'),
  newWritableStreamFromStreamWritable: unavailable('Writable.toWeb()'),
  newStreamDuplexFromReadableWritablePair: unavailable('Duplex.fromWeb()'),
  newReadableWritablePairFromDuplex: unavailable('Duplex.toWeb()'),
};
