// CommonJS `string_decoder` selector for bundled library dependencies.
//
// iconv-lite's internal codec module imports StringDecoder even when qs only
// selects a legacy DBCS codec. The JSSE fallback covers the ordinary buffered
// write/end surface; Node retains its native implementation.

if (typeof __host_write !== "undefined") {
  function StringDecoder(encoding) {
    this.encoding = encoding || "utf8";
  }
  StringDecoder.prototype.write = function (buffer) {
    return globalThis.Buffer.from(buffer).toString(this.encoding);
  };
  StringDecoder.prototype.end = function (buffer) {
    return buffer === undefined ? "" : this.write(buffer);
  };
  module.exports = { StringDecoder: StringDecoder };
} else {
  module.exports = require("node:string_decoder");
}
