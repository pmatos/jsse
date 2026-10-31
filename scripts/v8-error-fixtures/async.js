// Async function frames and `at async` frames (issues #1333, #1358, #1368),
// run verbatim on Node and on `jsse --node` by scripts/run-node-v8-error-oracle.sh,
// in the bytecode VM and the tree-walker alike. Everything runs inside
// functions: the top-level frame differs between the two hosts.
function show(label, value) {
  console.log("## " + label);
  console.log(value);
}

class Worker {
  async start() {
    await null;
    return new Error("start");
  }
  static async make() {
    await 1;
    return new Error("make");
  }
  async sync() {
    return new Error("sync");
  }
}

async function nested() {
  return await (async function inner() {
    await null;
    return new Error("inner");
  })();
}

async function throws() {
  throw new Error("thrown");
}

async function loops() {
  for (let i = 0; i < 2; i++) {
    await null;
  }
  try {
    await Promise.reject(1);
  } catch {}
  finally {
    await null;
  }
  return new Error("loops");
}

async function viaThen() {
  return new Promise((resolve) => Promise.resolve().then(() => resolve(new Error("then"))));
}

async function a() {
  await null;
  return new Error("x").stack;
}
async function b() {
  return await a();
}

function capture(skip) {
  const holder = {};
  Error.captureStackTrace(holder, skip);
  return holder.stack;
}
async function c(skip) {
  await null;
  return capture(skip);
}
async function e(skip) {
  return await c(skip);
}
async function f(skip) {
  return await e(skip);
}

async function frameBeforeAwait() {
  return new Error("sync part").stack.split("\n").slice(0, 3).join("\n");
}

async function withDefault(value = new Error("default")) {
  return value;
}

function Context() {}
const arrows = {
  early: async () => new Error("early"),
  defaulted: async (value = new Error("arrow default")) => value,
  late: async () => {
    await null;
    return new Error("late");
  },
};

async function main() {
  show("method after await", (await new Worker().start()).stack);
  show("static method after await", (await Worker.make()).stack);
  show("method before await", (await new Worker().sync()).stack);
  show("nested async function", (await nested()).stack);
  try {
    await throws();
  } catch (error) {
    show("thrown before await", error.stack);
  }
  show("loops and finally", (await loops()).stack);
  show("inside a then handler", (await viaThen()).stack);
  show("default parameter", (await withDefault()).stack);
  show("async arrow before await", (await arrows.early()).stack);
  show("async arrow after await", (await arrows.late.call(new Context())).stack);
  show("async arrow default parameter", (await arrows.defaulted.call(new Context())).stack);
  show("issue 1333", await b());
  show("issue 1358", await frameBeforeAwait());
  show("captureStackTrace skipping c", await f(c));
  show("captureStackTrace skipping e", await f(e));
  show("captureStackTrace skipping f", await f(f));
  show("captureStackTrace skipping main", await f(main));
  show("captureStackTrace skipping a stranger", await f(function stranger() {}));
  show("captureStackTrace skipping nothing", await f(undefined));
  show("captureStackTrace skipping a live function", await e(capture));
}

main();
