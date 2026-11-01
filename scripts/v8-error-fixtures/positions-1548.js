// Statement and keyword positions in async frames (issue #1548): V8 reports
// the last position it recorded when a frame parks where no expression has one
// of its own. Every shape here is one `jsse --node` reproduces, in the
// bytecode VM and the tree-walker; `expected-positions-1548.txt` is Node 26.8's
// output. Shapes it does not reproduce are listed in
// docs/agents/library-tests-typescript-conformance-generator-positions-1548.md.
const path = require("node:path");
const dir = path.dirname(__filename);

const seen = [];
function capture(label) {
  const lines = String(new Error(label).stack)
    .split("\n")
    .filter((line) => /^\s+at /.test(line) && line.includes(dir + "/"));
  seen.push("## " + label + "\n" + lines.join("\n").split(dir).join("<dir>"));
}

class Resource {
  async [Symbol.asyncDispose]() {
    await null;
    capture(Resource.current);
  }
}
let a = 1;
let b = 2;
const o = { p: 1 };
function f() {}

const declarations = {
  async constAddition() {
    await using one = new Resource();
    const x = a + b;
  },
  async constFolded() {
    await using one = new Resource();
    const x = 1 + 2;
  },
  async constMember() {
    await using one = new Resource();
    const x = o.p;
  },
  async constCall() {
    await using one = new Resource();
    const x = f();
  },
  async constLiteral() {
    await using one = new Resource();
    const x = 3;
  },
  async constArray() {
    await using one = new Resource();
    const x = [1, 2];
  },
  async constParenthesised() {
    await using one = new Resource();
    const x = (1, 2);
  },
  async constTemplate() {
    await using one = new Resource();
    const x = `t`;
  },
  async constFunction() {
    await using one = new Resource();
    const x = function () {};
  },
  async twoDeclarators() {
    await using one = new Resource();
    let q = 1,
      r = 2;
  },
  async varDeclaration() {
    await using one = new Resource();
    var z = 1;
  },
  async nestedBlock() {
    await using one = new Resource();
    {
      const x = 5;
    }
  },
  async inIf() {
    await using one = new Resource();
    if (a) {
      const x = 5;
    }
  },
  async inTry() {
    await using one = new Resource();
    try {
      const x = 5;
    } finally {
    }
  },
  async onlyTheResource() {
    await using one = new Resource();
  },
};

async function* source() {
  await null;
  capture("source");
  yield 1;
}
async function* delegates() {
  yield* source();
}
async function* delegatesToAnIdentifier() {
  const inner = source();
  yield* inner;
}
function* syncSource() {
  yield 1;
  capture("sync source");
}
function* syncDelegates() {
  const inner = syncSource();
  yield* inner;
}
async function* disposesAfterYield() {
  await using one = new Resource();
  yield 1;
}
async function* disposesAfterYieldOfCall() {
  await using one = new Resource();
  yield f();
}

async function* closes() {
  try {
    yield 1;
    yield 2;
  } finally {
    await null;
    capture(closes.current);
  }
}
const loops = {
  async breakInLoop() {
    for await (const x of closes()) break;
  },
  async breakInBlock() {
    for await (const x of closes()) {
      break;
    }
  },
  async breakAfterStatement() {
    for await (const x of closes()) {
      const y = 2;
      break;
    }
  },
  async breakAfterAwait() {
    for await (const x of closes()) {
      await null;
      break;
    }
  },
  async breakUnderConstantIf() {
    for await (const x of closes()) {
      if (true) break;
    }
  },
  async labeledBreak() {
    outer: {
      for await (const x of closes()) {
        break outer;
      }
    }
  },
};

async function main() {
  for (const [name, fn] of Object.entries(declarations)) {
    Resource.current = name;
    await fn();
  }
  for await (const s of delegates()) {
  }
  for (const s of syncDelegates()) {
  }
  for await (const s of delegatesToAnIdentifier()) {
  }
  for (const gen of [disposesAfterYield, disposesAfterYieldOfCall]) {
    Resource.current = gen.name;
    for await (const x of gen()) break;
  }
  for (const [name, fn] of Object.entries(loops)) {
    closes.current = name;
    await fn();
  }
  console.log(seen.join("\n"));
}
main();
