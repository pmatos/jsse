
/**
 * @param {Duplex} duplex
 * @param {{ readableType?: 'bytes' }} [options]
 * @returns {ReadableWritablePair}
 */
function newReadableWritablePairFromDuplex(duplex, options = kEmptyObject) {
  // Not using the internal/streams/utils isWritableNodeStream and
  // isReadableNodeStream utilities here because they will return false
  // if the duplex was created with writable or readable options set to
  // false. Instead, we'll check the readable and writable state after
  // and return closed WritableStream or closed ReadableStream as
  // necessary.
  if (typeof duplex?._writableState !== 'object' ||
      typeof duplex?._readableState !== 'object') {
    throw new ERR_INVALID_ARG_TYPE('duplex', 'stream.Duplex', duplex);
  }

  validateObject(options, 'options');

  const readableOptions = {
    __proto__: null,
    type: options.readableType,
  };

  if (options.readableType == null && options.type != null) {
    // 'options.type' is a deprecated alias for 'options.readableType'
    emitDEP0201();
    readableOptions.type = options.type;
  }

  if (isDestroyed(duplex)) {
    const writable = new WritableStream();
    const readable = new ReadableStream({ type: readableOptions.type });
    writable.close();
    readable.cancel();
    return { readable, writable };
  }

  const writableOptions = {
    __proto__: null,
    [kValidateChunk]: options[kValidateChunk],
    [kDestroyOnSyncError]: options[kDestroyOnSyncError],
  };

  const writable =
    isWritable(duplex) ?
      newWritableStreamFromStreamWritable(duplex, writableOptions) :
      new WritableStream();

  if (!isWritable(duplex))
    writable.close();

  const readable =
    isReadable(duplex) ?
      newReadableStreamFromStreamReadable(duplex, readableOptions) :
      new ReadableStream({ type: readableOptions.type });

  if (!isReadable(duplex))
    readable.cancel();

  return { writable, readable };
}

/**
 * @param {ReadableWritablePair} pair
 * @param {{
 *   allowHalfOpen? : boolean,
 *   decodeStrings? : boolean,
 *   encoding? : string,
 *   highWaterMark? : number,
 *   objectMode? : boolean,
 *   signal? : AbortSignal,
 * }} [options]
 * @returns {Duplex}
 */
function newStreamDuplexFromReadableWritablePair(pair = kEmptyObject, options = kEmptyObject) {
  validateObject(pair, 'pair');
  const {
    readable: readableStream,
    writable: writableStream,
  } = pair;

  if (!isReadableStream(readableStream)) {
    throw new ERR_INVALID_ARG_TYPE(
      'pair.readable',
      'ReadableStream',
      readableStream);
  }
  if (!isWritableStream(writableStream)) {
    throw new ERR_INVALID_ARG_TYPE(
      'pair.writable',
      'WritableStream',
      writableStream);
  }

  validateObject(options, 'options');
  const {
    allowHalfOpen = false,
    objectMode = false,
    encoding,
    decodeStrings = true,
    highWaterMark,
    signal,
  } = options;

  validateBoolean(objectMode, 'options.objectMode');
  if (encoding !== undefined && !Buffer.isEncoding(encoding))
    throw new ERR_INVALID_ARG_VALUE('options.encoding', encoding);

  const writer = writableStream.getWriter();
  const reader = readableStream.getReader();
  let writableClosed = false;
  let readableClosed = false;

  const duplex = new Duplex({
    allowHalfOpen,
    highWaterMark,
    objectMode,
    encoding,
    decodeStrings,
    signal,

    writev(chunks, callback) {
      function done(error) {
        try {
          callback(error);
        } catch (error) {
          // In a next tick because this is happening within
          // a promise context, and if there are any errors
          // thrown we don't want those to cause an unhandled
          // rejection. Let's just escape the promise and
          // handle it separately.
          process.nextTick(() => destroy(duplex, error));
        }
      }

      PromisePrototypeThen(
        PromisePrototypeThen(
          writer.ready,
          () => SafePromiseAllReturnVoid(
            chunks,
            (data) => writer.write(data.chunk))),
        done,
        done);
    },

    write(chunk, encoding, callback) {
      if (typeof chunk === 'string' && decodeStrings && !objectMode) {
        const enc = normalizeEncoding(encoding);

        if (enc === 'utf8') {
          chunk = encoder.encode(chunk);
        } else {
          chunk = Buffer.from(chunk, encoding);
          chunk = new Uint8Array(
            TypedArrayPrototypeGetBuffer(chunk),
            TypedArrayPrototypeGetByteOffset(chunk),
            TypedArrayPrototypeGetByteLength(chunk),
          );
        }
      }

      function done(error) {
        try {
          callback(error);
        } catch (error) {
          destroy(duplex, error);
        }
      }

      PromisePrototypeThen(
        PromisePrototypeThen(writer.ready, () => writer.write(chunk)),
        done,
        done);
    },

    final(callback) {
      function done(error) {
        try {
          callback(error);
        } catch (error) {
          // In a next tick because this is happening within
          // a promise context, and if there are any errors
          // thrown we don't want those to cause an unhandled
          // rejection. Let's just escape the promise and
          // handle it separately.
          process.nextTick(() => destroy(duplex, error));
        }
      }

      if (!writableClosed) {
        PromisePrototypeThen(
          writer.close(),
          done,
          done);
      }
    },

    read() {
      PromisePrototypeThen(
        reader.read(),
        (chunk) => {
          if (chunk.done) {
            duplex.push(null);
          } else {
            duplex.push(chunk.value);
          }
        },
        (error) => destroy(duplex, error));
    },

    destroy(error, callback) {
      function done() {
        try {
          callback(error);
        } catch (error) {
          // In a next tick because this is happening within
          // a promise context, and if there are any errors
          // thrown we don't want those to cause an unhandled
          // rejection. Let's just escape the promise and
          // handle it separately.
          process.nextTick(() => { throw error; });
        }
      }

      async function closeWriter() {
        if (!writableClosed)
          await writer.abort(error);
      }

      async function closeReader() {
        if (!readableClosed)
          await reader.cancel(error);
      }

      if (!writableClosed || !readableClosed) {
        PromisePrototypeThen(
          SafePromiseAllReturnVoid([
            closeWriter(),
            closeReader(),
          ]),
          done,
          done);
        return;
      }

      done();
    },
  });

  PromisePrototypeThen(
    writer.closed,
    () => {
      writableClosed = true;
      if (!isWritableEnded(duplex))
        destroy(duplex, new ERR_STREAM_PREMATURE_CLOSE());
    },
    (error) => {
      writableClosed = true;
      readableClosed = true;
      destroy(duplex, error);
    });

  PromisePrototypeThen(
    reader.closed,
    () => {
      readableClosed = true;
    },
    (error) => {
      writableClosed = true;
      readableClosed = true;
      destroy(duplex, error);
    });

  return duplex;
}

module.exports = {
  newWritableStreamFromStreamWritable,
  newReadableStreamFromStreamReadable,
  newStreamWritableFromWritableStream,
  newStreamReadableFromReadableStream,
  newReadableWritablePairFromDuplex,
  newStreamDuplexFromReadableWritablePair,
  kValidateChunk,
  kDestroyOnSyncError,
};
