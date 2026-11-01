'use strict';
// MessageChannel / MessagePort (#1528): delivery, close, ref and transfer
// semantics, as Node prints them. An unref'd timer is the hang detector: it
// only fires when something keeps the loop alive past the end of the script.
const { receiveMessageOnPort } = require('worker_threads');

const log = (...a) => console.log(...a);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
setTimeout(() => { log('HUNG'); process.exit(3); }, 8000).unref();

(async () => {
  log('shape', Object.keys(new MessageChannel()), typeof MessagePort, typeof MessageEvent);
  try { new MessagePort(); } catch (e) { log(e.name, e.code, e.message); }

  {
    // Messages sent before close arrive, then 'close' on both ends.
    const { port1, port2 } = new MessageChannel();
    const seen = [];
    port2.on('message', (m) => seen.push('msg ' + m));
    port2.on('close', () => seen.push('close2'));
    port1.on('close', () => seen.push('close1'));
    const closed = Promise.all([
      new Promise((resolve) => port1.once('close', resolve)),
      new Promise((resolve) => port2.once('close', resolve)),
    ]);
    port1.postMessage('a');
    port1.postMessage('b');
    port1.close();
    await closed;
    log('close order', seen);
    log('post after close', port1.postMessage('x'), port2.postMessage('y'));
  }
  {
    const { port1, port2 } = new MessageChannel();
    await new Promise((resolve) => {
      port1.close(() => { log('close callback'); resolve(); });
      port1.close();
    });
    log('hasRef when closed', port1.hasRef(), port2.hasRef());
  }
  {
    // Queued until somebody listens.
    const { port1, port2 } = new MessageChannel();
    port1.postMessage(1);
    port1.postMessage(2);
    await sleep(50);
    const seen = [];
    const both = new Promise((resolve) => {
      port2.on('message', (m) => { seen.push(m); if (seen.length === 2) resolve(); });
    });
    await both;
    log('late listener', seen);
    port2.close();
  }
  {
    // addEventListener starts the port; start() again changes nothing.
    const { port1, port2 } = new MessageChannel();
    const seen = [];
    const both = new Promise((resolve) => {
      port2.addEventListener('message', (e) => { seen.push(e.data); if (seen.length === 2) resolve(); });
    });
    port1.postMessage('p');
    port2.start();
    port1.postMessage('q');
    await both;
    log('addEventListener', seen);
    port2.close();
  }
  {
    const { port1, port2 } = new MessageChannel();
    log('refs', port1.hasRef(), port2.hasRef());
    port2.onmessage = () => {};
    log('after onmessage', port2.hasRef());
    port2.unref();
    log('after unref', port2.hasRef());
    port2.ref();
    log('after ref', port2.hasRef());
    port2.onmessage = null;
    log('after onmessage = null', port2.hasRef());
    port1.close();
  }
  {
    // A transferred port is detached; the receiver owns the channel end.
    const { port1, port2 } = new MessageChannel();
    const carrier = new MessageChannel();
    carrier.port1.postMessage('carrier', [port2]);
    log('received', receiveMessageOnPort(carrier.port2).message);
    log('detached', port2.hasRef(), port2.postMessage('z'));
    port1.close();
    carrier.port1.close();
  }
  {
    const { port1, port2 } = new MessageChannel();
    const spare = new MessageChannel();
    for (const list of [[port1], [spare.port1, spare.port1], [{}], 5]) {
      try { port1.postMessage('x', list); } catch (e) { log(e.name, e.code ?? '', e.message); }
    }
    try { port1.postMessage(); } catch (e) { log(e.name, e.code); }
    try { port1.postMessage(Symbol()); } catch (e) { log(e.name, e.message); }
    try { port1.postMessage(() => {}); } catch (e) { log(e.name, e.message); }
    port1.postMessage('with options', { transfer: [spare.port1] });
    log('options.transfer', receiveMessageOnPort(port2).message, spare.port1.hasRef());
    port1.close();
    spare.port2.close();
  }
  {
    // The event carries the transferred ports; they work on arrival.
    const a = new MessageChannel();
    const b = new MessageChannel();
    const arrived = new Promise((resolve) => { a.port2.onmessage = resolve; });
    a.port1.postMessage({ p: b.port1 }, [b.port1]);
    const e = await arrived;
    log('event', e.constructor.name, e.type, e.ports.length, e.ports[0] instanceof MessagePort,
        e.data.p === e.ports[0], Object.isFrozen(e.ports));
    const through = new Promise((resolve) => b.port2.once('message', resolve));
    e.ports[0].postMessage('through');
    log('through', await through);
    a.port1.close();
    b.port2.close();
  }
  {
    // One port's queue is delivered before the next macrotask, one message
    // per tick: microtasks and nextTick run between messages.
    const { port1, port2 } = new MessageChannel();
    const order = [];
    const done = new Promise((resolve) => {
      port2.on('message', (m) => {
        order.push('m' + m);
        process.nextTick(() => order.push('t' + m));
        Promise.resolve().then(() => { order.push('p' + m); if (m === 2) resolve(); });
      });
    });
    port1.postMessage(1);
    port1.postMessage(2);
    await done;
    await sleep(0);
    log('ticks between messages', order);
    port1.close();
  }
  log('end');
})();
