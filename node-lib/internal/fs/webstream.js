'use strict';

// FileHandle.prototype.readableWebStream (#1504), after Node v26.8.2
// lib/internal/fs/promises.js: a byte ReadableStream over the handle that holds
// a reference on it until the stream is done, cancelled or the handle closes.

const { FunctionPrototypeBind } = primordials;
const { codes: { ERR_INVALID_STATE } } = require('internal/errors');
const { kEmptyObject } = require('internal/util');
const { validateBoolean, validateObject } = require('internal/validators');

const kLocked = Symbol('kLocked');

function installReadableWebStream(FileHandle, { kRef, kUnref, kClosePromise }) {
  FileHandle.prototype.readableWebStream = function readableWebStream(options = kEmptyObject) {
    if (this.fd === -1)
      throw new ERR_INVALID_STATE('The FileHandle is closed');
    if (this[kClosePromise])
      throw new ERR_INVALID_STATE('The FileHandle is closing');
    if (this[kLocked])
      throw new ERR_INVALID_STATE('The FileHandle is locked');
    this[kLocked] = true;

    validateObject(options, 'options');
    const {
      type = 'bytes',
      autoClose = false,
    } = options;

    validateBoolean(autoClose, 'options.autoClose');

    if (type !== 'bytes') {
      process.emitWarning(
        'A non-"bytes" options.type has no effect. A byte-oriented steam is ' +
        'always created.',
        'ExperimentalWarning',
      );
    }

    const readFn = FunctionPrototypeBind(this.read, this);
    const ondone = async () => {
      this[kUnref]();
      if (autoClose) await this.close();
    };

    const { ReadableStream, readableStreamCancel } = require('internal/webstreams/readablestream');
    const readable = new ReadableStream({
      type: 'bytes',
      autoAllocateChunkSize: 16384,

      async pull(controller) {
        const view = controller.byobRequest.view;
        const { bytesRead } = await readFn(view, view.byteOffset, view.byteLength);

        if (bytesRead === 0) {
          controller.close();
          await ondone();
        }

        controller.byobRequest.respond(bytesRead);
      },

      async cancel() {
        await ondone();
      },
    });

    this[kRef]();
    this.once('close', () => {
      readableStreamCancel(readable);
    });

    return readable;
  };
}

module.exports = { installReadableWebStream };
