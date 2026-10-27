'use strict';
// The child side of the fork-* fixtures; argv[2] picks the behaviour.
const cp = require('child_process');
const util = require('util');
const mode = process.argv[2];

switch (mode) {
  case 'echo':
    process.on('message', (m) => {
      if (m !== null && typeof m === 'object' && m.cmd === 'quit') process.disconnect();
      else process.send(m);
    });
    break;
  case 'inspect':
    process.on('message', (m) => {
      if (m !== null && typeof m === 'object' && m.cmd === 'quit') process.disconnect();
      else process.send(util.inspect(m, { depth: 6, breakLength: Infinity }));
    });
    break;
  case 'info':
    process.send({
      fd: process.env.NODE_CHANNEL_FD,
      serialization: process.env.NODE_CHANNEL_SERIALIZATION_MODE,
      connected: process.connected,
      sendType: typeof process.send,
      disconnectType: typeof process.disconnect,
      channelType: process.channel && process.channel.constructor.name,
      args: process.argv.slice(2),
      messageListeners: process.listenerCount('message'),
    }, () => process.disconnect());
    break;
  case 'selfdisconnect':
    process.on('disconnect', () => console.log('child disconnect', process.connected, process.channel));
    process.send('bye', () => {
      process.disconnect();
      console.log('after disconnect', process.connected, process.channel);
      process.send('late', (e) => console.log('late send', e && e.code));
    });
    break;
  case 'exit':
    process.on('message', (code) => process.exit(code));
    break;
  case 'hold':
    process.on('message', () => {});
    process.send('ready');
    break;
  case 'noop':
    process.send('hello');
    break;
  case 'addremove': {
    const handler = () => {};
    process.on('message', handler);
    process.removeListener('message', handler);
    process.send('listener removed');
    break;
  }
  case 'once':
    process.once('message', (m) => process.send(['once', m]));
    break;
  case 'early':
    // messages that arrive before the first listener are kept for it
    setTimeout(() => process.on('message', (m) => {
      if (m !== null && typeof m === 'object' && m.cmd === 'quit') process.disconnect();
      else process.send(m);
    }), 100);
    process.send('armed');
    break;
  case 'bulk':
    process.on('message', (m) => {
      if (m === 'done') return process.disconnect();
      if (typeof m === 'string') return process.send({ length: m.length, tail: m.slice(-8) });
      return process.send({ n: m.n, echo: 'x'.repeat(m.n % 50) });
    });
    break;
  case 'leaf':
    process.on('disconnect', () => console.log('leaf: parent gone'));
    process.send('up');
    break;
  case 'middle': {
    const leaf = cp.fork(__filename, ['leaf'], { stdio: 'inherit' });
    leaf.on('message', () => process.send('ready'));
    process.on('message', () => {});
    break;
  }
  default:
    throw new Error('unknown mode ' + mode);
}
