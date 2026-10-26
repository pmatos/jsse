// #1216 acceptance: the embedded prelude gives a bare script process, console,
// util-formatted output and Buffer with nothing prepended.
console.log(/^v\d+\.\d+\.\d+/.test(process.version), typeof process.versions.node);
console.log(Buffer.from("a").toString("hex"), Buffer.from("a")[0], Buffer.isBuffer(Buffer.from("a")));
console.log("%s=%d %j", "n", 42, { a: [1, 2] });
console.log(Buffer.from("héllo wörld").toString("base64"));
console.log(Buffer.from("aGk=", "base64").toString("hex"));
console.log(Array.from(new TextEncoder().encode("é€")));
console.log(new TextDecoder().decode(new Uint8Array([0xe2, 0x82, 0xac, 0x21])));
console.error("to stderr");
