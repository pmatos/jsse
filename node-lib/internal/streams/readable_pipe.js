// Derived from Node.js lib/internal/streams/readable.js (v26.8.2), split to fit the embedded
// file budget. Copyright Joyent, Inc. and other Node contributors. MIT license.

'use strict';

exports.pipeOnDrain = pipeOnDrain;
exports.updateReadableListening = updateReadableListening;
exports.nReadingNextTick = nReadingNextTick;
exports.resume = resume;
exports.resume_ = resume_;
exports.flow = flow;

const {
  ArrayPrototypeIndexOf,
  ReflectApply,
  SafeSet,
} = primordials;
const {
  Stream,
  prependListener,
} = require('internal/streams/legacy');
let debug = require('internal/util/debuglog').debuglog('stream', (fn) => {
  debug = fn;
});
const {
  kState,
  kDestroyed,
} = require('internal/streams/utils');
const Readable = require('internal/streams/readable');
const { errorOrDestroy, kEndEmitted, kReading, kNeedReadable, kEmittedReadable, kReadableListening, kResumeScheduled, kMultiAwaitDrain, kHasFlowing, kFlowing, kHasPaused, kPaused, kDataListening } = require('internal/streams/readable_state');
const { emitReadable } = require('internal/streams/readable_read');

Readable.prototype.pipe = function(dest, pipeOpts) {
  const src = this;
  const state = this._readableState;

  if (state.pipes.length === 1) {
    if ((state[kState] & kMultiAwaitDrain) === 0) {
      state[kState] |= kMultiAwaitDrain;
      state.awaitDrainWriters = new SafeSet(
        state.awaitDrainWriters ? [state.awaitDrainWriters] : [],
      );
    }
  }

  state.pipes.push(dest);
  debug('pipe count=%d opts=%j', state.pipes.length, pipeOpts);

  const doEnd = (!pipeOpts || pipeOpts.end !== false) &&
              dest !== process.stdout &&
              dest !== process.stderr;

  const endFn = doEnd ? onend : unpipe;
  if ((state[kState] & kEndEmitted) !== 0)
    process.nextTick(endFn);
  else
    src.once('end', endFn);

  dest.on('unpipe', onunpipe);
  function onunpipe(readable, unpipeInfo) {
    debug('onunpipe');
    if (readable === src) {
      if (unpipeInfo && unpipeInfo.hasUnpiped === false) {
        unpipeInfo.hasUnpiped = true;
        cleanup();
      }
    }
  }

  function onend() {
    debug('onend');
    dest.end();
  }

  let ondrain;

  let cleanedUp = false;
  function cleanup() {
    debug('cleanup');
    // Cleanup event handlers once the pipe is broken.
    dest.removeListener('close', onclose);
    dest.removeListener('finish', onfinish);
    if (ondrain) {
      dest.removeListener('drain', ondrain);
    }
    dest.removeListener('error', onerror);
    dest.removeListener('unpipe', onunpipe);
    src.removeListener('end', onend);
    src.removeListener('end', unpipe);
    src.removeListener('data', ondata);

    cleanedUp = true;

    // If the reader is waiting for a drain event from this
    // specific writer, then it would cause it to never start
    // flowing again.
    // So, if this is awaiting a drain, then we just call it now.
    // `pipeOnDrain` only removes this destination from the awaiting-drain set
    // and resumes the source once nothing else is awaiting a drain, so it is
    // safe to call unconditionally and no-ops when this writer wasn't awaiting
    // a drain. Guarding on `needDrain` used to skip this for a destination that
    // errored synchronously, leaving it stuck in the set and starving other
    // destinations piped from the same source (see #53185).
    if (ondrain && state.awaitDrainWriters)
      ondrain();
  }

  function pause() {
    // If the user unpiped during `dest.write()`, it is possible
    // to get stuck in a permanently paused state if that write
    // also returned false.
    // => Check whether `dest` is still a piping destination.
    if (!cleanedUp) {
      if (state.pipes.length === 1 && state.pipes[0] === dest) {
        debug('false write response, pause', 0);
        state.awaitDrainWriters = dest;
        state[kState] &= ~kMultiAwaitDrain;
      } else if (state.pipes.length > 1 && state.pipes.includes(dest)) {
        debug('false write response, pause', state.awaitDrainWriters.size);
        state.awaitDrainWriters.add(dest);
      }
      src.pause();
    }
    if (!ondrain) {
      // When the dest drains, it reduces the awaitDrain counter
      // on the source.  This would be more elegant with a .once()
      // handler in flow(), but adding and removing repeatedly is
      // too slow.
      ondrain = pipeOnDrain(src, dest);
      dest.on('drain', ondrain);
    }
  }

  src.on('data', ondata);
  function ondata(chunk) {
    debug('ondata');
    try {
      const ret = dest.write(chunk);
      debug('dest.write', ret);

      if (ret === false) {
        pause();
      }
    } catch (error) {
      dest.destroy(error);
    }
  }

  // If the dest has an error, then stop piping into it.
  // However, don't suppress the throwing behavior for this.
  function onerror(er) {
    debug('onerror', er);
    unpipe();
    dest.removeListener('error', onerror);
    if (dest.listenerCount('error') === 0) {
      const s = dest._writableState || dest._readableState;
      if (s && !s.errorEmitted) {
        // User incorrectly emitted 'error' directly on the stream.
        errorOrDestroy(dest, er);
      } else {
        dest.emit('error', er);
      }
    }
  }

  // Make sure our error handler is attached before userland ones.
  prependListener(dest, 'error', onerror);

  // Both close and finish should trigger unpipe, but only once.
  function onclose() {
    dest.removeListener('finish', onfinish);
    unpipe();
  }
  dest.once('close', onclose);
  function onfinish() {
    debug('onfinish');
    dest.removeListener('close', onclose);
    unpipe();
  }
  dest.once('finish', onfinish);

  function unpipe() {
    debug('unpipe');
    src.unpipe(dest);
  }

  // Tell the dest that it's being piped to.
  dest.emit('pipe', src);

  // Start the flow if it hasn't been started already.

  if (dest.writableNeedDrain === true) {
    pause();
  } else if ((state[kState] & kFlowing) === 0) {
    debug('pipe resume');
    src.resume();
  }

  return dest;
};

function pipeOnDrain(src, dest) {
  return function pipeOnDrainFunctionResult() {
    const state = src._readableState;

    // `ondrain` will call directly,
    // `this` maybe not a reference to dest,
    // so we use the real dest here.
    if (state.awaitDrainWriters === dest) {
      debug('pipeOnDrain', 1);
      state.awaitDrainWriters = null;
    } else if ((state[kState] & kMultiAwaitDrain) !== 0) {
      debug('pipeOnDrain', state.awaitDrainWriters.size);
      state.awaitDrainWriters.delete(dest);
    }

    if ((!state.awaitDrainWriters || state.awaitDrainWriters.size === 0) &&
      (state[kState] & kDataListening) !== 0) {
      src.resume();
    }
  };
}


Readable.prototype.unpipe = function(dest) {
  const state = this._readableState;
  const unpipeInfo = { hasUnpiped: false };

  // If we're not piping anywhere, then do nothing.
  if (state.pipes.length === 0)
    return this;

  if (!dest) {
    // remove all.
    const dests = state.pipes;
    state.pipes = [];
    this.pause();

    for (let i = 0; i < dests.length; i++)
      dests[i].emit('unpipe', this, { hasUnpiped: false });
    return this;
  }

  // Try to find the right one.
  const index = ArrayPrototypeIndexOf(state.pipes, dest);
  if (index === -1)
    return this;

  state.pipes.splice(index, 1);
  if (state.pipes.length === 0)
    this.pause();

  dest.emit('unpipe', this, unpipeInfo);

  return this;
};

// Set up data events if they are asked for
// Ensure readable listeners eventually get something.
Readable.prototype.on = function(ev, fn) {
  const res = Stream.prototype.on.call(this, ev, fn);
  const state = this._readableState;

  if (ev === 'data') {
    state[kState] |= kDataListening;

    // Update readableListening so that resume() may be a no-op
    // a few lines down. This is needed to support once('readable').
    state[kState] |= this.listenerCount('readable') > 0 ? kReadableListening : 0;

    // Try start flowing on next tick if stream isn't explicitly paused.
    if ((state[kState] & (kHasFlowing | kFlowing)) !== kHasFlowing) {
      this.resume();
    }
  } else if (ev === 'readable') {
    if ((state[kState] & (kEndEmitted | kReadableListening)) === 0) {
      state[kState] |= kReadableListening | kNeedReadable | kHasFlowing;
      state[kState] &= ~(kFlowing | kEmittedReadable);
      debug('on readable');
      if (state.length) {
        emitReadable(this);
      } else if ((state[kState] & kReading) === 0) {
        process.nextTick(nReadingNextTick, this);
      }
    }
  }

  return res;
};
Readable.prototype.addListener = Readable.prototype.on;

Readable.prototype.removeListener = function(ev, fn) {
  const state = this._readableState;

  const res = Stream.prototype.removeListener.call(this,
                                                   ev, fn);

  if (ev === 'readable') {
    // We need to check if there is someone still listening to
    // readable and reset the state. However this needs to happen
    // after readable has been emitted but before I/O (nextTick) to
    // support once('readable', fn) cycles. This means that calling
    // resume within the same tick will have no
    // effect.
    process.nextTick(updateReadableListening, this);
  } else if (ev === 'data' && this.listenerCount('data') === 0) {
    state[kState] &= ~kDataListening;
  }

  return res;
};
Readable.prototype.off = Readable.prototype.removeListener;

Readable.prototype.removeAllListeners = function(ev) {
  const res = ReflectApply(Stream.prototype.removeAllListeners, this, arguments);

  if (ev === 'readable' || ev === undefined) {
    // We need to check if there is someone still listening to
    // readable and reset the state. However this needs to happen
    // after readable has been emitted but before I/O (nextTick) to
    // support once('readable', fn) cycles. This means that calling
    // resume within the same tick will have no
    // effect.
    process.nextTick(updateReadableListening, this);
  }

  return res;
};

function updateReadableListening(self) {
  const state = self._readableState;

  if (self.listenerCount('readable') > 0) {
    state[kState] |= kReadableListening;
  } else {
    state[kState] &= ~kReadableListening;
  }

  if ((state[kState] & (kHasPaused | kPaused | kResumeScheduled)) === (kHasPaused | kResumeScheduled)) {
    // Flowing needs to be set to true now, otherwise
    // the upcoming resume will not flow.
    state[kState] |= kHasFlowing | kFlowing;

    // Crude way to check if we should resume.
  } else if ((state[kState] & kDataListening) !== 0) {
    self.resume();
  } else if ((state[kState] & kReadableListening) === 0) {
    state[kState] &= ~(kHasFlowing | kFlowing);
  }
}

function nReadingNextTick(self) {
  debug('readable nexttick read 0');
  self.read(0);
}

// pause() and resume() are remnants of the legacy readable stream API
// If the user uses them, then switch into old mode.
Readable.prototype.resume = function() {
  const state = this._readableState;
  if ((state[kState] & kDestroyed) !== 0) {
    return this;
  }
  if ((state[kState] & kFlowing) === 0) {
    debug('resume');
    // We flow only if there is no one listening
    // for readable, but we still have to call
    // resume().
    state[kState] |= kHasFlowing;
    if ((state[kState] & kReadableListening) === 0) {
      state[kState] |= kFlowing;
    } else {
      state[kState] &= ~kFlowing;
    }
    resume(this, state);
  }
  state[kState] |= kHasPaused;
  state[kState] &= ~kPaused;
  return this;
};

function resume(stream, state) {
  if ((state[kState] & kResumeScheduled) === 0) {
    state[kState] |= kResumeScheduled;
    process.nextTick(resume_, stream, state);
  }
}

function resume_(stream, state) {
  debug('resume', (state[kState] & kReading) !== 0);
  if ((state[kState] & kReading) === 0) {
    stream.read(0);
  }

  state[kState] &= ~kResumeScheduled;
  stream.emit('resume');
  flow(stream);
  if ((state[kState] & (kFlowing | kReading)) === kFlowing)
    stream.read(0);
}

Readable.prototype.pause = function() {
  const state = this._readableState;
  if ((state[kState] & kDestroyed) !== 0) {
    return this;
  }
  debug('call pause');
  if ((state[kState] & (kHasFlowing | kFlowing)) !== kHasFlowing) {
    debug('pause');
    state[kState] |= kHasFlowing;
    state[kState] &= ~kFlowing;
    this.emit('pause');
  }
  state[kState] |= kHasPaused | kPaused;
  return this;
};

function flow(stream) {
  const state = stream._readableState;
  debug('flow');
  while ((state[kState] & kFlowing) !== 0 && stream.read() !== null);
}
