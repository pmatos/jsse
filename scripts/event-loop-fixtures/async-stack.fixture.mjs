// RUN: 0
// A stack captured inside a promise reaction job continues with `at async`
// frames along the pending promises that await the running function (#1368).
// Frames of the host's own modules (`node:`) are left out of what is printed.
const show = (e) =>
  console.log(e.stack.split("\n").filter((line) => !line.includes("node:")).join("\n"));

function k() {
  throw new Error("sync");
}
try {
  k();
} catch (e) {
  show(e);
}

async function f() {
  await 1;
  throw new Error("await");
}
async function g() {
  await f();
}
await g().catch(show);

async function viaThen() {
  await Promise.resolve().then(() => {
    throw new Error("then");
  });
}
await viaThen().catch(show);

class Worker {
  async run() {
    await null;
    throw new Error("method");
  }
  static async start() {
    await new Worker().run();
  }
}
await Worker.start().catch(show);

async function nested() {
  await null;
  return Promise.resolve().then(() => f());
}
await nested().catch(show);

async function enclosing() {
  await null;
  return Promise.resolve().then(() => g().catch(show));
}
await enclosing();

async function first() {
  throw new Error("before the first await");
}
async function second() {
  await first();
}
await second().catch(show);

Error.stackTraceLimit = 2;
await g().catch(show);
Error.stackTraceLimit = 1;
await g().catch(show);
Error.stackTraceLimit = 10;

const holder = { async method() { await null; throw new Error("arrow"); } };
const arrow = async () => { await holder.method(); };
await arrow().catch(show);

await Promise.resolve();
show(new Error("after the top-level await"));
