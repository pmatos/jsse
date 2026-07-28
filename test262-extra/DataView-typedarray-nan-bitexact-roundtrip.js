/*---
description: >
  DataView/TypedArray raw-byte float accessors (getFloat64/32/16 and the
  matching typed-array index access) must decode any JS-controlled byte
  pattern faithfully: a NaN encoding must always decode as NaN, a signed
  zero must always decode with its sign bit intact, and merely reading must
  never mutate the backing buffer. Writing a decoded value back out (via the
  matching setFloatN method or a typed-array index assignment) must encode
  it deterministically: the same decoded value always re-encodes to the same
  bytes, on both the DataView and typed-array write paths.

  This is a regression baseline for the NaN-boxing migration (epic #69,
  design in #402, docs/specs/2026-07-26-nan-boxed-js-value-design.md).
  Issue #405 routed every raw-byte float read through a single smart
  constructor, `JsValue::number`; issue #406 (this file) pins today's
  behavior of that boundary so a future change to `JsValue::number` (the
  Phase 3 representation swap, issue #414) cannot silently corrupt or
  misdecode a JS-controlled bit pattern.

  Byte-exact preservation is intentionally NOT asserted for NaN payloads.
  Per the ratified design, Phase 3 makes `JsValue::number` canonicalize
  *every* NaN — regardless of sign, payload, or signaling/quiet bit,
  including patterns with no tag collision at all — to a single positive
  bit pattern (`if n.is_nan() { CANONICAL_NAN } else { n }`). A byte-exact
  round-trip assertion for NaN would therefore necessarily start failing
  the moment Phase 3 correctly lands. What the design doc *does* guarantee
  bit-exact, forever, is signed zero and every other finite double
  (`+0.0`/`-0.0` differ from the reserved boxing signature by construction,
  since neither is ever NaN) — those get the strict byte-exact assertion
  below. For NaN, this file asserts the two invariants that hold under
  either the current no-op passthrough or the future canonicalizing
  constructor: the value still decodes as NaN, and re-encoding it is
  deterministic and agrees between the DataView and typed-array write
  paths. Deliberately included among the tested patterns is
  `0xFFF8000000000000` (sign 1, exponent all-1, quiet bit 1) — the exact
  reserved NaN-boxing signature the design doc calls out as
  hardware-producible via `(-1.0).sqrt()` on this project's target.
esid: sec-dataview.prototype.getfloat64
info: |
  DataView.prototype.getFloat64 ( byteOffset [ , littleEndian ] )
    3. Return ? GetViewValue(v, byteOffset, littleEndian, "Float64").

  24.1.1.5 GetValueFromBuffer ( arrayBuffer, byteIndex, type, isLittleEndian )
    Interprets the raw bytes as the requested IEEE 754 format and returns
    the mathematical value with no canonicalization step in the read
    direction; NaN payload/sign is exactly whatever the bytes encode.

  10.4.5.7 IntegerIndexedElementGet ( O, index )
    Calls GetValueFromBuffer with isLittleEndian fixed to *true*,
    unconditionally, regardless of host byte order — this is why the tests
    below always pass littleEndian=true to the DataView accessors too, so
    both access paths agree on the same in-memory byte layout.

  24.1.1.6 NumberToRawBytes ( type, value, isLittleEndian )
    "If value is NaN, rawValue may be set to any implementation chosen
    IEEE 754-2008 binary64 [or binary32/binary16] format Not-a-Number
    encoding. An implementation must always choose either the same
    encoding for each implementation distinguishable NaN value, or an
    implementation-defined canonical value." This is the spec license the
    determinism assertions below rely on and the design doc's
    canonicalize-every-NaN choice exercises.
includes: [compareArray.js]
---*/

function bytesToBuffer(bytes) {
  var buffer = new ArrayBuffer(bytes.length);
  var dv = new DataView(buffer);
  for (var i = 0; i < bytes.length; i++) {
    dv.setUint8(i, bytes[i]);
  }
  return buffer;
}

// Exercise one raw NaN byte pattern through both read paths (DataView,
// typed-array index) and both write-back paths, asserting: still NaN,
// source buffer untouched by reads, and deterministic re-encoding that
// agrees between DataView and typed-array writes. Byte-exact preservation
// of the *original* pattern is deliberately not asserted here — see the
// file-level description.
function checkNaNRoundTrip(bytes, width, getMethod, setMethod, TA, label) {
  var buffer = bytesToBuffer(bytes);

  var viaDV = new DataView(buffer)[getMethod](0, true);
  assert(viaDV !== viaDV, label + ": DataView " + getMethod + " should read a NaN");
  assert.compareArray(new Uint8Array(buffer), bytes, label + ": buffer unchanged after " + getMethod);

  var viaTA = new TA(buffer)[0];
  assert(viaTA !== viaTA, label + ": typed array index read should be NaN");
  assert.compareArray(new Uint8Array(buffer), bytes, label + ": buffer unchanged after typed array read");

  var rtDV1 = new DataView(new ArrayBuffer(width));
  rtDV1[setMethod](0, viaDV, true);
  var rtDV2 = new DataView(new ArrayBuffer(width));
  rtDV2[setMethod](0, viaDV, true);
  assert.compareArray(
    new Uint8Array(rtDV1.buffer), new Uint8Array(rtDV2.buffer),
    label + ": " + setMethod + " re-encodes the same NaN deterministically"
  );

  var rtTA1 = new TA(new ArrayBuffer(width));
  rtTA1[0] = viaTA;
  var rtTA2 = new TA(new ArrayBuffer(width));
  rtTA2[0] = viaTA;
  assert.compareArray(
    new Uint8Array(rtTA1.buffer), new Uint8Array(rtTA2.buffer),
    label + ": typed-array index assignment re-encodes the same NaN deterministically"
  );

  assert.compareArray(
    new Uint8Array(rtDV1.buffer), new Uint8Array(rtTA1.buffer),
    label + ": DataView and typed-array writes encode the read-back NaN identically"
  );

  var reread = new TA(rtDV1.buffer)[0];
  assert(reread !== reread, label + ": round-tripped bytes still decode as NaN");
}

// Signed zero has no payload to lose at any width, and the design doc
// guarantees it (unlike NaN) survives a box/unbox round trip unchanged
// forever, so this asserts full byte-exact preservation.
function checkSignedZeroRoundTrip(bytes, width, getMethod, setMethod, TA, label, negative) {
  var buffer = bytesToBuffer(bytes);

  var viaDV = new DataView(buffer)[getMethod](0, true);
  assert(viaDV === 0, label + ": DataView " + getMethod + " reads zero");
  assert.sameValue(Object.is(viaDV, -0), negative, label + ": DataView sign bit via Object.is");
  assert.compareArray(new Uint8Array(buffer), bytes, label + ": buffer unchanged after " + getMethod);

  var viaTA = new TA(buffer)[0];
  assert(viaTA === 0, label + ": typed array index reads zero");
  assert.sameValue(Object.is(viaTA, -0), negative, label + ": typed array sign bit via Object.is");
  assert.compareArray(new Uint8Array(buffer), bytes, label + ": buffer unchanged after typed array read");

  var rtDV = new DataView(new ArrayBuffer(width));
  rtDV[setMethod](0, viaDV, true);
  assert.compareArray(new Uint8Array(rtDV.buffer), bytes, label + ": " + setMethod + " round trip preserves the sign bit exactly");

  var rtTA = new TA(new ArrayBuffer(width));
  rtTA[0] = viaTA;
  assert.compareArray(new Uint8Array(rtTA.buffer), bytes, label + ": typed-array round trip preserves the sign bit exactly");
}

// --- Float64: 8 raw NaN byte patterns (little-endian in-memory layout) ---
var FLOAT64_NANS = [
  { bytes: [0, 0, 0, 0, 0, 0, 248, 127], label: "f64 canonical quiet (0x7FF8000000000000)" },
  { bytes: [0, 0, 0, 0, 0, 0, 249, 127], label: "f64 quiet with payload (0x7FF9000000000000)" },
  // The exact reserved NaN-boxing signature: sign 1, exponent all-1, quiet
  // bit 1 -- design doc's cited (-1.0).sqrt() hardware output.
  { bytes: [0, 0, 0, 0, 0, 0, 248, 255], label: "f64 negative quiet (0xFFF8000000000000, reserved signature)" },
  { bytes: [1, 0, 0, 0, 0, 0, 240, 127], label: "f64 positive signaling, minimal payload (0x7FF0000000000001)" },
  { bytes: [1, 0, 0, 0, 0, 0, 240, 255], label: "f64 negative signaling, minimal payload (0xFFF0000000000001)" },
  { bytes: [255, 255, 255, 255, 255, 255, 255, 127], label: "f64 max mantissa, positive (0x7FFFFFFFFFFFFFFF)" },
  { bytes: [255, 255, 255, 255, 255, 255, 255, 255], label: "f64 max mantissa, negative (0xFFFFFFFFFFFFFFFF)" },
];

FLOAT64_NANS.forEach(function (c) {
  checkNaNRoundTrip(c.bytes, 8, "getFloat64", "setFloat64", Float64Array, c.label);
});

// --- Float32: 5 raw NaN byte patterns ---
var FLOAT32_NANS = [
  { bytes: [0, 0, 192, 127], label: "f32 canonical quiet (0x7FC00000)" },
  { bytes: [1, 0, 192, 127], label: "f32 quiet with payload (0x7FC00001)" },
  { bytes: [0, 0, 192, 255], label: "f32 negative quiet (0xFFC00000)" },
  { bytes: [1, 0, 128, 127], label: "f32 positive signaling, minimal payload (0x7F800001)" },
  { bytes: [255, 255, 255, 255], label: "f32 max mantissa, negative (0xFFFFFFFF)" },
];

FLOAT32_NANS.forEach(function (c) {
  checkNaNRoundTrip(c.bytes, 4, "getFloat32", "setFloat32", Float32Array, c.label);
});

// --- Float16: 5 raw NaN byte patterns ---
var FLOAT16_NANS = [
  { bytes: [0, 126], label: "f16 canonical quiet (0x7E00)" },
  { bytes: [1, 126], label: "f16 quiet with payload (0x7E01)" },
  { bytes: [0, 254], label: "f16 negative quiet (0xFE00)" },
  { bytes: [1, 124], label: "f16 positive signaling, minimal payload (0x7C01)" },
  { bytes: [255, 255], label: "f16 max mantissa, negative (0xFFFF)" },
];

FLOAT16_NANS.forEach(function (c) {
  checkNaNRoundTrip(c.bytes, 2, "getFloat16", "setFloat16", Float16Array, c.label);
});

// --- Signed zero: bit-exact forever, at every width ---
checkSignedZeroRoundTrip([0, 0, 0, 0, 0, 0, 0, 0], 8, "getFloat64", "setFloat64", Float64Array, "f64 +0.0", false);
checkSignedZeroRoundTrip([0, 0, 0, 0, 0, 0, 0, 128], 8, "getFloat64", "setFloat64", Float64Array, "f64 -0.0", true);
checkSignedZeroRoundTrip([0, 0, 0, 0], 4, "getFloat32", "setFloat32", Float32Array, "f32 +0.0", false);
checkSignedZeroRoundTrip([0, 0, 0, 128], 4, "getFloat32", "setFloat32", Float32Array, "f32 -0.0", true);
checkSignedZeroRoundTrip([0, 0], 2, "getFloat16", "setFloat16", Float16Array, "f16 +0.0", false);
checkSignedZeroRoundTrip([0, 128], 2, "getFloat16", "setFloat16", Float16Array, "f16 -0.0", true);
