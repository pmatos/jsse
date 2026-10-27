'use strict';

// The asynchronous half of `fs.Dir`: the listing is a snapshot taken by
// `opendirSync`, so reading it needs no syscall and these only defer.

const { fs } = require('internal/host/fs');

const { Dir } = fs;
const proto = Dir.prototype;

function viaCallback(self, method, cb, name) {
  if (cb === undefined) {
    return new Promise((resolve, reject) => {
      self[name]((error, value) => (error ? reject(error) : resolve(value)));
    });
  }
  if (typeof cb !== 'function') {
    const { codes } = require('internal/errors');
    throw new codes.ERR_INVALID_ARG_TYPE('callback', 'Function', cb);
  }
  setImmediate(() => {
    let value;
    try {
      value = self[method]();
    } catch (error) {
      cb(error);
      return;
    }
    if (value === undefined) cb(null);
    else cb(null, value);
  });
}

proto.read = function read(cb) {
  return viaCallback(this, 'readSync', cb, 'read');
};

proto.close = function close(cb) {
  return viaCallback(this, 'closeSync', cb, 'close');
};

proto[Symbol.asyncIterator] = async function* entries() {
  try {
    for (;;) {
      const entry = await this.read();
      if (entry === null) return;
      yield entry;
    }
  } finally {
    try {
      await this.close();
    } catch (error) {
      if (error.code !== 'ERR_DIR_CLOSED') throw error;
    }
  }
};

if (Symbol.asyncDispose) {
  proto[Symbol.asyncDispose] = function asyncDispose() {
    return this.close().catch((error) => {
      if (error.code !== 'ERR_DIR_CLOSED') throw error;
    });
  };
}
