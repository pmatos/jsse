'use strict';
// Web streams transferred with structuredClone and over a MessageChannel
// (#1528): chunks, errors, cancel, abort and close cross the channel as in
// Node. An unref'd timer is the hang detector.
const log = (...a) => console.log(...a);
setTimeout(() => { log('HUNG'); process.exit(3); }, 8000).unref();
async function main() {
  // ReadableStream chunks
  const rs = new ReadableStream({
    start(c) { c.enqueue('a'); c.enqueue({ n: 1, d: new Date(0) }); c.enqueue(new Uint8Array([1, 2, 3])); c.close(); },
  });
  const rs2 = structuredClone(rs, { transfer: [rs] });
  log('rs locked orig', rs.locked, 'clone', rs2.locked, rs2 instanceof ReadableStream, rs2.constructor === ReadableStream);
  const out = [];
  for await (const v of rs2) out.push(v);
  log('read', out);

  // errors
  const rse = new ReadableStream({ start(c) { c.enqueue(1); c.error(new Error('boom')); } });
  const rse2 = structuredClone(rse, { transfer: [rse] });
  const r = rse2.getReader();
  try { log(await r.read()); log(await r.read()); } catch (e) { log('err', e.name, e.message); }

  // cancel propagates
  let cancelled;
  const rsc = new ReadableStream({ pull(c) { c.enqueue('x'); }, cancel(reason) { cancelled = reason; } }, { highWaterMark: 0 });
  const rsc2 = structuredClone(rsc, { transfer: [rsc] });
  const rc = rsc2.getReader();
  log('first', await rc.read());
  await rc.cancel('why');
  await new Promise((res) => setTimeout(res, 100));
  log('cancelled with', cancelled);

  // WritableStream
  const got = [];
  const ws = new WritableStream({ write(c) { got.push(c); }, close() { got.push('closed'); } });
  const ws2 = structuredClone(ws, { transfer: [ws] });
  const w = ws2.getWriter();
  await w.write('one'); await w.write({ two: 2 });
  await w.close();
  await new Promise((res) => setTimeout(res, 100));
  log('writes', got, ws2 instanceof WritableStream);

  // WritableStream abort
  let aborted;
  const wsa = new WritableStream({ abort(r) { aborted = r; } });
  const wsa2 = structuredClone(wsa, { transfer: [wsa] });
  await wsa2.getWriter().abort('stop');
  await new Promise((res) => setTimeout(res, 100));
  log('aborted with', aborted);

  // TransformStream
  const ts = new TransformStream({ transform(c, ctl) { ctl.enqueue(String(c).toUpperCase()); } });
  const ts2 = structuredClone(ts, { transfer: [ts] });
  const tw = ts2.writable.getWriter();
  tw.write('abc'); tw.write('def'); tw.close();
  const tr = [];
  for await (const v of ts2.readable) tr.push(v);
  log('transform', tr, ts2 instanceof TransformStream);

  // streams inside an object, and several
  const a = new ReadableStream({ start(c) { c.enqueue(1); c.close(); } });
  const b = new ReadableStream({ start(c) { c.enqueue(2); c.close(); } });
  const o = structuredClone({ a, b, again: a, n: 5 }, { transfer: [a, b] });
  log('object', Object.keys(o), o.a === o.again, o.a !== o.b);
  log(await o.a.getReader().read(), await o.b.getReader().read());

  // over a MessageChannel
  const { port1, port2 } = new MessageChannel();
  const src = new ReadableStream({ start(c) { c.enqueue('m1'); c.enqueue('m2'); c.close(); } });
  port1.postMessage({ stream: src }, [src]);
  await new Promise((res) => {
    port2.onmessage = async ({ data }) => {
      const chunks = [];
      for await (const v of data.stream) chunks.push(v);
      log('via channel', chunks);
      port2.close();
      res();
    };
  });

  // locked / bad
  const l = new ReadableStream(); l.getReader();
  try { structuredClone(l, { transfer: [l] }); } catch (e) { log(e.name, e.message); }
  const n = new ReadableStream();
  try { structuredClone({ n }); } catch (e) { log(e.name, e.message); }
  log('done');
}
main().catch((e) => log('FAIL', e));
