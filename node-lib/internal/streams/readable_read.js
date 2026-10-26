// Derived from Node.js lib/internal/streams/readable.js (v26.8.2), split to fit the embedded
// file budget. Copyright Joyent, Inc. and other Node contributors. MIT license.

'use strict';

exports.computeNewHighWaterMark = computeNewHighWaterMark;
exports.howMuchToRead = howMuchToRead;
exports.onEofChunk = onEofChunk;
exports.emitReadable = emitReadable;
exports.emitReadable_ = emitReadable_;
exports.maybeReadMore = maybeReadMore;
exports.maybeReadMore_ = maybeReadMore_;

const {
  NumberIsInteger,
  NumberIsNaN,
  NumberParseInt,
} = primordials;
let debug = require('internal/util/debuglog').debuglog('stream', (fn) => {
  debug = fn;
});
const {
  kState,
  kObjectMode,
  kErrorEmitted,
  kDestroyed,
  kCloseEmitted,
  kErrored,
  kConstructed,
} = require('internal/streams/utils');
const {
  codes: {
    ERR_METHOD_NOT_IMPLEMENTED,
    ERR_OUT_OF_RANGE,
  },
} = require('internal/errors');
const Readable = require('internal/streams/readable');
const { errorOrDestroy, kDecoderValue, kEnded, kReading, kSync, kNeedReadable, kEmittedReadable, kMultiAwaitDrain, kReadingMore, kDataEmitted, kDecoder, kFlowing } = require('internal/streams/readable_state');
const { fromList, endReadable } = require('internal/streams/readable_push');
const { flow } = require('internal/streams/readable_pipe');

// Don't raise the hwm > 1GB.
const MAX_HWM = 0x40000000;
function computeNewHighWaterMark(n) {
  if (n > MAX_HWM) {
    throw new ERR_OUT_OF_RANGE('size', '<= 1GiB', n);
  } else {
    // Get the next highest power of 2 to prevent increasing hwm excessively in
    // tiny amounts.
    n--;
    n |= n >>> 1;
    n |= n >>> 2;
    n |= n >>> 4;
    n |= n >>> 8;
    n |= n >>> 16;
    n++;
  }
  return n;
}

// This function is designed to be inlinable, so please take care when making
// changes to the function body.
function howMuchToRead(n, state) {
  if (n <= 0 || (state.length === 0 && (state[kState] & kEnded) !== 0))
    return 0;
  if ((state[kState] & kObjectMode) !== 0)
    return 1;
  if (NumberIsNaN(n)) {
    // Fast path for buffers.
    if ((state[kState] & kDecoder) === 0 && state.length)
      return state.buffer[state.bufferIndex].length;

    // Only flow one buffer at a time.
    if ((state[kState] & kFlowing) !== 0 && state.length)
      return state.buffer[state.bufferIndex].length;

    return state.length;
  }
  if (n <= state.length)
    return n;
  return (state[kState] & kEnded) !== 0 ? state.length : 0;
}

// You can override either this method, or the async _read(n) below.
Readable.prototype.read = function(n) {
  debug('read', n);
  // Same as parseInt(undefined, 10), however V8 7.3 performance regressed
  // in this scenario, so we are doing it manually.
  if (n === undefined) {
    n = NaN;
  } else if (!NumberIsInteger(n)) {
    n = NumberParseInt(n, 10);
  }
  const state = this._readableState;
  const nOrig = n;

  // If we're asking for more than the current hwm, then raise the hwm.
  if (n > state.highWaterMark)
    state.highWaterMark = computeNewHighWaterMark(n);

  if (n !== 0)
    state[kState] &= ~kEmittedReadable;

  // If we're doing read(0) to trigger a readable event, but we
  // already have a bunch of data in the buffer, then just trigger
  // the 'readable' event and move on. `state.length` cannot change
  // within this block, so it is loaded once instead of three times.
  const stateLength = state.length;
  if (n === 0 &&
      (state[kState] & kNeedReadable) !== 0 &&
      ((state.highWaterMark !== 0 ?
        stateLength >= state.highWaterMark :
        stateLength > 0) ||
       (state[kState] & kEnded) !== 0)) {
    debug('read: emitReadable');
    if (stateLength === 0 && (state[kState] & kEnded) !== 0)
      endReadable(this);
    else
      emitReadable(this);
    return null;
  }

  n = howMuchToRead(n, state);

  // If we've ended, and we're now clear, then finish it up.
  if (n === 0 && (state[kState] & kEnded) !== 0) {
    if (state.length === 0)
      endReadable(this);
    return null;
  }

  // All the actual chunk generation logic needs to be
  // *below* the call to _read.  The reason is that in certain
  // synthetic stream cases, such as passthrough streams, _read
  // may be a completely synchronous operation which may change
  // the state of the read buffer, providing enough data when
  // before there was *not* enough.
  //
  // So, the steps are:
  // 1. Figure out what the state of things will be after we do
  // a read from the buffer.
  //
  // 2. If that resulting state will trigger a _read, then call _read.
  // Note that this may be asynchronous, or synchronous.  Yes, it is
  // deeply ugly to write APIs this way, but that still doesn't mean
  // that the Readable class should behave improperly, as streams are
  // designed to be sync/async agnostic.
  // Take note if the _read call is sync or async (ie, if the read call
  // has returned yet), so that we know whether or not it's safe to emit
  // 'readable' etc.
  //
  // 3. Actually pull the requested chunks out of the buffer and return.

  // if we need a readable event, then we need to do some reading.
  let doRead = (state[kState] & kNeedReadable) !== 0;
  debug('need readable', doRead);

  // If we currently have less than the highWaterMark, then also read some.
  if (state.length === 0 || state.length - n < state.highWaterMark) {
    doRead = true;
    debug('length less than watermark', doRead);
  }

  // However, if we've ended, then there's no point, if we're already
  // reading, then it's unnecessary, if we're constructing we have to wait,
  // and if we're destroyed or errored, then it's not allowed,
  if ((state[kState] & (kReading | kEnded | kDestroyed | kErrored | kConstructed)) !== kConstructed) {
    doRead = false;
    debug('reading, ended or constructing', doRead);
  } else if (doRead) {
    debug('do read');
    state[kState] |= kReading | kSync;
    // If the length is currently zero, then we *need* a readable event.
    if (state.length === 0)
      state[kState] |= kNeedReadable;

    // Call internal read method
    try {
      this._read(state.highWaterMark);
    } catch (err) {
      errorOrDestroy(this, err);
    }
    state[kState] &= ~kSync;

    // If _read pushed data synchronously, then `reading` will be false,
    // and we need to re-evaluate how much data we can return to the user.
    if ((state[kState] & kReading) === 0)
      n = howMuchToRead(nOrig, state);
  }

  let ret;
  if (n > 0)
    ret = fromList(n, state);
  else
    ret = null;

  if (ret === null) {
    state[kState] |= state.length <= state.highWaterMark ? kNeedReadable : 0;
    n = 0;
  } else {
    state.length -= n;
    if ((state[kState] & kMultiAwaitDrain) !== 0) {
      state.awaitDrainWriters.clear();
    } else {
      state.awaitDrainWriters = null;
    }
  }

  if (state.length === 0) {
    // If we have nothing in the buffer, then we want to know
    // as soon as we *do* get something into the buffer.
    if ((state[kState] & kEnded) === 0)
      state[kState] |= kNeedReadable;

    // If we tried to read() past the EOF, then emit end on the next tick.
    if (nOrig !== n && (state[kState] & kEnded) !== 0)
      endReadable(this);
  }

  if (ret !== null && (state[kState] & (kErrorEmitted | kCloseEmitted)) === 0) {
    state[kState] |= kDataEmitted;
    this.emit('data', ret);
  }

  return ret;
};

function onEofChunk(stream, state) {
  debug('onEofChunk');
  if ((state[kState] & kEnded) !== 0) return;
  const decoder = (state[kState] & kDecoder) !== 0 ? state[kDecoderValue] : null;
  if (decoder) {
    const chunk = decoder.end();
    if (chunk?.length) {
      state.buffer.push(chunk);
      state.length += (state[kState] & kObjectMode) !== 0 ? 1 : chunk.length;
    }
  }
  state[kState] |= kEnded;

  if ((state[kState] & kSync) !== 0) {
    // If we are sync, wait until next tick to emit the data.
    // Otherwise we risk emitting data in the flow()
    // the readable code triggers during a read() call.
    emitReadable(stream);
  } else {
    // Emit 'readable' now to make sure it gets picked up.
    state[kState] &= ~kNeedReadable;
    state[kState] |= kEmittedReadable;
    // We have to emit readable now that we are EOF. Modules
    // in the ecosystem (e.g. dicer) rely on this event being sync.
    emitReadable_(stream);
  }
}

// Don't emit readable right away in sync mode, because this can trigger
// another read() call => stack overflow.  This way, it might trigger
// a nextTick recursion warning, but that's not so bad.
function emitReadable(stream) {
  const state = stream._readableState;
  debug('emitReadable');
  state[kState] &= ~kNeedReadable;
  if ((state[kState] & kEmittedReadable) === 0) {
    debug('emitReadable', (state[kState] & kFlowing) !== 0);
    state[kState] |= kEmittedReadable;
    process.nextTick(emitReadable_, stream);
  }
}

function emitReadable_(stream) {
  const state = stream._readableState;
  debug('emitReadable_');
  if ((state[kState] & (kDestroyed | kErrored)) === 0 && (state.length || (state[kState] & kEnded) !== 0)) {
    stream.emit('readable');
    state[kState] &= ~kEmittedReadable;
  }

  // The stream needs another readable event if:
  // 1. It is not flowing, as the flow mechanism will take
  //    care of it.
  // 2. It is not ended.
  // 3. It is below the highWaterMark, so we can schedule
  //    another readable later.
  state[kState] |=
    (state[kState] & (kFlowing | kEnded)) === 0 &&
    state.length <= state.highWaterMark ? kNeedReadable : 0;
  flow(stream);
}


// At this point, the user has presumably seen the 'readable' event,
// and called read() to consume some data.  that may have triggered
// in turn another _read(n) call, in which case reading = true if
// it's in progress.
// However, if we're not ended, or reading, and the length < hwm,
// then go ahead and try to read some more preemptively.
function maybeReadMore(stream, state) {
  if ((state[kState] & (kReadingMore | kReading | kConstructed)) === kConstructed) {
    state[kState] |= kReadingMore;
    process.nextTick(maybeReadMore_, stream, state);
  }
}

function maybeReadMore_(stream, state) {
  // Attempt to read more data if we should.
  //
  // The conditions for reading more data are (one of):
  // - Not enough data buffered (state.length < state.highWaterMark). The loop
  //   is responsible for filling the buffer with enough data if such data
  //   is available. If highWaterMark is 0 and we are not in the flowing mode
  //   we should _not_ attempt to buffer any extra data. We'll get more data
  //   when the stream consumer calls read() instead.
  // - No data in the buffer, and the stream is in flowing mode. In this mode
  //   the loop below is responsible for ensuring read() is called. Failing to
  //   call read here would abort the flow and there's no other mechanism for
  //   continuing the flow if the stream consumer has just subscribed to the
  //   'data' event.
  //
  // In addition to the above conditions to keep reading data, the following
  // conditions prevent the data from being read:
  // - The stream has ended (state.ended).
  // - There is already a pending 'read' operation (state.reading). This is a
  //   case where the stream has called the implementation defined _read()
  //   method, but they are processing the call asynchronously and have _not_
  //   called push() with new data. In this case we skip performing more
  //   read()s. The execution ends in this method again after the _read() ends
  //   up calling push() with more data.
  while ((state[kState] & (kReading | kEnded)) === 0 &&
         (state.length < state.highWaterMark ||
          ((state[kState] & kFlowing) !== 0 && state.length === 0))) {
    const len = state.length;
    debug('maybeReadMore read 0');
    stream.read(0);
    if (len === state.length)
      // Didn't get any data, stop spinning.
      break;
  }
  state[kState] &= ~kReadingMore;
}

// Abstract method.  to be overridden in specific implementation classes.
// call cb(er, data) where data is <= n in length.
// for virtual (non-string, non-buffer) streams, "length" is somewhat
// arbitrary, and perhaps not very meaningful.
Readable.prototype._read = function(n) {
  throw new ERR_METHOD_NOT_IMPLEMENTED('_read()');
};
