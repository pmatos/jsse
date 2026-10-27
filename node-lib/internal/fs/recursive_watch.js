'use strict';

// `fs.watch(path, { recursive: true })` on a host whose watches are not
// recursive: one watch per directory, kept in step with the tree. Reported
// names are relative to the root. A directory that appears is watched and its
// entries announced as renames; one that disappears loses its watches. Events
// on a sub-directory itself are dropped, because its parent reports them.

const { sync } = require('internal/host/fs');
const { Buffer } = require('buffer');
const { startWatch, kRename, kSelf } = require('internal/fs/watch_native');

const kBoth = 3;

function join(base, name) {
  return base === '' ? name : `${base}/${name}`;
}

class RecursiveWatch {
  constructor(root, onevent, persistent, ignored, onerror) {
    this.ignored = ignored;
    this.root = root.toString();
    this.onevent = onevent;
    this.onerror = onerror;
    this.persistent = persistent;
    this.closed = false;
    this.dirs = new Map();
    this.watchDirectory(this.root, '', false);
  }

  watchDirectory(path, relative, announce) {
    const handle = startWatch(
      path, (bits, name) => this.handle(path, relative, bits, name), this.persistent);
    this.dirs.set(path, handle);
    let entries;
    try {
      entries = sync.readdirSync(path, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      const child = join(relative, entry.name);
      if (this.ignored !== null && this.ignored(child)) continue;
      if (announce) this.onevent(kRename, child);
      if (entry.isDirectory()) this.watchSubtree(`${path}/${entry.name}`, child, announce);
    }
  }

  watchSubtree(path, relative, announce) {
    if (this.closed || this.dirs.has(path)) return;
    try {
      this.watchDirectory(path, relative, announce);
    } catch (error) {
      if (error.code === 'ENOSPC' || error.code === 'EMFILE') this.onerror(error);
    }
  }

  unwatchSubtree(path) {
    const prefix = `${path}/`;
    for (const [dir, handle] of this.dirs) {
      if (dir === path || dir.startsWith(prefix)) {
        handle.close();
        this.dirs.delete(dir);
      }
    }
  }

  handle(dir, relative, bits, name) {
    if (this.closed) return;
    if (bits & kSelf) {
      if (dir === this.root) this.onevent(bits & kBoth, Buffer.from(name).toString());
      return;
    }
    const child = join(relative, Buffer.from(name).toString());
    if (this.ignored !== null && this.ignored(child)) return;
    this.onevent(bits & kBoth, child);
    if (!(bits & kRename)) return;
    const path = `${dir}/${Buffer.from(name).toString()}`;
    let stats;
    try {
      stats = sync.lstatSync(path);
    } catch {
      this.unwatchSubtree(path);
      return;
    }
    if (stats.isDirectory()) this.watchSubtree(path, child, true);
  }

  ref() {
    this.persistent = true;
    for (const handle of this.dirs.values()) handle.ref();
  }

  unref() {
    this.persistent = false;
    for (const handle of this.dirs.values()) handle.unref();
  }

  close() {
    if (this.closed) return;
    this.closed = true;
    for (const handle of this.dirs.values()) handle.close();
    this.dirs.clear();
  }
}

module.exports = { RecursiveWatch };
