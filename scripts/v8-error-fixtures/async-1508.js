// `at async Promise.all (index N)` frames and V8's ` [as method]` suffix
// (issue #1508), run verbatim on Node and on `jsse --node`; Node's output is
// pinned in expected-async-1508.txt and replayed by tests/node_stack_frames_1508.rs.
// Frames print without their paths, and only the ones this fixture defines
// (plus the combinator frames, which have no file) are kept.
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

async function leaf() {
  await null;
  return new Error("x").stack;
}
async function slow() {
  await null;
  await null;
  return new Error("slow").stack;
}
async function viaAll() {
  return (await Promise.all([1, leaf()]))[1];
}
async function viaSettled() {
  return (await Promise.allSettled([leaf(), 2]))[0].value;
}
async function viaAny() {
  return await Promise.any([Promise.reject(1), leaf()]);
}
async function viaRace() {
  return await Promise.race([leaf()]);
}
async function nested() {
  return (await Promise.all([viaAll()]))[0];
}
async function viaAllThen() {
  return await Promise.all([leaf()]).then((v) => v[0]);
}
async function raceOfAll() {
  return await Promise.race([Promise.all([leaf()]).then((v) => v[0])]);
}
async function manyElements() {
  const results = await Promise.all([slow(), leaf(), Promise.resolve(3)]);
  return results[1];
}
async function sharedPromise() {
  const shared = leaf();
  const [stack] = await Promise.all([shared, shared]);
  return stack;
}
async function viaResolve() {
  return await new Promise((resolve) => leaf().then(resolve));
}
async function viaCombinatorOnly() {
  const stacks = [];
  await Promise.all([
    (async function inner() {
      await null;
      stacks.push(new Error("inner").stack);
    })(),
  ]);
  return stacks[0];
}

async function main() {
  show("Promise.all", await viaAll());
  show("Promise.allSettled", await viaSettled());
  show("Promise.any", await viaAny());
  show("Promise.race has no frame", await viaRace());
  show("nested Promise.all", await nested());
  show("Promise.all then handler", await viaAllThen());
  show("race of all", await raceOfAll());
  show("several elements", await manyElements());
  show("a promise with two reactions", await sharedPromise());
  show("a resolve function", await viaResolve());
  show("combinator not awaited by the leaf's caller", await viaCombinatorOnly());
}

function named() {
  return new Error("named").stack;
}
class Widget {
  method() {
    return new Error("method").stack;
  }
  get value() {
    return new Error("getter").stack;
  }
  static make() {
    return new Error("static").stack;
  }
}
const holder = { other: named, again: named };
const single = { other: named };
const inherited = Object.create({ shared: named });
const widget = new Widget();
widget.alias = widget.method;
const exported = {};
exported.run = function () {
  return new Error("inferred").stack;
};
Widget.alias = Widget.make;
function Timeoutish() {}
Timeoutish.prototype.tm = named;
const timer = new Timeoutish();
timer._onTimeout = named;
const hidden = {};
Object.defineProperty(hidden, "secret", { value: named, enumerable: false });
const accessors = {
  get prop() {
    return new Error("accessor").stack;
  },
};

function Gadget() {}
Gadget.prototype.build = function Gadget() {
  return new Error("same name").stack;
};

function methods() {
  show("a function named like its receiver type", new Gadget().build());
  show("an alias", single.other());
  show("two aliases are ambiguous", holder.other());
  show("an inherited alias", inherited.shared());
  show("a class method called by its name", widget.method());
  show("a class method called through an alias", widget.alias());
  const getter = Object.getOwnPropertyDescriptor(Widget.prototype, "value").get;
  show("a getter called explicitly", getter.call(widget));
  show("a getter of an object literal", Object.getOwnPropertyDescriptor(accessors, "prop").get.call(accessors));
  show("a static method", Widget.make());
  show("a static method through an alias", Widget.alias());
  show("a function inferred from an assignment", exported.run());
  show("a prototype method and an own alias", timer.tm());
  show("an own alias beside a prototype method", timer._onTimeout());
  show("a non-enumerable alias", hidden.secret());
  show("call with another receiver", named.call(single));
  show("a plain call", named());
}

methods();
main();
