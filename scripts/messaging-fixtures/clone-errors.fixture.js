'use strict';
// structuredClone of platform objects (#1528): what refuses, and with which
// error, as Node reports it. Only `name`, `code` and `message` are printed.
function t(name, f) {
  try {
    console.log(name, 'ok', f());
  } catch (e) {
    console.log(name, e.name, e.code, JSON.stringify(e.message));
  }
}

t('readable', () => structuredClone(new ReadableStream()));
t('writable', () => structuredClone(new WritableStream()));
t('transform', () => structuredClone(new TransformStream()));
t('nested', () => structuredClone({ a: [new ReadableStream()] }));
t('port', () => structuredClone(new MessageChannel().port1));
t('listed elsewhere', () => structuredClone({ r: new ReadableStream() }, { transfer: [] }));
t('unlisted among listed', () => {
  const r = new ReadableStream();
  return structuredClone([r, new WritableStream()], { transfer: [r] });
});
t('not transferable', () => structuredClone(1, { transfer: [{}] }));
t('map', () => structuredClone(1, { transfer: [new Map()] }));
t('view', () => structuredClone(1, { transfer: [new Uint8Array(1)] }));
t('duplicate stream', () => {
  const r = new ReadableStream();
  return structuredClone([r], { transfer: [r, r] });
});
t('duplicate buffer', () => {
  const b = new ArrayBuffer(1);
  return structuredClone(1, { transfer: [b, b] });
});
t('locked readable', () => {
  const r = new ReadableStream();
  r.getReader();
  return structuredClone(r, { transfer: [r] });
});
t('locked writable', () => {
  const w = new WritableStream();
  w.getWriter();
  return structuredClone(w, { transfer: [w] });
});
t('locked transform side', () => {
  const s = new TransformStream();
  s.writable.getWriter();
  return structuredClone(s, { transfer: [s] });
});
t('transfer not in message', () => {
  const r = new ReadableStream();
  structuredClone(1, { transfer: [r] });
  return r.locked;
});
t('transfer readable', () => {
  const r = new ReadableStream();
  const c = structuredClone(r, { transfer: [r] });
  return [c instanceof ReadableStream, c === r, r.locked, c.locked, c.constructor === ReadableStream];
});
t('transfer transform', () => {
  const s = new TransformStream();
  const c = structuredClone(s, { transfer: [s] });
  return [c instanceof TransformStream, s.readable.locked, c.readable.locked, c.writable.locked];
});
t('port transfer', () => {
  const { port1 } = new MessageChannel();
  const c = structuredClone(port1, { transfer: [port1] });
  return [c === port1, c.constructor.name, c instanceof MessagePort];
});
t('closed port', () => {
  const { port1 } = new MessageChannel();
  port1.close();
  return structuredClone(port1, { transfer: [port1] });
});
t('buffers still work', () => {
  const b = new ArrayBuffer(4);
  const c = structuredClone({ b }, { transfer: [b] });
  return [b.byteLength, c.b.byteLength];
});
