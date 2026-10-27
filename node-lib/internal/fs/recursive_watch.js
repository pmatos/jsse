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
    this.inodes = new Map();
    this.starting = true;
    try {
      this.watchDirectory(this.root, '', false);
    } catch (error) {
      this.close();
      throw error;
    } finally {
      this.starting = false;
    }
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

  watchSubtree(path, relative, announce, ino) {
    if (this.closed) return;
    if (ino === undefined) {
      try {
        ino = sync.lstatSync(path).ino;
      } catch {
        return;
      }
    }
    if (this.dirs.has(path)) {
      if (this.inodes.get(path) === ino) return;
      this.unwatchSubtree(path);
    }
    try {
      this.watchDirectory(path, relative, announce);
      this.inodes.set(path, ino);
    } catch (error) {
      if (error.code !== 'ENOSPC' && error.code !== 'EMFILE') return;
      if (this.starting) throw error;
      this.onerror(error);
    }
  }

  unwatchSubtree(path) {
    const prefix = `${path}/`;
    for (const [dir, handle] of this.dirs) {
      if (dir === path || dir.startsWith(prefix)) {
        handle.close();
        this.dirs.delete(dir);
        this.inodes.delete(dir);
      }
    }
  }

  handle(dir, relative, bits, name) {
    if (this.closed) return;
    if (bits & kSelf) {
      const own = Buffer.from(name).toString();
      if (dir === this.root && !(this.ignored !== null && this.ignored(own))) {
        this.onevent(bits & kBoth, own);
      }
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
    if (stats.isDirectory()) this.watchSubtree(path, child, true, stats.ino);
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
    this.inodes.clear();
  }
}

module.exports = { RecursiveWatch };
