'use strict';

// The handles under `fs.watch` (issue #1235): an inotify watch from the
// `__host_watch_*` floor, or, where the host has none (`ENOSYS`), a polling
// stand-in with the same interface, and the table that routes native events to
// the handle. Events are `(bits, name)`: bit 1 is a change, bit 2 a rename,
// bit 4 marks the watched path itself. See
// docs/adr/2026-10-27-0300-node-host-fs-watch.md.

const binding = require('internal/host/child_process');
const { sync } = require('internal/host/fs');

const kChange = 1;
const kRename = 2;
const kSelf = 4;
const POLL_INTERVAL = 100;

const natives = new Map();

binding.onWatch((id, bits, name) => {
  const handle = natives.get(id);
  if (handle !== undefined) handle.onevent(bits, name);
});

class NativeWatch {
  constructor(path, onevent, persistent) {
    this.id = binding.watchAdd(path);
    this.onevent = onevent;
    this.refed = persistent;
    this.closed = false;
    natives.set(this.id, this);
    if (persistent) binding.loopRef(1);
  }

  ref() {
    if (this.closed || this.refed) return;
    this.refed = true;
    binding.loopRef(1);
  }

  unref() {
    if (this.closed || !this.refed) return;
    this.refed = false;
    binding.loopRef(-1);
  }

  close() {
    if (this.closed) return;
    this.closed = true;
    natives.delete(this.id);
    binding.watchRemove(this.id);
    if (this.refed) binding.loopRef(-1);
  }
}

function basename(path) {
  const bytes = typeof path === 'string' ? path : path.toString();
  return bytes.slice(bytes.lastIndexOf('/') + 1);
}

function signature(stats) {
  return `${stats.mtimeMs}:${stats.ctimeMs}:${stats.size}:${stats.ino}:${stats.mode}`;
}

function scan(path) {
  const entries = new Map();
  let stats;
  try {
    stats = sync.lstatSync(path);
  } catch (error) {
    error.syscall = 'watch';
    error.message = error.message.replace(', lstat ', ', watch ');
    throw error;
  }
  if (!stats.isDirectory()) {
    entries.set(basename(path), signature(stats));
    return { directory: false, entries };
  }
  for (const name of sync.readdirSync(path)) {
    try {
      entries.set(name, signature(sync.lstatSync(`${path}/${name}`)));
    } catch {
      // vanished while scanning
    }
  }
  return { directory: true, entries };
}

// Where there is no inotify: a timer that diffs a directory listing (or one
// file) and reports the difference as the events inotify would have sent.
class PollWatch {
  constructor(path, onevent, persistent) {
    this.path = path.toString();
    this.onevent = onevent;
    this.closed = false;
    this.snapshot = scan(this.path);
    this.timer = setInterval(() => this.poll(), POLL_INTERVAL);
    if (!persistent) this.timer.unref();
  }

  poll() {
    let next;
    try {
      next = scan(this.path);
    } catch {
      this.onevent(kRename | kSelf, basename(this.path));
      this.close();
      return;
    }
    const before = this.snapshot.entries;
    this.snapshot = next;
    const flag = next.directory ? 0 : kSelf;
    for (const [name, signed] of next.entries) {
      const old = before.get(name);
      if (this.closed) return;
      if (old === undefined) this.onevent(kRename | flag, name);
      else if (old !== signed) this.onevent(kChange | flag, name);
    }
    for (const name of before.keys()) {
      if (this.closed) return;
      if (!next.entries.has(name)) this.onevent(kRename | flag, name);
    }
  }

  ref() {
    if (!this.closed) this.timer.ref();
  }

  unref() {
    if (!this.closed) this.timer.unref();
  }

  close() {
    if (this.closed) return;
    this.closed = true;
    clearInterval(this.timer);
  }
}

function startWatch(path, onevent, persistent) {
  try {
    return new NativeWatch(path, onevent, persistent);
  } catch (error) {
    if (error.code !== 'ENOSYS') throw error;
  }
  return new PollWatch(path, onevent, persistent);
}

module.exports = { startWatch, kRename, kSelf };
