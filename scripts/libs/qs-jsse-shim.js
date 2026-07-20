// iconv-lite enables optional Node stream/Buffer prototype extensions whenever
// process.versions.node exists. qs uses only iconv.encode/decode, and the shared
// host shim intentionally does not implement Node streams, so keep iconv-lite
// on its browser-compatible core path under JSSE. This shim is inert on Node.

if (
  typeof __host_write !== "undefined" &&
  process.versions &&
  process.versions.node
) {
  delete process.versions.node;
}
