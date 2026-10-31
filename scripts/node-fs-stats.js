// Node host-compat `fs` Stats, BigIntStats, StatFs and Dirent (issue #1218),
// modelled on lib/internal/fs/utils.js. Runs after node-fs-support.js and
// parks its exports on `__node_fs_support.stats`.

(function () {
  "use strict";

  var S = globalThis.__node_fs_support;
  if (!S) return;
  var hostConstants = S.hostConstants;
  var S_IFMT = S.S_IFMT;

  function setOwn(obj, name, value) {
    Object.defineProperty(obj, name, {
      value: value,
      writable: true,
      enumerable: true,
      configurable: true,
    });
    return value;
  }

  // Rounded, not floored: the Date constructor floors a float timestamp.
  function dateFromMs(ms) {
    return new Date(Math.round(Number(ms)));
  }

  function instantFromNs(ns) {
    var Instant = globalThis.Temporal && globalThis.Temporal.Instant;
    if (!Instant) {
      throw S.nodeError(Error, "ERR_NO_TEMPORAL", "Temporal is not available");
    }
    return new Instant(ns);
  }

  function lazyField(proto, name, make) {
    Object.defineProperty(proto, name, {
      enumerable: true,
      configurable: true,
      get: function () {
        return setOwn(this, name, make(this));
      },
      set: function (value) {
        setOwn(this, name, value);
      },
    });
  }

  var kPartial = {
    atime: Symbol("kPartialAtimeNs"),
    mtime: Symbol("kPartialMtimeNs"),
    ctime: Symbol("kPartialCtimeNs"),
    birthtime: Symbol("kPartialBirthtimeNs"),
  };
  var TIMES = ["atime", "mtime", "ctime", "birthtime"];

  function StatsBase(dev, mode, nlink, uid, gid, rdev, blksize, ino, size, blocks) {
    this.dev = dev;
    this.mode = mode;
    this.nlink = nlink;
    this.uid = uid;
    this.gid = gid;
    this.rdev = rdev;
    this.blksize = blksize;
    this.ino = ino;
    this.size = size;
    this.blocks = blocks;
  }

  [
    ["isDirectory", "S_IFDIR"],
    ["isFile", "S_IFREG"],
    ["isBlockDevice", "S_IFBLK"],
    ["isCharacterDevice", "S_IFCHR"],
    ["isSymbolicLink", "S_IFLNK"],
    ["isFIFO", "S_IFIFO"],
    ["isSocket", "S_IFSOCK"],
  ].forEach(function (entry) {
    var type = hostConstants[entry[1]];
    StatsBase.prototype[entry[0]] = function () {
      return this._checkModeProperty(type);
    };
  });

  function Stats(dev, mode, nlink, uid, gid, rdev, blksize, ino, size, blocks,
                 atimeS, atimeNs, mtimeS, mtimeNs, ctimeS, ctimeNs, birthtimeS, birthtimeNs) {
    StatsBase.call(this, dev, mode, nlink, uid, gid, rdev, blksize, ino, size, blocks);
    this.atimeMs = atimeS * 1e3 + atimeNs / 1e6;
    this.mtimeMs = mtimeS * 1e3 + mtimeNs / 1e6;
    this.ctimeMs = ctimeS * 1e3 + ctimeNs / 1e6;
    this.birthtimeMs = birthtimeS * 1e3 + birthtimeNs / 1e6;
    this[kPartial.atime] = atimeNs;
    this[kPartial.mtime] = mtimeNs;
    this[kPartial.ctime] = ctimeNs;
    this[kPartial.birthtime] = birthtimeNs;
  }
  Object.setPrototypeOf(Stats.prototype, StatsBase.prototype);
  Object.setPrototypeOf(Stats, StatsBase);
  Stats.prototype._checkModeProperty = function (property) {
    return (this.mode & S_IFMT) === property;
  };

  function BigIntStats(dev, mode, nlink, uid, gid, rdev, blksize, ino, size, blocks,
                       atimeNs, mtimeNs, ctimeNs, birthtimeNs) {
    StatsBase.call(this, dev, mode, nlink, uid, gid, rdev, blksize, ino, size, blocks);
    this.atimeMs = atimeNs / 1000000n;
    this.mtimeMs = mtimeNs / 1000000n;
    this.ctimeMs = ctimeNs / 1000000n;
    this.birthtimeMs = birthtimeNs / 1000000n;
    this.atimeNs = atimeNs;
    this.mtimeNs = mtimeNs;
    this.ctimeNs = ctimeNs;
    this.birthtimeNs = birthtimeNs;
  }
  Object.setPrototypeOf(BigIntStats.prototype, StatsBase.prototype);
  Object.setPrototypeOf(BigIntStats, StatsBase);
  BigIntStats.prototype._checkModeProperty = function (property) {
    return (this.mode & BigInt(S_IFMT)) === BigInt(property);
  };

  TIMES.forEach(function (time) {
    var ms = time + "Ms";
    lazyField(Stats.prototype, time, function (self) { return dateFromMs(self[ms]); });
    lazyField(BigIntStats.prototype, time, function (self) { return dateFromMs(self[ms]); });
  });
  // The prototype accessors come last in Node's property order as well.
  TIMES.forEach(function (time) {
    lazyField(Stats.prototype, time + "Instant", function (self) {
      var ms = self[time + "Ms"];
      var sec = BigInt(Math.floor(ms / 1e3));
      return instantFromNs(sec * 1000000000n + BigInt(self[kPartial[time]]));
    });
    lazyField(BigIntStats.prototype, time + "Instant", function (self) {
      return instantFromNs(self[time + "Ns"]);
    });
  });

  // `v` is the 18-number (or 18-BigInt) array the `__host_fs_stat` floor returns.
  function getStats(v) {
    if (typeof v[0] === "bigint") {
      var ns = function (i) { return v[i] * 1000000000n + v[i + 1]; };
      return new BigIntStats(v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7], v[8],
        v[9], ns(10), ns(12), ns(14), ns(16));
    }
    var s = Object.create(Stats.prototype);
    Stats.apply(s, v);
    return s;
  }

  function StatFs(type, bsize, frsize, blocks, bfree, bavail, files, ffree) {
    this.type = type;
    this.bsize = bsize;
    this.frsize = frsize;
    this.blocks = blocks;
    this.bfree = bfree;
    this.bavail = bavail;
    this.files = files;
    this.ffree = ffree;
  }

  var kType = Symbol("type");
  var kStats = Symbol("stats");
  var C = {
    FILE: 1, DIR: 2, LINK: 3, FIFO: 4, SOCKET: 5, CHAR: 6, BLOCK: 7,
  };

  function Dirent(name, type, path) {
    this.name = name;
    this.parentPath = path;
    this[kType] = type;
  }
  [
    ["isDirectory", C.DIR, "isDirectory"],
    ["isFile", C.FILE, "isFile"],
    ["isBlockDevice", C.BLOCK, "isBlockDevice"],
    ["isCharacterDevice", C.CHAR, "isCharacterDevice"],
    ["isSymbolicLink", C.LINK, "isSymbolicLink"],
    ["isFIFO", C.FIFO, "isFIFO"],
    ["isSocket", C.SOCKET, "isSocket"],
  ].forEach(function (entry) {
    Dirent.prototype[entry[0]] = function () {
      if (this[kStats]) return this[kStats][entry[2]]();
      return this[kType] === entry[1];
    };
  });

  // A listing entry the filesystem reported as type 0: classify it by lstat.
  function direntFromStats(name, stats, path) {
    var d = new Dirent(name, null, path);
    d[kStats] = stats;
    return d;
  }

  S.stats = {
    Stats: Stats,
    BigIntStats: BigIntStats,
    StatFs: StatFs,
    Dirent: Dirent,
    direntFromStats: direntFromStats,
    getStats: getStats,
  };
})();
//# sourceURL=node:internal/fs/utils
