'use strict';

// The `fs` module: the synchronous core is assembled by the prelude
// (scripts/node-fs-*.js) and parked on `internal/host/fs`; the callback API,
// `fs.promises` and the streams load on first use. See
// docs/adr/2026-10-26-2300-node-host-fs-async.md.

const { ObjectDefineProperty } = primordials;
const bundle = require('internal/host/fs');
const { fs } = bundle;

// The lazy accessors below must not be seen by anything that copies `fs`.
bundle.sync = { ...fs };

function defineLazy(id, names) {
  for (const name of names) {
    const settle = (target, value) => {
      ObjectDefineProperty(target, name, {
        value, writable: true, enumerable: true, configurable: true,
      });
      return value;
    };
    ObjectDefineProperty(fs, name, {
      get() { return settle(this, require(id)[name]); },
      set(value) { settle(this, value); },
      enumerable: true,
      configurable: true,
    });
  }
}

defineLazy('internal/fs/callbacks', [
  'access', 'appendFile', 'chmod', 'chown', 'close', 'copyFile', 'cp',
  'exists', 'fchmod', 'fchown', 'fdatasync', 'fstat', 'fsync', 'ftruncate',
  'futimes', 'lchown', 'link', 'lstat', 'lutimes', 'mkdir', 'mkdtemp', 'open',
  'opendir', 'read', 'readFile', 'readdir', 'readlink', 'realpath', 'rename',
  'rm', 'rmdir', 'stat', 'statfs', 'symlink', 'truncate', 'unlink', 'utimes',
  'write', 'writeFile',
]);
defineLazy('internal/fs/streams', [
  'ReadStream', 'WriteStream', 'FileReadStream', 'FileWriteStream',
  'createReadStream', 'createWriteStream',
]);

defineLazy('internal/fs/glob', ['glob', 'globSync']);

ObjectDefineProperty(fs, 'promises', {
  get() { return require('fs/promises'); },
  enumerable: true,
  configurable: true,
});

require('internal/fs/dir');

module.exports = fs;
