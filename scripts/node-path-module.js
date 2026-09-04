// CommonJS `path` selector for bundled library dependencies.
//
// css-tree's test suite only uses the POSIX-shaped join/relative/basename/
// dirname/extname below (confirmed by grepping its lib/__tests tree). Node
// keeps its native module (independent oracle); JSSE gets a minimal
// same-surface POSIX implementation. Only display-name cosmetics (fixture
// test names, error messages) depend on this module — no assertion outcome
// does — so exactness beyond "doesn't throw and gives a sane path" is not
// load-bearing.

if (typeof __host_write !== "undefined") {
  function normalizeParts(path) {
    var absolute = path.charAt(0) === "/";
    var segments = path.split("/");
    var out = [];
    for (var i = 0; i < segments.length; i++) {
      var segment = segments[i];
      if (segment === "" || segment === ".") continue;
      if (segment === "..") {
        if (out.length && out[out.length - 1] !== "..") {
          out.pop();
        } else if (!absolute) {
          out.push("..");
        }
        continue;
      }
      out.push(segment);
    }
    return { absolute: absolute, segments: out };
  }

  function normalize(path) {
    if (path === "") return ".";
    var parts = normalizeParts(path);
    var joined = parts.segments.join("/");
    if (parts.absolute) return "/" + joined;
    return joined || ".";
  }

  function join() {
    var parts = [];
    for (var i = 0; i < arguments.length; i++) {
      if (arguments[i]) parts.push(arguments[i]);
    }
    return normalize(parts.join("/"));
  }

  function dirname(path) {
    var norm = normalize(path);
    var idx = norm.lastIndexOf("/");
    if (idx < 0) return ".";
    if (idx === 0) return "/";
    return norm.slice(0, idx);
  }

  function basename(path, ext) {
    var norm = normalize(path);
    var idx = norm.lastIndexOf("/");
    var base = idx >= 0 ? norm.slice(idx + 1) : norm;
    if (ext && base.slice(-ext.length) === ext && base !== ext) {
      base = base.slice(0, -ext.length);
    }
    return base;
  }

  function extname(path) {
    var base = basename(path);
    var idx = base.lastIndexOf(".");
    if (idx <= 0) return "";
    return base.slice(idx);
  }

  function relative(from, to) {
    var fromParts = normalizeParts(from).segments;
    var toParts = normalizeParts(to).segments;
    var common = 0;
    while (
      common < fromParts.length &&
      common < toParts.length &&
      fromParts[common] === toParts[common]
    ) {
      common++;
    }
    var up = fromParts.length - common;
    var out = [];
    for (var i = 0; i < up; i++) out.push("..");
    return out.concat(toParts.slice(common)).join("/") || ".";
  }

  module.exports = {
    join: join,
    dirname: dirname,
    basename: basename,
    extname: extname,
    relative: relative,
    normalize: normalize,
    sep: "/",
  };
} else {
  module.exports = require("node:path");
}
