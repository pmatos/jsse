// Generator frames and `for await` / `await using` positions (issue #1529), run
// verbatim on Node and on `jsse --node`; Node's output is pinned in
// expected-generators-1529.txt and replayed by tests/node_stack_frames_1529.rs.
// Frames print without their paths, and only the ones this fixture defines are kept.
const path = require("node:path");
const dir = path.dirname(__filename);

function show(label, stack) {
  console.log("## " + label);
  const lines = String(stack)
    .split("\n")
    .filter((line) => !/^\s+at /.test(line) || line.includes(dir + "/") || /Promise\.\w+ \(index/.test(line))
    .filter((line) => !line.includes("Object.<anonymous>"));
  console.log(lines.join("\n").split(dir).join("<dir>"));
}

function* sync() {
  yield new Error("sync").stack;
}
function* syncAfterYield() {
  yield 1;
  yield new Error("after yield").stack;
}
function* delegating() {
  yield* syncAfterYield();
}
function* afterDelegating() {
  yield* syncAfterYield();
  yield new Error("after delegating").stack;
}
function* caughtDelegation() {
  try {
    yield* 5;
  } catch {
    yield new Error("caught delegation").stack;
  }
}
function* finalizing() {
  try {
    yield 1;
  } finally {
    yield new Error("finally").stack;
  }
}
function* syncWithArgs(a, b = new Error("default").stack) {
  yield b;
}
class Box {
  *items() {
    yield new Error("method").stack;
  }
  static *make() {
    yield new Error("static").stack;
  }
  async *stream() {
    await null;
    yield new Error("stream").stack;
  }
}
const literal = {
  *gen() {
    yield new Error("literal").stack;
  },
};

async function* after() {
  await null;
  yield new Error("after").stack;
}
async function* beforeAwait() {
  yield new Error("before").stack;
}
async function* afterYield() {
  yield 1;
  await null;
  yield new Error("after yield").stack;
}
async function leaf() {
  await null;
  return new Error("leaf").stack;
}
async function* awaiting() {
  yield await leaf();
}
async function* throwing() {
  await null;
  throw new Error("thrown");
}
async function* viaAll() {
  yield (await Promise.all([leaf()]))[0];
}

async function loops() {
  for (const s of sync()) show("sync generator", s);
  for (const s of delegating()) if (s !== 1) show("yield* of a sync generator", s);
  for (const s of syncWithArgs(1)) show("generator default parameter", s);
  for (const s of new Box().items()) show("generator method", s);
  for (const s of Box.make()) show("static generator method", s);
  for (const s of literal.gen()) show("generator in an object literal", s);
  for (const s of syncAfterYield()) if (s !== 1) show("second step of a generator", s);
  for (const s of afterDelegating()) if (typeof s === "string" && s.includes("after delegating")) show("body after a finished yield*", s);
  for (const s of caughtDelegation()) show("a yield* that throws, caught", s);
  const fin = finalizing();
  fin.next();
  show("return() into a finally", fin.return(7).value);
  const fin2 = finalizing();
  fin2.next();
  show("throw() into a finally", fin2.throw(new Error("t")).value);
  const g = sync();
  show("next called directly", g.next().value);
  show("spread of a generator", [...sync()][0]);
  show("Array.from of a generator", Array.from(sync())[0]);
}

async function awaits() {
  for await (const s of after()) show("async generator after an await", s);
  for await (const s of beforeAwait()) show("async generator before any await", s);
  for await (const s of afterYield()) if (s !== 1) show("async generator after a yield", s);
  for await (const s of awaiting()) show("async generator awaiting an async function", s);
  for await (const s of viaAll()) show("async generator awaiting Promise.all", s);
  for await (const s of new Box().stream()) show("async generator method", s);
  try {
    for await (const s of throwing()) show("unreachable", s);
  } catch (e) {
    show("an error thrown by an async generator", e.stack);
  }
  const it = after();
  const first = it.next();
  show("next called directly", (await first).value);
  const queued = after();
  const [a, b] = [queued.next(), queued.next()];
  show("two queued requests", (await a).value);
  await b;
}

async function heads() {
  let s;
  for await (s of after()) show("for await with an assigned variable", s);
  for await (const { stack } of errors()) show("for await with a destructured binding", stack);
  for await ([s] of arrays()) show("for await with an assigned pattern", s);
  for await (
    const
      spread of
      after()
  ) show("for await over several lines", spread);
  for await (var v of after()) show("for await with var", v);
  for await (const x of [Promise.resolve(1)]) show("for await over an array", new Error(String(x)).stack);
}

async function* errors() {
  await null;
  yield new Error("destructured");
}
async function* arrays() {
  await null;
  yield [new Error("array pattern").stack];
}
async function inner() {
  for await (const x of after()) {
    return new Error("returned from the loop").stack;
  }
}

class Resource {
  constructor(label) {
    this.label = label;
  }
  async [Symbol.asyncDispose]() {
    await null;
    show(this.label, new Error("dispose").stack);
  }
}
async function disposing() {
  await using one = new Resource("await using in a function");
  await null;
}
async function disposingInBlock() {
  {
    await using one = new Resource("await using in a block");
    await using two = new Resource("two await using declarations");
  }
  for (const x of [1]) {
    await using three = new Resource("await using in a loop");
  }
}
async function main() {
  await loops();
  await awaits();
  await heads();
  show("return inside for await", await inner());
  await disposing();
  await disposingInBlock();
}

main().catch((e) => console.log("failed", e));
