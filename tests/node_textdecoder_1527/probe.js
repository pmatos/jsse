// TextDecoder under --node against Node v26.8.2 (#1527): constructor and decode
// argument validation, error codes and messages, fatal/streaming/BOM state,
// class shape and inspect output. expected.txt is this script's output on Node:
//   node tests/node_textdecoder_1527/probe.js > tests/node_textdecoder_1527/expected.txt
function t(name, fn) {
  try {
    const r = fn();
    console.log(name, '=>', typeof r === 'string' ? JSON.stringify(r) : r);
  } catch (e) {
    console.log(name, 'THROWS', e && e.constructor && e.constructor.name, e && e.name, e && e.code, JSON.stringify(e && e.message));
  }
}
const u8 = (...a) => new Uint8Array(a);
t('badlabel', () => new TextDecoder('nope'));
t('badlabel-sym', () => new TextDecoder(Symbol('x')));
t('label-undefined', () => new TextDecoder(undefined).encoding);
t('label-null', () => new TextDecoder(null));
t('label-empty', () => new TextDecoder(''));
t('label-object', () => new TextDecoder({ toString() { return 'utf-8'; } }).encoding);
t('opts-number', () => new TextDecoder('utf-8', 1));
t('opts-string', () => new TextDecoder('utf-8', 'x'));
t('opts-null', () => new TextDecoder('utf-8', null).fatal);
t('opts-fn', () => new TextDecoder('utf-8', function () {}).fatal);
t('opts-arr', () => new TextDecoder('utf-8', []).fatal);
t('decode-str', () => new TextDecoder().decode('abc'));
t('decode-num', () => new TextDecoder().decode(1));
t('decode-null', () => new TextDecoder().decode(null));
t('decode-obj', () => new TextDecoder().decode({}));
t('decode-undefined', () => new TextDecoder().decode(undefined));
t('decode-ab', () => new TextDecoder().decode(new Uint8Array([104, 105]).buffer));
t('decode-dv', () => new TextDecoder().decode(new DataView(new Uint8Array([104, 105]).buffer)));
t('decode-u16', () => new TextDecoder().decode(new Uint16Array([0x6968])));
t('decode-sab', () => new TextDecoder().decode(new SharedArrayBuffer(2)));
t('decode-buffer', () => new TextDecoder().decode(Buffer.from('hi')));
t('decode-opts-null', () => new TextDecoder().decode(u8(104), null));
t('decode-opts-num', () => new TextDecoder().decode(u8(104), 1));
t('decode-opts-str', () => new TextDecoder().decode(u8(104), 'x'));
t('decode-opts-fn', () => new TextDecoder().decode(u8(104), () => {}));
t('decode-this', () => TextDecoder.prototype.decode.call({}, u8(1)));
t('encoding-this', () => Object.getOwnPropertyDescriptor(TextDecoder.prototype, 'encoding').get.call({}));
t('fatal-this', () => Object.getOwnPropertyDescriptor(TextDecoder.prototype, 'fatal').get.call({}));
t('ignoreBOM-this', () => Object.getOwnPropertyDescriptor(TextDecoder.prototype, 'ignoreBOM').get.call({}));
t('call-without-new', () => TextDecoder());
t('tag', () => Object.prototype.toString.call(new TextDecoder()));
t('keys', () => Object.getOwnPropertyNames(TextDecoder.prototype).join());
t('own-syms', () => Object.getOwnPropertySymbols(TextDecoder.prototype).map(String).join());
t('ownprops', () => Reflect.ownKeys(new TextDecoder()).map(String).join());
t('length', () => TextDecoder.length + ' ' + TextDecoder.prototype.decode.length);
t('name', () => TextDecoder.name + ' ' + TextDecoder.prototype.decode.name);
t('inspect', () => require('util').inspect(new TextDecoder()));
t('inspect-fatal', () => require('util').inspect(new TextDecoder('utf-16le', { fatal: true, ignoreBOM: true })));
t('inspect-hidden', () => require('util').inspect(new TextDecoder(), { showHidden: true }));
t('inspect-depth', () => require('util').inspect(new TextDecoder(), { depth: -1 }));
t('inspect-class', () => require('util').inspect(TextDecoder));
t('desc-enc', () => { const d = Object.getOwnPropertyDescriptor(TextDecoder.prototype, 'encoding'); return [typeof d.get, d.set, d.enumerable, d.configurable].join(); });
t('desc-fatal', () => { const d = Object.getOwnPropertyDescriptor(TextDecoder.prototype, 'fatal'); return [typeof d.get, d.set, d.enumerable, d.configurable].join(); });
t('desc-global', () => { const d = Object.getOwnPropertyDescriptor(globalThis, 'TextDecoder'); return [typeof d.value, d.writable, d.enumerable, d.configurable].join(); });
t('util-same', () => require('util').TextDecoder === TextDecoder);
// fatal flush
t('fatal-utf8-C0', () => new TextDecoder('utf-8', { fatal: true }).decode(u8(0xc0)));
t('fatal-le-1', () => new TextDecoder('utf-16le', { fatal: true }).decode(u8(0)));
t('fatal-be-1', () => new TextDecoder('utf-16be', { fatal: true }).decode(u8(0)));
t('fatal-1252', () => new TextDecoder('windows-1252', { fatal: true }).decode(u8(0x81)));
t('fatal-le-stream-even-odd', () => { const d = new TextDecoder('utf-16le', { fatal: true }); d.decode(u8(0, 0), { stream: true }); return d.decode(u8(0)); });
t('fatal-le-stream-odd-even', () => { const d = new TextDecoder('utf-16le', { fatal: true }); d.decode(u8(0), { stream: true }); return d.decode(u8(0, 0)); });
t('fatal-le-stream-odd-odd', () => { const d = new TextDecoder('utf-16le', { fatal: true }); d.decode(u8(0x41), { stream: true }); return d.decode(u8(0)); });
t('fatal-le-stream-odd-flush', () => { const d = new TextDecoder('utf-16le', { fatal: true }); d.decode(u8(0x41), { stream: true }); return d.decode(); });
t('fatal-le-stream-flush-then-reuse', () => { const d = new TextDecoder('utf-16le', { fatal: true }); d.decode(u8(0x41), { stream: true }); try { d.decode(); } catch (e) {} return d.decode(u8(0x41, 0)); });
t('fatal-le-lone-hi-stream-flush', () => { const d = new TextDecoder('utf-16le', { fatal: true }); d.decode(u8(0x3d, 0xd8), { stream: true }); return d.decode(); });
t('fatal-le-lone-hi-stream-then-nonlo', () => { const d = new TextDecoder('utf-16le', { fatal: true }); d.decode(u8(0x3d, 0xd8), { stream: true }); return d.decode(u8(0x41, 0)); });
t('nonfatal-le-lone-hi-flush', () => { const d = new TextDecoder('utf-16le'); d.decode(u8(0x3d, 0xd8), { stream: true }); return d.decode(); });
t('nonfatal-le-odd-flush', () => { const d = new TextDecoder('utf-16le'); d.decode(u8(0x41), { stream: true }); return d.decode(); });
t('nonfatal-le-odd-even', () => { const d = new TextDecoder('utf-16le'); d.decode(u8(0x41), { stream: true }); return d.decode(u8(0, 0x42, 0)); });
t('fatal-le-split-pair', () => { const d = new TextDecoder('utf-16le', { fatal: true }); d.decode(u8(0x3d), { stream: true }); d.decode(u8(0xd8, 0x4d), { stream: true }); return d.decode(u8(0xdc)); });
t('fatal-be-stream-flush', () => { const d = new TextDecoder('utf-16be', { fatal: true }); d.decode(u8(0), { stream: true }); return d.decode(); });
t('fatal-utf8-trunc-flush', () => { const d = new TextDecoder('utf-8', { fatal: true }); d.decode(u8(0xe2, 0x82), { stream: true }); return d.decode(); });
t('fatal-utf8-trunc-flush2', () => { const d = new TextDecoder('utf-8', { fatal: true }); d.decode(u8(0xe2, 0x82), { stream: true }); return d.decode(u8(0x41)); });
t('nonfatal-utf8-trunc-flush', () => { const d = new TextDecoder('utf-8'); d.decode(u8(0xe2, 0x82), { stream: true }); return d.decode(); });
t('fatal-err-props', () => { try { new TextDecoder('utf-8', { fatal: true }).decode(u8(0xff)); } catch (e) { return [Object.keys(e).join(), e.name, e.code, e.constructor.name, e.stack.split('\n')[0], e instanceof TypeError].join(' | '); } });
t('badlabel-props', () => { try { new TextDecoder('nope'); } catch (e) { return [Object.keys(e).join(), e.name, e.code, e.constructor.name, e.stack.split('\n')[0], e instanceof RangeError].join(' | '); } });
t('invalid-arg-props', () => { try { new TextDecoder().decode(1); } catch (e) { return [Object.keys(e).join(), e.name, e.code, e.constructor.name, e.stack.split('\n')[0]].join(' | '); } });
t('labels-ws', () => new TextDecoder('  UTF-8\n').encoding);
t('labels-utf16', () => new TextDecoder('utf-16').encoding);
t('labels-ascii', () => new TextDecoder('ascii').encoding);
t('labels-latin1', () => new TextDecoder('latin1').encoding);
t('labels-utf16le-dash', () => new TextDecoder('UTF-16LE').encoding);
t('bom-strip', () => new TextDecoder().decode(u8(0xef, 0xbb, 0xbf, 0x41)).length);
t('bom-keep', () => new TextDecoder('utf-8', { ignoreBOM: true }).decode(u8(0xef, 0xbb, 0xbf, 0x41)).length);
t('bom-16-strip', () => new TextDecoder('utf-16le').decode(u8(0xff, 0xfe, 0x41, 0)).length);
t('stream-reuse', () => { const d = new TextDecoder(); const a = d.decode(u8(0xe2, 0x82), { stream: true }); const b = d.decode(u8(0xac)); return a + '|' + b; });
t('undefined-fatal-opt', () => new TextDecoder('utf-8', { fatal: 'yes', ignoreBOM: 1 }).fatal);
t('toString', () => String(new TextDecoder()));
t('ctor-proto', () => Object.getPrototypeOf(TextDecoder) === Function.prototype);
t('subclass', () => { class X extends TextDecoder {} return new X().encoding; });
t('decode-detached', () => { const b = new ArrayBuffer(4); structuredClone(b, { transfer: [b] }); return new TextDecoder().decode(b); });

const cat = (...x) => { const o = []; x.forEach((v) => o.push(...v)); return new Uint8Array(o); };
for (const enc of ['utf-8', 'utf-16le', 'utf-16be']) {
  const bom = enc === 'utf-8' ? u8(0xef, 0xbb, 0xbf) : enc === 'utf-16le' ? u8(0xff, 0xfe) : u8(0xfe, 0xff);
  const bad = enc === 'utf-8' ? u8(0xff) : enc === 'utf-16le' ? u8(0x00, 0xdc) : u8(0xdc, 0x00);
  const a = enc === 'utf-8' ? u8(0x41) : enc === 'utf-16le' ? u8(0x41, 0) : u8(0, 0x41);
  t(enc + ' err-mid-stream-after-bom-then-bom', () => { const d = new TextDecoder(enc, { fatal: true }); d.decode(cat(bom, a), { stream: true }); try { d.decode(bad, { stream: true }); } catch {} return d.decode(cat(bom, a)).length; });
  t(enc + ' err-first-stream-then-bom', () => { const d = new TextDecoder(enc, { fatal: true }); try { d.decode(bad, { stream: true }); } catch {} return d.decode(cat(bom, a)).length; });
  t(enc + ' err-flush-then-bom', () => { const d = new TextDecoder(enc, { fatal: true }); d.decode(cat(bom, a), { stream: true }); try { d.decode(bad); } catch {} return d.decode(cat(bom, a)).length; });
  t(enc + ' stream-bom-twice', () => { const d = new TextDecoder(enc); const x = d.decode(cat(bom, a), { stream: true }); const y = d.decode(cat(bom, a)); return [x.length, y.length]; });
  t(enc + ' empty-stream-then-bom', () => { const d = new TextDecoder(enc); d.decode(u8(), { stream: true }); return d.decode(cat(bom, a)).length; });
  t(enc + ' stream-string', () => new TextDecoder(enc).decode('abc', { stream: true }));
  t(enc + ' stream-null', () => new TextDecoder(enc).decode(null, { stream: true }));
  t(enc + ' nonstream-str', () => new TextDecoder(enc).decode('abc'));
  t(enc + ' sab', () => new TextDecoder(enc).decode(new SharedArrayBuffer(2)).length);
  t(enc + ' detached-stream', () => { const b = new ArrayBuffer(4); structuredClone(b, { transfer: [b] }); return new TextDecoder(enc).decode(b, { stream: true }); });
}
t('1252 str', () => new TextDecoder('windows-1252').decode('abc'));
t('1252 num', () => new TextDecoder('windows-1252').decode(1));
t('1252 sab', () => new TextDecoder('windows-1252').decode(new SharedArrayBuffer(2)).length);
t('1252 stream', () => new TextDecoder('windows-1252').decode(new Uint8Array([0x80]), { stream: true }));
t('1252 euro', () => new TextDecoder('windows-1252').decode(new Uint8Array([0x80])));
t('1252 fatal e9', () => new TextDecoder('windows-1252', { fatal: true }).decode(new Uint8Array([0xe9])));
for (const [label, args] of [['utf8 stream 1', [1, { stream: true }]], ['utf8 {}', [{}]], ['utf8 null stream', [null, { stream: true }]]]) {
  try { new TextDecoder().decode(...args); } catch (e) { console.log(label, e.name, e.code, e.message, Object.keys(e), e.stack.split('\n')[0]); }
}
for (const enc of ['utf-16le', 'windows-1252']) {
  try { new TextDecoder(enc).decode({}); } catch (e) { console.log(enc, e.name, e.code, e.message, Object.keys(e), e.stack.split('\n')[0]); }
}
try { new TextDecoder('xx'); } catch (e) { console.log(Object.keys(e), e.stack.split('\n')[0]); }
console.log(Object.getOwnPropertyNames(new TextDecoder()).length);
const w = new TextDecoder('utf-8');
console.log(JSON.stringify(w.decode(u8(0xf0, 0x9f), { stream: true })), JSON.stringify(w.decode(u8(0x98, 0x80), { stream: true })), JSON.stringify(w.decode()));
console.log(JSON.stringify(new TextDecoder('utf-16le').decode(u8(0x3d, 0xd8, 0x4d, 0xdc, 0x41))));
t('proxy-u8', () => new TextDecoder().decode(new Proxy(new Uint8Array(2), {})));
t('dataview-window', () => new TextDecoder().decode(new DataView(new Uint8Array([0x41, 0x42, 0x43]).buffer, 1, 1)));
t('float64-view', () => new TextDecoder().decode(new Float64Array([0])).length);
t('subclass-buffer-getter', () => { class V extends Uint16Array { get buffer() { throw new Error('boom'); } } return new TextDecoder().decode(new V(2)).length; });
t('pending-is-a-copy', () => { const d = new TextDecoder(); const b = new Uint8Array([0xe2, 0x82]); d.decode(b, { stream: true }); b[0] = 0x41; return d.decode(new Uint8Array([0xac])); });
t('subarray-window', () => new TextDecoder().decode(new Uint8Array([0x41, 0x42, 0x43]).subarray(1, 2)));
t('buffer-pool', () => new TextDecoder().decode(Buffer.from('xyz').subarray(1)));
t('latin1-stream-keeps-state', () => { const d = new TextDecoder('latin1'); return d.decode(u8(0xe9), { stream: true }) + d.decode(u8(0xe9)); });
t('label-nbsp', () => new TextDecoder(' utf-8'));
t('label-proto', () => new TextDecoder('__proto__'));
