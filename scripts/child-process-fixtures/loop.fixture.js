// RUN: 0
const cp = require('child_process');
const { show } = require('./lib');

const log = [];
process.on('beforeExit', () => { if (!log.includes('beforeExit')) log.push('beforeExit'); });
process.on('exit', (code) => { log.push('exit ' + code); show('log', log); });

// A running child keeps the loop alive; nextTick and promise jobs run before
// the events of the same tick.
const alive = cp.spawn('sh', ['-c', 'sleep 0.1; echo late']);
alive.stdout.on('data', (d) => log.push('data ' + String(d).trim()));
alive.on('spawn', () => log.push('spawn'));
alive.on('exit', (c) => log.push('exit-event ' + c));
alive.on('close', () => log.push('close'));
process.nextTick(() => log.push('tick'));
Promise.resolve().then(() => log.push('microtask'));
log.push('sync');

// An unref'd child with ignored stdio does not keep the loop alive.
const detached = cp.spawn('sleep', ['30'], { stdio: 'ignore', detached: true });
detached.unref();
detached.on('exit', () => log.push('detached exit (never)'));
setTimeout(() => process.kill(detached.pid, 'SIGKILL'), 250);

// ref() brings it back for the duration of the timer, then unref() lets go.
const toggled = cp.spawn('sleep', ['30'], { stdio: 'ignore' });
toggled.unref();
toggled.ref();
toggled.unref();
toggled.on('exit', (c, s) => log.push('toggled ' + s));
setTimeout(() => toggled.kill(), 120);

// Timers and children interleave through one loop.
const quick = cp.spawn('true');
setTimeout(() => log.push('timer 0'), 0);
quick.on('close', () => show('quick closed', true));
