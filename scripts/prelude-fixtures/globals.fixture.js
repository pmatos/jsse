// #1216: the prelude's global surface, compared line-for-line with Node.
function kind(v) {
  return typeof v === "function" ? "function" : typeof v;
}
var rows = [
  ["process", kind(process)],
  ["process.argv", Array.isArray(process.argv)],
  ["process.env", kind(process.env)],
  ["process.exit", kind(process.exit)],
  ["process.hrtime.bigint", typeof process.hrtime.bigint()],
  ["process.stdout.write", kind(process.stdout.write)],
  ["process.stderr.write", kind(process.stderr.write)],
  ["console.log", kind(console.log)],
  ["console.error", kind(console.error)],
  ["console.warn", kind(console.warn)],
  ["console.info", kind(console.info)],
  ["console.debug", kind(console.debug)],
  ["console.dir", kind(console.dir)],
  ["console.group", kind(console.group)],
  ["console.table", kind(console.table)],
  ["console.count", kind(console.count)],
  ["console.assert", kind(console.assert)],
  ["console.time", kind(console.time)],
  ["Buffer", kind(Buffer)],
  ["Buffer.isBuffer", kind(Buffer.isBuffer)],
  ["Buffer instanceof Uint8Array", Buffer.from("x") instanceof Uint8Array],
  ["TextEncoder", kind(TextEncoder)],
  ["TextDecoder", kind(TextDecoder)],
  ["crypto.getRandomValues", kind(crypto.getRandomValues)],
  ["crypto.randomUUID", kind(crypto.randomUUID)],
];
rows.forEach(function (r) {
  console.log(r[0] + ": " + r[1]);
});

console.log(
  Object.keys(globalThis)
    .filter(function (k) {
      return ["process", "Buffer", "TextEncoder", "TextDecoder", "crypto", "util"].indexOf(k) >= 0;
    })
    .join(),
);

var rnd = crypto.getRandomValues(new Uint8Array(32));
console.log(rnd.length, rnd instanceof Uint8Array);
console.log(
  /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(
    crypto.randomUUID(),
  ),
);
try {
  crypto.getRandomValues(new Float32Array(1));
} catch (e) {
  console.log(e instanceof Error);
}

console.log(Buffer.concat([Buffer.from("ab"), Buffer.alloc(2, 1)]).toString("hex"));
console.log(Buffer.from([1, 2, 3]).toJSON());
console.log(Buffer.byteLength("€"), Buffer.isEncoding("utf8"), Buffer.isEncoding("nope"));
console.log(Buffer.from("ff00", "hex").readUInt16BE(0));
console.log(new TextDecoder("utf-8", { fatal: false }).decode(new Uint8Array([0xff])).length);
console.log(new TextEncoder().encoding);

process.stdout.write("no newline");
process.stdout.write("\n");
