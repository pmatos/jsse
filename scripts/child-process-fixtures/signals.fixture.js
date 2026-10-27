// RUN: 0
const cp = require('child_process');
const { err, show } = require('./lib');

show('pid', { type: typeof process.pid, positive: process.pid > 1, ppid: typeof process.ppid });
show('kill 0 self', process.kill(process.pid, 0));
for (const bad of [
  () => process.kill('x'), () => process.kill(process.pid, 'SIGNOPE'), () => process.kill(2 ** 22, 0),
  () => process.kill(process.pid, 9999), () => process.kill(undefined),
]) {
  try { bad(); console.log('no throw'); } catch (e) { show('throws', err(e)); }
}

const events = [];
let wake;
process.on('SIGUSR2', (name, num) => {
  events.push(`SIGUSR2 ${name} ${num}`);
  wake();
});
process.once('SIGUSR1', (name) => events.push(`once ${name}`));
show('listeners', { usr1: process.listenerCount('SIGUSR1'), usr2: process.listenerCount('SIGUSR2') });
const arrival = () => new Promise((resolve) => { wake = resolve; });
(async () => {
  let got = arrival();
  process.kill(process.pid, 'SIGUSR2');
  await got;
  got = arrival();
  process.kill(process.pid, 'SIGUSR2');
  await got;
  process.kill(process.pid, 'SIGUSR1');
  await new Promise((resolve) => setTimeout(resolve, 50));
  next();
})();
setTimeout(() => {}, 100);

function next() {
  show('events', events);
  show('usr1 left', process.listenerCount('SIGUSR1'));
  process.removeAllListeners('SIGUSR2');
  const c = cp.spawn(process.execPath, require('./lib').selfArgs('-e',
    'process.on("SIGTERM", () => { console.log("child got SIGTERM"); process.exit(8); }); console.log("ready"); setTimeout(() => {}, 5000);'));
  c.stdout.once('data', () => c.kill('SIGTERM'));
  c.on('exit', (code, signal) => show('child exit', { code, signal }));
  c.on('close', () => {
    const g = cp.spawn('sleep', ['5']);
    g.on('exit', (code, signal) => show('killed by pid', { code, signal }));
    show('process.kill child', process.kill(g.pid, 'SIGKILL'));
  });
}
