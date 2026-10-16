// A host console must be exposed through the same global-object property
// that bare-name lookup reads. This affects JetStream web-ssr's console stub.
if (console !== globalThis.console)
  throw new Error('console is not the globalThis.console property');
const replacement = { log() {} };
globalThis.console = replacement;
if (console !== replacement)
  throw new Error('globalThis.console replacement is ignored by bare console');
