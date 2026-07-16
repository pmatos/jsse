// Opt a library bundle into the shared in-process test harness on both jsse and
// Node. This must be prepended immediately before node-test-harness.js.
globalThis.__JSSE_FORCE_TEST_HARNESS__ = true;
