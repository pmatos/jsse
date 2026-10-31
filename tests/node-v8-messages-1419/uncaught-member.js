// An engine-thrown member read on undefined: the fatal report's header and
// caret sit at the property name, as V8 puts them.
const config = {};
function readPort() {
  return config.server.port;
}
readPort();
