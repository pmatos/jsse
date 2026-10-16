// Evaluate only the upstream registry/driver definitions, without running tests.
const fs = require('node:fs'), vm = require('node:vm');
const source = fs.readFileSync(process.argv[2] + '/JetStreamDriver.js', 'utf8');
const context = {
  console, isInBrowser: false, performance: { now: Date.now },
  JetStreamParams: {
    prefetchResources: false, forceGC: false, testList: [], testExcludeList: [],
    testIterationCountMap: {}, testWorstCaseCountMap: {},
  },
};
const inspect = `
function metadata(b) {
  return {name:b.name,kind:b.constructor.name,files:b.files,
    preload:Object.fromEntries(b.preloadEntries),iterations:b.iterations,
    tags:Array.from(b.tags),args:b.benchmarkArguments,deterministicRandom:b._deterministicRandom,
    children:b.benchmarks?.map(metadata)};
}
JSON.stringify(BENCHMARKS.map(metadata))`;
fs.writeFileSync(process.argv[3], vm.runInNewContext(source + inspect, context));
