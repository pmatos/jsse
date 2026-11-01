'use strict';
// Nothing here may keep the process alive (#1528): transferred streams that
// are never read, a channel nobody listens on, an unref'd listener. An unref'd
// timer is the hang detector: it fires only if something holds the loop open.
setTimeout(() => { console.log('HUNG'); process.exit(3); }, 8000).unref();
// transferred but never read: no handle keeps the process alive
const rs = new ReadableStream({ start(c) { c.enqueue(1); } });
const ws = new WritableStream();
const ts = new TransformStream();
const [rs2, ws2, ts2] = structuredClone([rs, ws, ts], { transfer: [rs, ws, ts] });
// a channel nobody listens on
const { port1, port2 } = new MessageChannel();
port1.postMessage('unseen');
// a listener that was unref'd
const c = new MessageChannel();
c.port2.on('message', () => console.log('late'));
c.port2.unref();
console.log('sync done');
