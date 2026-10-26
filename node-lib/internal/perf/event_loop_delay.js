'use strict';
const {
  Symbol,
  SymbolDispose,
} = primordials;

const {
  codes: {
    ERR_ILLEGAL_CONSTRUCTOR,
    ERR_INVALID_THIS,
  },
} = require('internal/errors');

const {
  validateBoolean,
  validateInteger,
  validateObject,
} = require('internal/validators');

const {
  Histogram,
  HdrState,
  kHandle,
  kSkipThrow,
} = require('internal/histogram');
const { setInterval, clearInterval } = require('timers');

const {
  kEmptyObject,
} = require('internal/util');


const kEnabled = Symbol('kEnabled');

// Samples the time between ticks of an unref'd interval, like Node's uv timer.
class EventLoopDelayHandle extends HdrState {
  constructor(resolution) {
    super(1, 3.6e12, 3);
    this.resolution = resolution;
    this.timer = undefined;
  }

  start() {
    let previous = process.hrtime.bigint();
    this.timer = setInterval(() => {
      const now = process.hrtime.bigint();
      if (now > previous) this.record(now - previous);
      previous = now;
    }, this.resolution);
    this.timer.unref();
  }

  stop() {
    clearInterval(this.timer);
    this.timer = undefined;
  }
}

class ELDHistogram extends Histogram {
  constructor(skipThrowSymbol = undefined) {
    if (skipThrowSymbol !== kSkipThrow) {
      throw new ERR_ILLEGAL_CONSTRUCTOR();
    }
    super(skipThrowSymbol);
  }

  /**
   * @returns {boolean}
   */
  enable() {
    if (this[kEnabled] === undefined)
      throw new ERR_INVALID_THIS('ELDHistogram');
    if (this[kEnabled]) return false;
    this[kEnabled] = true;
    this[kHandle].start();
    return true;
  }

  /**
   * @returns {boolean}
   */
  disable() {
    if (this[kEnabled] === undefined)
      throw new ERR_INVALID_THIS('ELDHistogram');
    if (!this[kEnabled]) return false;
    this[kEnabled] = false;
    this[kHandle].stop();
    return true;
  }

  [SymbolDispose]() {
    this.disable();
  }
}

/**
 * @param {{
 *   samplePerIteration : boolean,
 *   resolution : number
 * }} [options]
 * @returns {ELDHistogram}
 */
function monitorEventLoopDelay(options = kEmptyObject) {
  validateObject(options, 'options');

  const { samplePerIteration = false, resolution = 10 } = options;
  validateBoolean(samplePerIteration, 'options.samplePerIteration');
  validateInteger(resolution, 'options.resolution', 1);

  const histogram = new ELDHistogram(kSkipThrow);
  histogram[kEnabled] = false;
  histogram[kHandle] = new EventLoopDelayHandle(resolution);
  return histogram;
}

module.exports = monitorEventLoopDelay;
