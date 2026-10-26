'use strict';

// A JavaScript port of the HdrHistogram arithmetic behind Node v26.8.2
// lib/internal/histogram.js (MIT), so `createHistogram()` and
// `monitorEventLoopDelay()` report the same min/max/mean/stddev/percentiles as
// Node for the same recorded values. Not ported: the Node 26 statistics
// extras (cdf, ccdf, countAt, ewma*, burnRate, skewness, kurtosis, the
// ksTest/welchTest family, buckets, percentileCI, percentilesAt, subtract,
// recordCorrected) and structured-clone transfer.

const {
  Map,
  MapPrototypeEntries,
  MathCeil,
  MathLog2,
  MathSqrt,
  NumberIsNaN,
  NumberMAX_SAFE_INTEGER,
  ObjectFromEntries,
  Symbol,
} = primordials;

const {
  customInspectSymbol: kInspect,
  kEmptyObject,
} = require('internal/util');
const { inspect } = require('util');
const {
  codes: {
    ERR_ILLEGAL_CONSTRUCTOR,
    ERR_INVALID_ARG_TYPE,
    ERR_INVALID_ARG_VALUE,
    ERR_INVALID_THIS,
    ERR_OUT_OF_RANGE,
  },
} = require('internal/errors');
const {
  validateInteger,
  validateNumber,
  validateObject,
} = require('internal/validators');

const kHandle = Symbol('kHandle');
const kRecordable = Symbol('kRecordable');
const kSkipThrow = Symbol('kSkipThrow');

const INT64_MAX = (1n << 63n) - 1n;

function bitLength(v) {
  return v === 0n ? 0 : v.toString(2).length;
}

class HdrState {
  constructor(lowest, highest, figures) {
    this.lowest = BigInt(lowest);
    this.highest = BigInt(highest);
    const largestSingleUnit = 2 * 10 ** figures;
    this.unitMagnitude = BigInt(bitLength(this.lowest) - 1);
    const subBucketCountMagnitude = MathCeil(MathLog2(largestSingleUnit));
    this.halfMagnitude = BigInt(subBucketCountMagnitude - 1);
    this.subBucketCount = 1n << BigInt(subBucketCountMagnitude);
    this.subBucketHalfCount = this.subBucketCount >> 1n;
    this.subBucketMask = (this.subBucketCount - 1n) << this.unitMagnitude;
    let smallestUntrackable = this.subBucketCount << this.unitMagnitude;
    let buckets = 1n;
    while (smallestUntrackable <= this.highest) {
      if (smallestUntrackable > INT64_MAX / 2n) {
        buckets++;
        break;
      }
      smallestUntrackable <<= 1n;
      buckets++;
    }
    this.countsLength = Number((buckets + 1n) * this.subBucketHalfCount);
    this.reset();
  }

  reset() {
    this.counts = new Map();
    this.total = 0n;
    this.minValue = INT64_MAX;
    this.maxValue = 0n;
    this.exceedsCount = 0n;
    this.prev = 0n;
  }

  bucketIndex(v) {
    return BigInt(bitLength(v | this.subBucketMask)) - this.unitMagnitude -
           (this.halfMagnitude + 1n);
  }

  subBucketIndex(v, bucket) {
    return v >> (bucket + this.unitMagnitude);
  }

  lowestEquivalent(v) {
    const bucket = this.bucketIndex(v);
    return this.subBucketIndex(v, bucket) << (bucket + this.unitMagnitude);
  }

  rangeSize(v) {
    const bucket = this.bucketIndex(v);
    const sub = this.subBucketIndex(v, bucket);
    return 1n << (this.unitMagnitude + (sub >= this.subBucketCount ? bucket + 1n : bucket));
  }

  highestEquivalent(v) {
    return this.lowestEquivalent(v) + this.rangeSize(v) - 1n;
  }

  medianEquivalent(v) {
    return this.lowestEquivalent(v) + (this.rangeSize(v) >> 1n);
  }

  countsIndex(v) {
    const bucket = this.bucketIndex(v);
    const sub = this.subBucketIndex(v, bucket);
    return Number(((bucket + 1n) << this.halfMagnitude) + (sub - this.subBucketHalfCount));
  }

  record(value) {
    value = BigInt(value);
    if (value < 0n || this.countsIndex(value) >= this.countsLength) {
      this.exceedsCount++;
      return;
    }
    const slot = this.lowestEquivalent(value);
    this.counts.set(slot, (this.counts.get(slot) ?? 0n) + 1n);
    this.total++;
    if (value < this.minValue) this.minValue = value;
    if (value > this.maxValue) this.maxValue = value;
  }

  recordDelta(now) {
    const previous = this.prev;
    this.prev = now;
    if (previous > 0n && now > previous) this.record(now - previous);
  }

  add(other) {
    for (const [slot, count] of other.counts) {
      this.counts.set(slot, (this.counts.get(slot) ?? 0n) + count);
    }
    this.total += other.total;
    if (other.minValue < this.minValue) this.minValue = other.minValue;
    if (other.maxValue > this.maxValue) this.maxValue = other.maxValue;
    this.exceedsCount += other.exceedsCount;
  }

  sortedSlots() {
    return [...this.counts].sort((a, b) => (a[0] < b[0] ? -1 : 1));
  }

  countBigInt() { return this.total; }
  exceedsBigInt() { return this.exceedsCount; }

  minBigInt() {
    if (this.total === 0n) return INT64_MAX;
    return this.counts.has(0n) ? 0n : this.lowestEquivalent(this.minValue);
  }

  maxBigInt() {
    return this.maxValue === 0n ? 0n : this.highestEquivalent(this.maxValue);
  }

  mean() {
    if (this.total === 0n) return NaN;
    let sum = 0;
    for (const [slot, count] of this.counts) {
      sum += Number(count) * Number(this.medianEquivalent(slot));
    }
    return sum / Number(this.total);
  }

  stddev() {
    if (this.total === 0n) return NaN;
    const mean = this.mean();
    let geometric = 0;
    for (const [slot, count] of this.counts) {
      const dev = Number(this.medianEquivalent(slot)) - mean;
      geometric += dev * dev * Number(count);
    }
    return MathSqrt(geometric / Number(this.total));
  }

  percentileBigInt(percentile) {
    const requested = percentile > 100 ? 100 : percentile;
    let target = BigInt(Math.trunc((requested / 100) * Number(this.total) + 0.5));
    if (target < 1n) target = 1n;
    let cumulative = 0n;
    for (const [slot, count] of this.sortedSlots()) {
      cumulative += count;
      if (cumulative >= target) return this.highestEquivalent(slot);
    }
    return 0n;
  }

  // The HdrHistogram percentile iterator with one tick per half distance.
  percentileEntries() {
    const out = [];
    const slots = this.sortedSlots();
    let toIterateTo = 0;
    let cumulative = 0n;
    let value = 0n;
    let index = -1;
    const advance = () => {
      if (++index >= slots.length) return false;
      value = slots[index][0];
      cumulative += slots[index][1];
      return true;
    };
    for (;;) {
      if (cumulative >= this.total) {
        out.push([100, value]);
        return out;
      }
      if (index === -1) advance();
      let emitted = false;
      do {
        const current = (100 * Number(cumulative)) / Number(this.total);
        if (toIterateTo <= current) {
          out.push([toIterateTo, value]);
          const temp = Math.trunc(MathLog2(100 / (100 - toIterateTo))) + 1;
          toIterateTo += 100 / 2 ** temp;
          emitted = true;
          break;
        }
      } while (advance());
      if (!emitted) return out;
    }
  }
}

function isHistogram(object) {
  return object?.[kHandle] !== undefined;
}

function fromEntries(entries, transform) {
  const map = new Map();
  for (let i = 0; i < entries.length; i++) {
    map.set(entries[i][0], transform(entries[i][1]));
  }
  return map;
}

function checkPercentile(percentile) {
  validateNumber(percentile, 'percentile');
  if (NumberIsNaN(percentile) || percentile <= 0 || percentile > 100)
    throw new ERR_OUT_OF_RANGE('percentile', '> 0 && <= 100', percentile);
}

function handleOf(self) {
  if (!isHistogram(self))
    throw new ERR_INVALID_THIS('Histogram');
  return self[kHandle];
}

class Histogram {
  constructor(skipThrowSymbol = undefined) {
    if (skipThrowSymbol !== kSkipThrow) {
      throw new ERR_ILLEGAL_CONSTRUCTOR();
    }
  }

  [kInspect](depth, options) {
    if (depth < 0)
      return this;

    const opts = {
      ...options,
      depth: options.depth == null ? null : options.depth - 1,
    };

    return `Histogram ${inspect({
      min: this.min,
      max: this.max,
      mean: this.mean,
      exceeds: this.exceeds,
      stddev: this.stddev,
      count: this.count,
      percentiles: this.percentiles,
    }, opts)}`;
  }

  get count() { return Number(handleOf(this).countBigInt()); }
  get countBigInt() { return handleOf(this).countBigInt(); }
  get min() { return Number(handleOf(this).minBigInt()); }
  get minBigInt() { return handleOf(this).minBigInt(); }
  get max() { return Number(handleOf(this).maxBigInt()); }
  get maxBigInt() { return handleOf(this).maxBigInt(); }
  get mean() { return handleOf(this).mean(); }
  get exceeds() { return Number(handleOf(this).exceedsBigInt()); }
  get exceedsBigInt() { return handleOf(this).exceedsBigInt(); }
  get stddev() { return handleOf(this).stddev(); }

  percentile(percentile) {
    const handle = handleOf(this);
    checkPercentile(percentile);
    return Number(handle.percentileBigInt(percentile));
  }

  percentileBigInt(percentile) {
    const handle = handleOf(this);
    checkPercentile(percentile);
    return handle.percentileBigInt(percentile);
  }

  get percentiles() {
    return fromEntries(handleOf(this).percentileEntries(), Number);
  }

  get percentilesBigInt() {
    return fromEntries(handleOf(this).percentileEntries(), (v) => v);
  }

  reset() {
    handleOf(this).reset();
  }

  toJSON() {
    return {
      count: this.count,
      min: this.min,
      max: this.max,
      mean: this.mean,
      exceeds: this.exceeds,
      stddev: this.stddev,
      percentiles: ObjectFromEntries(MapPrototypeEntries(this.percentiles)),
    };
  }
}

class RecordableHistogram extends Histogram {
  constructor(skipThrowSymbol = undefined) {
    if (skipThrowSymbol !== kSkipThrow) {
      throw new ERR_ILLEGAL_CONSTRUCTOR();
    }

    super(skipThrowSymbol);
  }

  record(val) {
    if (this[kRecordable] === undefined)
      throw new ERR_INVALID_THIS('RecordableHistogram');
    if (typeof val !== 'bigint')
      validateInteger(val, 'val', 1);
    this[kHandle].record(val);
  }

  recordDelta() {
    if (this[kRecordable] === undefined)
      throw new ERR_INVALID_THIS('RecordableHistogram');
    this[kHandle].recordDelta(process.hrtime.bigint());
  }

  add(other) {
    if (this[kRecordable] === undefined)
      throw new ERR_INVALID_THIS('RecordableHistogram');
    if (other[kRecordable] === undefined)
      throw new ERR_INVALID_ARG_TYPE('other', 'RecordableHistogram', other);
    this[kHandle].add(other[kHandle]);
  }
}

function createRecordableHistogram(handle) {
  const histogram = new RecordableHistogram(kSkipThrow);
  histogram[kRecordable] = true;
  histogram[kHandle] = handle;
  return histogram;
}

function createHistogram(options = kEmptyObject) {
  validateObject(options, 'options');
  const {
    lowest = 1,
    highest = NumberMAX_SAFE_INTEGER,
    figures = 3,
  } = options;
  if (typeof lowest !== 'bigint')
    validateInteger(lowest, 'options.lowest', 1, NumberMAX_SAFE_INTEGER);
  if (typeof highest !== 'bigint') {
    validateInteger(highest, 'options.highest',
                    2 * lowest, NumberMAX_SAFE_INTEGER);
  } else if (highest < 2n * lowest) {
    throw new ERR_INVALID_ARG_VALUE('options.highest', highest);
  }
  validateInteger(figures, 'options.figures', 1, 5);
  return createRecordableHistogram(new HdrState(lowest, highest, figures));
}

module.exports = {
  Histogram,
  RecordableHistogram,
  HdrState,
  isHistogram,
  kHandle,
  kSkipThrow,
  createHistogram,
};
