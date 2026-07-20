// Zod-specific Web host surface, prepended only to its library-test bundle.
// The shim is intentionally small: URL itself is bundled from whatwg-url;
// these are the remaining globals the v4 classic runtime suite uses.
(function () {
  "use strict";

  if (typeof globalThis.atob === "undefined") {
    globalThis.atob = function (value) {
      var input = String(value).replace(/[\t\n\f\r ]/g, "");
      if (input.length % 4 === 0) input = input.replace(/={1,2}$/, "");
      if (input.length % 4 === 1 || /[^+/0-9A-Za-z]/.test(input)) {
        throw new Error("Invalid character");
      }
      return Buffer.from(input, "base64").toString("latin1");
    };
  }

  if (typeof globalThis.btoa === "undefined") {
    globalThis.btoa = function (value) {
      var input = String(value);
      for (var i = 0; i < input.length; i++) {
        if (input.charCodeAt(i) > 0xff) throw new Error("Invalid character");
      }
      return Buffer.from(input, "latin1").toString("base64");
    };
  }

  if (typeof globalThis.File === "undefined") {
    globalThis.File = class File {
      constructor(parts, name, options) {
        options = options || {};
        this.name = String(name);
        this.type = options.type ? String(options.type).toLowerCase() : "";
        this.lastModified = options.lastModified || Date.now();
        this.size = 0;
        for (var i = 0; i < parts.length; i++) {
          var part = parts[i];
          if (typeof part === "string") this.size += Buffer.byteLength(part);
          else if (part && typeof part.byteLength === "number") this.size += part.byteLength;
          else if (part && typeof part.size === "number") this.size += part.size;
          else this.size += Buffer.byteLength(String(part));
        }
      }

      get [Symbol.toStringTag]() {
        return "File";
      }
    };
  }
})();
