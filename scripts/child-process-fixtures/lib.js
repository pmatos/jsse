'use strict';
const path = require('path');
const crypto = require('crypto');

const isJsse = path.basename(process.execPath).startsWith('jsse');

// Arguments that make the running engine execute `args` as a Node program.
exports.selfArgs = (...args) => (isJsse ? ['--node'] : []).concat(args);

exports.md5 = (buf) => crypto.createHash('md5').update(buf).digest('hex');

// Error properties that Node makes deterministic.
exports.err = (e) => e == null ? e : {
  name: e.name, code: e.code, errno: e.errno, syscall: e.syscall, path: e.path,
  spawnargs: e.spawnargs, message: e.message.split('\n')[0],
};

// Run a ChildProcess to 'close' and report what it did.
exports.finish = (child) => new Promise((resolve) => {
  const out = [];
  const err = [];
  const events = [];
  if (child.stdout) child.stdout.on('data', (d) => out.push(d));
  if (child.stderr) child.stderr.on('data', (d) => err.push(d));
  child.on('spawn', () => events.push('spawn'));
  child.on('error', (e) => events.push('error:' + e.code));
  child.on('exit', (code, signal) => events.push(`exit:${code}:${signal}`));
  child.on('close', (code, signal) => {
    events.push(`close:${code}:${signal}`);
    resolve({
      events,
      stdout: Buffer.concat(out).toString('latin1'),
      stderr: Buffer.concat(err).toString('latin1'),
    });
  });
});

exports.show = (label, value) => console.log(label, JSON.stringify(value));

// fork() options that point the child at another engine when FORK_EXEC is set
// (the oracle's jsse-to-node and node-to-jsse passes).
exports.forkOpts = (extra = {}) => process.env.FORK_EXEC ?
  { execPath: process.env.FORK_EXEC, execArgv: process.env.FORK_ARGS ? [process.env.FORK_ARGS] : [], ...extra } :
  extra;
