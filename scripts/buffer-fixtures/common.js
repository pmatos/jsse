'use strict';

// Shared by the probes: stable printing and a digest, so output can be diffed
// between Node and `jsse --node` and pinned.
const lines = [];
let hash = 0x811c9dc5;

function emit(label, value) {
  const line = label + ' ' + (typeof value === 'string' ? value : JSON.stringify(value));
  lines.push(line);
  for (let i = 0; i < line.length; i++) {
    hash = Math.imul(hash ^ line.charCodeAt(i), 0x01000193) >>> 0;
  }
  console.log(line);
}

function attempt(label, fn) {
  try {
    emit(label, fn());
  } catch (e) {
    emit(label, 'throws ' + (e && e.name) + ' ' + (e && e.code) + ' ' + (e && e.message));
  }
}

function digest() {
  return 'digest ' + hash.toString(16);
}

let seed = 20260101;
function rnd(n) {
  seed = (Math.imul(seed, 1103515245) + 12345) & 0x7fffffff;
  return (seed >>> 8) % n;
}

module.exports = { emit, attempt, digest, rnd };
