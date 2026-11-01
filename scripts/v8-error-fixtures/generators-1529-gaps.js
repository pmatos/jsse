// What `generators-1529.js` left out (issues #1529, #1548): shapes whose frames
// V8 reports at a keyword or statement the AST used to record no offset for
// (`yield*`, `break`, the initializer of a declaration). Replayed against
// `expected-generators-1529-gaps.txt`, Node 26.8's output, by
// tests/node_stack_frames_1548.rs; see
// docs/agents/library-tests-typescript-conformance-generator-positions-1548.md.
const path = require("node:path");
const dir = path.dirname(__filename);

function show(label, stack) {
  console.log("## " + label);
  const lines = String(stack)
    .split("\n")
    .filter((line) => !/^\s+at /.test(line) || line.includes(dir + "/"));
  console.log(lines.join("\n").split(dir).join("<dir>"));
}

function* source() {
  yield 1;
  yield new Error("source").stack;
}
function* delegatesToAnIdentifier() {
  const inner = source();
  yield* inner;
}
async function* after() {
  await null;
  yield new Error("after").stack;
}
async function* delegatingAsync() {
  yield* after();
}
class Resource {
  async [Symbol.asyncDispose]() {
    await null;
    show("await using in an async generator", new Error("dispose").stack);
  }
}
async function* disposingGenerator() {
  await using one = new Resource();
  yield 1;
}
async function statements() {
  await using one = new Resource();
  const x = 1 + 2;
}
async function main() {
  for (const s of delegatesToAnIdentifier()) if (s !== 1) show("yield* of an identifier", s);
  for await (const s of delegatingAsync()) show("yield* of an async generator", s);
  for await (const x of disposingGenerator()) break;
  await statements();
}
main();
