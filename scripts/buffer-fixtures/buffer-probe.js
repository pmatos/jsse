'use strict';

// `require("buffer")` differential probe: module shape, constants,
// INSPECT_MAX_BYTES, isUtf8/isAscii, transcode, atob/btoa, Blob and File.
const buffer = require('buffer');
const { emit, attempt, digest, rnd } = require('./common.js');

emit('same module', [buffer === require('node:buffer'), buffer.Buffer === Buffer,
                     buffer.atob === atob, buffer.btoa === btoa]);
emit('keys', Object.keys(buffer));
for (const [key, d] of Object.entries(Object.getOwnPropertyDescriptors(buffer))) {
  emit('descriptor ' + key, [
    d.writable, d.enumerable, d.configurable, typeof d.get, d.get && d.get.name,
    d.set && d.set.name, typeof d.value === 'function' ? [d.value.name, d.value.length] : '',
  ]);
}
emit('constants', [buffer.constants, Object.isFrozen(buffer.constants),
                   Object.getOwnPropertyDescriptors(buffer.constants)]);
emit('limits', [buffer.kMaxLength, buffer.kStringMaxLength, buffer.INSPECT_MAX_BYTES]);
emit('SlowBuffer', [typeof buffer.SlowBuffer, 'SlowBuffer' in buffer]);

attempt('inspect default', () => require('util').inspect(Buffer.alloc(60, 1)));
buffer.INSPECT_MAX_BYTES = 3;
attempt('inspect 3', () => [buffer.INSPECT_MAX_BYTES, require('util').inspect(Buffer.from('abcdef')),
                            require('util').inspect(Buffer.from('abc'))]);
buffer.INSPECT_MAX_BYTES = 0;
attempt('inspect 0', () => require('util').inspect(Buffer.from('abc')));
for (const bad of [-1, 'x', NaN, null, undefined, {}]) {
  attempt('INSPECT_MAX_BYTES = ' + String(bad), () => {
    buffer.INSPECT_MAX_BYTES = bad;
    return buffer.INSPECT_MAX_BYTES;
  });
}
buffer.INSPECT_MAX_BYTES = 50;
attempt('inspect restored', () => require('util').inspect(Buffer.alloc(52, 255)));

const utf8Cases = {
  empty: [], ascii: [0x61, 0x7f], e: [0xc3, 0xa9], euro: [0xe2, 0x82, 0xac],
  emoji: [0xf0, 0x9f, 0x98, 0x80], trunc2: [0xc3], trunc3: [0xe2, 0x82],
  trunc4: [0xf0, 0x9f, 0x98], cont: [0x80], c0: [0xc0, 0x80], c1: [0xc1, 0xbf],
  c2: [0xc2, 0x80], df: [0xdf, 0xbf], e080: [0xe0, 0x80, 0x80], e0a0: [0xe0, 0xa0, 0x80],
  surrogate: [0xed, 0xa0, 0x80], edbf: [0xed, 0x9f, 0xbf], ee: [0xee, 0x80, 0x80],
  f080: [0xf0, 0x80, 0x80, 0x80], f090: [0xf0, 0x90, 0x80, 0x80],
  f48f: [0xf4, 0x8f, 0xbf, 0xbf], f490: [0xf4, 0x90, 0x80, 0x80], f5: [0xf5, 0x80, 0x80, 0x80],
  ff: [0xff], badcont: [0xe2, 0x28, 0xa1], tail: [0x61, 0xe2], mix: [0x61, 0xc3, 0xa9, 0x62],
};
for (const [name, bytes] of Object.entries(utf8Cases)) {
  const buf = Buffer.from(bytes);
  emit('isUtf8/isAscii ' + name, [buffer.isUtf8(buf), buffer.isAscii(buf),
    buffer.isUtf8(new Uint8Array(bytes)), buffer.isAscii(new Uint8Array(bytes).buffer),
    buffer.isUtf8(new Uint8Array(bytes).buffer)]);
}
attempt('isUtf8 view window', () => {
  const big = Buffer.from([0xc3, 0xa9, 0xff, 0xc3]);
  return [buffer.isUtf8(big.subarray(0, 2)), buffer.isUtf8(big.subarray(2)), buffer.isUtf8(big.subarray(1, 3))];
});
attempt('isUtf8 Uint16Array', () => [buffer.isUtf8(new Uint16Array([0xa9c3])), buffer.isAscii(new Uint16Array([0x80]))]);
attempt('isUtf8 shared', () => buffer.isUtf8(new SharedArrayBuffer(2)));
for (const bad of [undefined, null, 'x', 5, {}, [1], new DataView(new ArrayBuffer(1))]) {
  attempt('isUtf8 ' + Object.prototype.toString.call(bad), () => buffer.isUtf8(bad));
  attempt('isAscii ' + Object.prototype.toString.call(bad), () => buffer.isAscii(bad));
}

const bytesets = {
  ascii: Array.from(Buffer.from('hello')),
  latin: [0x61, 0xe9, 0xff, 0x80, 0x7f],
  utf8: Array.from(Buffer.from('aé€\u{1f600}Ā')),
  u16: Array.from(Buffer.from('aé€\u{1f600}Ā', 'utf16le')),
  badutf8: [0x61, 0xff, 0xc3, 0x28, 0xe2, 0x82],
  loneHigh: [0x3d, 0xd8, 0x61, 0x00],
  loneLow: [0x4d, 0xdc, 0x61, 0x00],
  odd16: [0x61, 0x00, 0x62],
  oddHigh: [0x3d, 0xd8, 0x62],
  fff8: [0xf8, 0xff, 0x61, 0x00],
  fff8utf8: Array.from(Buffer.from('a\uFFF8\uFFF9b')),
  one: [0x41],
};
const names = ['utf8', 'utf-8', 'ucs2', 'ucs-2', 'utf16le', 'UTF-16LE', 'latin1', 'binary', 'ascii',
               'UTF8', 'hex', 'base64', 'base64url', 'foo', undefined];
let n = 0;
for (const [setName, bytes] of Object.entries(bytesets)) {
  for (const from of names) {
    for (const to of names) {
      n++;
      attempt(`transcode ${setName} ${from} ${to}`, () => {
        const out = buffer.transcode(Buffer.from(bytes), from, to);
        return [out.constructor.name, out.toString('hex')];
      });
    }
  }
}
emit('transcode count', n);
attempt('transcode empty', () => {
  const out = buffer.transcode(Buffer.alloc(0), 'foo', 'bar');
  return [out.constructor.name, out.length];
});
attempt('transcode Uint8Array', () => buffer.transcode(new Uint8Array([0x61]), 'utf8', 'ucs2').toString('hex'));
for (const bad of [undefined, 'x', [1], new ArrayBuffer(2), new Uint16Array(1)]) {
  attempt('transcode source ' + Object.prototype.toString.call(bad), () => buffer.transcode(bad, 'utf8', 'ucs2'));
}
attempt('transcode error props', () => {
  try {
    buffer.transcode(Buffer.from('a'), 'hex', 'utf8');
  } catch (e) {
    return [e.name, e.code, e.errno, Object.keys(e), e instanceof Error];
  }
});

// The global atob/btoa are the host's, whose error messages differ from Node's.
function kind(fn) {
  try {
    return fn();
  } catch (e) {
    return 'throws ' + e.name + ' ' + e.code;
  }
}
for (const input of ['', 'aGk=', 'aGk', ' a G k = ', 'a', 'a===', '\u00ff', '+/+/', '-_-_', 'aGVsbG8gd29ybGQ=']) {
  emit('atob ' + JSON.stringify(input), kind(() => buffer.atob(input)));
}
for (const input of ['', 'hi', 'h', '\u00ff', '\u0100', 'hello world']) {
  emit('btoa ' + JSON.stringify(input), kind(() => buffer.btoa(input)));
}
emit('atob()', kind(() => buffer.atob()).replace(/ \w+$/, ''));
emit('btoa()', kind(() => buffer.btoa()).replace(/ \w+$/, ''));

const { Blob, File } = buffer;
emit('Blob/File', [Blob.name, Blob.length, File.name, File.length, Object.getPrototypeOf(File) === Blob,
                   Object.getOwnPropertyNames(Blob.prototype).filter((k) => !/stream/i.test(k)).join(), Object.getOwnPropertyNames(File.prototype).join()]);
attempt('Blob(string)', () => new Blob('x'));
attempt('Blob(null)', () => new Blob(null));
attempt('Blob(opts 5)', () => new Blob([], 5));
attempt('Blob(opts null)', () => new Blob([], null).size);
attempt('Blob endings', () => new Blob([], { endings: 'x' }));
attempt('Blob endings undefined', () => new Blob([], { endings: undefined }).size);
attempt('Blob sizes', () => [new Blob().size, new Blob([{}]).size, new Blob([1, null, undefined]).size,
                              new Blob(['€', new Uint8Array(3), new Uint16Array(2), new ArrayBuffer(5)]).size,
                              new Blob(new Set(['ab', 'c'])).size, new Blob(['\ud800']).size]);
attempt('Blob native endings', () => [new Blob(['a\nb\r\nc\rd'], { endings: 'native' }).size,
                                      new Blob(['a\nb\r\nc\rd']).size]);
attempt('Blob types', () => [new Blob([], { type: 'Text/Plain' }).type, new Blob([], { type: 'é' }).type,
                             new Blob([], { type: 3 }).type, new Blob([], { type: null }).type,
                             new Blob([], { type: 'a\tb' }).type, new Blob([], {}).type]);
attempt('Blob toString', () => [String(new Blob()), Object.prototype.toString.call(new File([], 'f')),
                                Object.keys(new Blob(['a'])), JSON.stringify(new Blob(['a']))]);
attempt('Blob inspect', () => require('util').inspect(new Blob(['abc'], { type: 'a/b' })));
attempt('Blob slice', () => {
  const b = new Blob(['hello', new Uint8Array([33]), new Blob(['!!'])]);
  return [b.slice().size, b.slice(-3).size, b.slice(1, 4, 'A/B').type, b.slice(2, 1).size,
          b.slice(NaN).size, b.slice(0, Infinity).size, b.slice(-Infinity, 2).size,
          b.slice(100).size, b.slice('1', '3').size, b.slice(1.9, 3.9).size, b.slice(undefined, undefined, undefined).type,
          b.slice(0, 1) instanceof Blob];
});
attempt('Blob slice of File', () => {
  const f = new File(['abc'], 'n.txt', { type: 'x/y' });
  const s = f.slice(1);
  return [s.constructor.name, s.size, s.type, 'name' in s];
});
attempt('Blob bad this', () => Object.getOwnPropertyDescriptor(Blob.prototype, 'size').get.call({}));
attempt('Blob bad this method', () => Blob.prototype.slice.call({}));
attempt('File args', () => new File([]));
attempt('File args 0', () => new File());
attempt('File fields', () => {
  const f = new File(['a', 'bc'], 5, { lastModified: 7 });
  return [f.name, f.lastModified, f.size, f.type, f instanceof Blob, Object.keys(f), String(f)];
});
attempt('File lastModified', () => [new File([], 'x', { lastModified: '7' }).lastModified,
  new File([], 'x', { lastModified: { valueOf() { return 9; } } }).lastModified,
  new File([], 'x', { lastModified: NaN }).lastModified, new File([], 'x', { lastModified: 1.9 }).lastModified,
  new File([], 'x', { lastModified: -1.9 }).lastModified, new File([], 'x', { lastModified: Infinity }).lastModified,
  new File([], 'x', { lastModified: null }).lastModified, typeof new File([], 'x').lastModified,
  Math.abs(new File([], 'x').lastModified - Date.now()) < 5000]);
attempt('File name coercion', () => [new File([], undefined).name, new File([], null).name, new File([], { toString() { return 'o'; } }).name]);
attempt('File symbol name', () => new File([], Symbol()));
attempt('File inspect', () => require('util').inspect(new File(['ab'], 'f.txt', { type: 'a/b', lastModified: 5 })));
attempt('File bad this', () => Object.getOwnPropertyDescriptor(File.prototype, 'name').get.call(new Blob()));
attempt('File primitive this', () => Object.getOwnPropertyDescriptor(File.prototype, 'lastModified').get.call(undefined));
attempt('Blob iterator reads', () => {
  let reads = 0;
  const p = new Proxy(['a'], { get(t, k, r) { if (k === Symbol.iterator) reads++; return Reflect.get(t, k, r); } });
  return [new Blob(p).size, reads];
});
attempt('Blob from Blob/File', () => new Blob([new Blob(['ab']), new File(['cd'], 'f')]).size);
attempt('resolveObjectURL', () => [buffer.resolveObjectURL('blob:nodedata:nope'), buffer.resolveObjectURL('x'),
                                   buffer.resolveObjectURL('http://a/'), buffer.resolveObjectURL.length]);

(async () => {
  const b = new Blob(['hello', new Uint8Array([0xe2, 0x82, 0xac]), new Blob(['!'])], { type: 'T/x' });
  emit('async text', await b.text());
  emit('async bytes', [await b.bytes(), (await b.bytes()).constructor.name]);
  const ab = await b.arrayBuffer();
  emit('async arrayBuffer', [ab.constructor.name, ab.byteLength, Array.from(new Uint8Array(ab))]);
  emit('async separate copies', [(await b.arrayBuffer()) !== (await b.arrayBuffer())]);
  emit('async slice text', await b.slice(1, 6).text());
  emit('async invalid utf8', await new Blob([new Uint8Array([0xff, 0x61])]).text());
  emit('async bom', JSON.stringify(await new Blob([new Uint8Array([0xef, 0xbb, 0xbf, 0x61])]).text()));
  emit('async file', await new File(['x', 'y'], 'n').text());
  emit('async mutated source', await (async () => {
    const src = new Uint8Array([97, 98]);
    const blob = new Blob([src]);
    src[0] = 122;
    return blob.text();
  })());
  try {
    await Blob.prototype.text.call({});
  } catch (e) {
    emit('async text bad this', [e.name, e.code, e.message]);
  }
  console.log(digest());
})();
