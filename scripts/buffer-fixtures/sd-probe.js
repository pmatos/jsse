'use strict';

// StringDecoder differential probe: module shape, errors, and the decoder state
// machine over every encoding, hand-picked byte sequences (valid and not) cut at
// every point, plus seeded random chunkings. Run with "full" to print each case.
const { StringDecoder } = require('string_decoder');
const { emit, attempt, digest, rnd } = require('./common.js');

const full = process.argv.includes('full');

emit('shape', [
  typeof StringDecoder, StringDecoder.name, StringDecoder.length,
  Object.getOwnPropertyNames(StringDecoder).join(),
  Object.getOwnPropertyNames(StringDecoder.prototype).join(),
  Object.keys(require('string_decoder')).join(),
]);
const descriptors = Object.getOwnPropertyDescriptors(StringDecoder.prototype);
for (const key of ['write', 'end', 'text', 'lastChar', 'lastNeed', 'lastTotal']) {
  const d = descriptors[key];
  emit('descriptor ' + key, [
    d.writable, d.enumerable, d.configurable,
    typeof d.get, d.get && d.get.name, typeof d.value, d.value && d.value.length,
  ]);
}

for (const name of [
  undefined, null, '', 'utf8', 'UTF8', 'utf-8', 'UTF-8', 'ucs2', 'UCS2', 'ucs-2',
  'utf16le', 'UTF16LE', 'utf-16le', 'UTF-16LE', 'base64', 'BASE64', 'base64url',
  'hex', 'HEX', 'latin1', 'binary', 'BINARY', 'ascii', 'ASCII', 'Ascii', 'buffer',
  'foo', 'utf7', 5, {}, 'utf-16', 'base-64',
]) {
  attempt('encoding ' + String(name), () => {
    const d = new StringDecoder(name);
    return [d.encoding, d.lastNeed, d.lastTotal, d.lastChar.length, Object.keys(d)];
  });
}

const d0 = new StringDecoder('utf8');
attempt('write string', () => d0.write('abc'));
attempt('write number', () => d0.write(5));
attempt('write null', () => d0.write(null));
attempt('write array', () => d0.write([1, 2]));
attempt('write Uint16Array', () => new StringDecoder('utf16le').write(new Uint16Array([0x61, 0x62])));
attempt('write DataView', () => d0.write(new DataView(new Uint8Array([0x68, 0x69]).buffer)));
attempt('write subarray', () => {
  const big = Buffer.from('xxhixx');
  return d0.write(big.subarray(2, 4));
});
attempt('write Float32Array', () => d0.write(new Float32Array([1])));
attempt('write without this', () => StringDecoder.prototype.write.call({}, Buffer.from('a')));
attempt('end undefined', () => new StringDecoder('utf8').end());
attempt('end string', () => new StringDecoder('utf8').end('x'));
attempt('end buf', () => new StringDecoder('utf8').end(Buffer.from([0xe2, 0x82])));
attempt('end twice', () => {
  const d = new StringDecoder('utf8');
  d.write(Buffer.from([0xe2]));
  return [d.end(), d.end(), d.lastNeed];
});
attempt('text', () => {
  const d = new StringDecoder('utf8');
  d.write(Buffer.from([0xe2]));
  return [d.text(Buffer.from('abcdef'), 2), d.lastNeed, d.lastTotal];
});
attempt('lastChar', () => {
  const d = new StringDecoder('utf8');
  d.write(Buffer.from([0xf0, 0x9f]));
  return [d.lastChar.length, Array.from(d.lastChar), d.lastNeed, d.lastTotal,
          d.lastChar.constructor.name];
});
attempt('lastChar is a live view', () => {
  const d = new StringDecoder('utf8');
  const view = d.lastChar;
  d.write(Buffer.from([0xe2, 0x82]));
  return [Array.from(view), view.buffer === d.lastChar.buffer];
});
attempt('inspect', () => require('util').inspect(new StringDecoder('utf16le')));
attempt('instance own keys', () => Reflect.ownKeys(new StringDecoder()).map(String));

const samples = {
  utf8ascii: [0x61, 0x62],
  utf8mix: Array.from(Buffer.from('aé€\u{1f600}b')),
  utf8emoji2: Array.from(Buffer.from('\u{1f600}\u{1f4a9}')),
  c3: [0xc3],
  e2: [0xe2],
  e282: [0xe2, 0x82],
  f09f98: [0xf0, 0x9f, 0x98],
  cont: [0x80],
  conts: [0x80, 0x80, 0x80, 0x80, 0x80],
  ff: [0xff, 0xfe, 0x61],
  badseq: [0xe2, 0x28, 0xa1],
  surrogate: [0xed, 0xa0, 0x80, 0x61],
  toobig: [0xf4, 0x90, 0x80, 0x80],
  overlong: [0xc0, 0x80, 0xe0, 0x80, 0x80],
  leadlead: [0xf0, 0x9f, 0xf0, 0x9f, 0x98, 0x80],
  trunc4: [0x61, 0xf0, 0x9f, 0x98, 0x80, 0xf0, 0x9f],
  longconts: [0xe2, 0x82, 0xac, 0x82, 0x82, 0x82],
  leadthencont: [0xc3, 0xa9, 0xa9, 0xc3],
  u16mix: Array.from(Buffer.from('a\u{1f600}b€', 'utf16le')),
  u16hi: [0x3d, 0xd8],
  u16hi2: [0x3d, 0xd8, 0x3d, 0xd8],
  u16pair: [0x3d, 0xd8, 0x4d, 0xdc],
  u16odd: [0x61, 0x00, 0x62],
  u16lo: [0x4d, 0xdc, 0x61, 0x00],
  u16hiend: [0x61, 0x00, 0x3d, 0xd8, 0x62],
  u16seq: [0x3d, 0xd8, 0x4d, 0xdc, 0x3d, 0xd8, 0x4d, 0xdc, 0x00],
  b64: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
  b64bin: [0xfb, 0xff, 0xbf, 0x3e, 0x3f, 0xff],
  empty: [],
};
const encodings = ['utf8', 'utf16le', 'ucs2', 'base64', 'base64url', 'hex', 'latin1', 'binary', 'ascii'];

function run(encoding, bytes, cuts) {
  const d = new StringDecoder(encoding);
  const buf = Buffer.from(bytes);
  const trace = [];
  let at = 0;
  for (const cut of cuts.concat([bytes.length])) {
    const out = d.write(buf.subarray(at, cut));
    trace.push(out, d.lastNeed, d.lastTotal, Buffer.from(d.lastChar).toString('hex'));
    at = cut;
  }
  trace.push(d.end(), d.lastNeed, d.lastTotal);
  return JSON.stringify(trace);
}

function fnv(text, h) {
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 0x01000193) >>> 0;
  return h;
}

for (const encoding of encodings) {
  for (const name of Object.keys(samples)) {
    const bytes = samples[name];
    let h = 0x811c9dc5;
    for (let i = 0; i <= bytes.length; i++) {
      for (let j = i; j <= bytes.length; j++) {
        const text = run(encoding, bytes, [i, j]);
        if (full) emit(`${encoding} ${name} ${i},${j}`, text);
        h = fnv(text, h);
      }
    }
    emit(`cuts ${encoding} ${name}`, h.toString(16));
  }
}

const alphabet = [
  0x00, 0x41, 0x7f, 0x80, 0x8f, 0xa9, 0xbf, 0xc2, 0xc3, 0xdf, 0xe0, 0xe2, 0xed, 0xef,
  0xf0, 0xf4, 0xf8, 0xff, 0x3d, 0xd8, 0x4d, 0xdc, 0xdb, 0xdf,
];
for (const encoding of ['utf8', 'utf16le', 'base64', 'base64url']) {
  for (let block = 0; block < 10; block++) {
    let h = 0x811c9dc5;
    for (let n = 0; n < 150; n++) {
      const bytes = [];
      for (let k = rnd(18); k > 0; k--) bytes.push(alphabet[rnd(alphabet.length)]);
      const cuts = [];
      for (let k = rnd(5); k > 0; k--) cuts.push(rnd(bytes.length + 1));
      cuts.sort((a, b) => a - b);
      const text = run(encoding, bytes, cuts);
      if (full) emit(`fuzz ${encoding} ${bytes.join(',')} | ${cuts.join(',')}`, text);
      h = fnv(text, h);
    }
    emit(`fuzz ${encoding} block ${block}`, h.toString(16));
  }
}

console.log(digest());
