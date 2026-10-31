// V8 message text and error positions of engine-thrown errors (#1419), run
// verbatim on Node and on `jsse --node` by scripts/run-node-v8-messages-oracle.sh.
// Every case prints the error's name and message and its top stack frame, which
// carries the line and column V8 reports.
const o = {};
const n = null;
const u = undefined;
const k = "kk";
const num = 1;
const arr = [];
class Plain {}
class Other {
  method() {
    return this.missing();
  }
  static make() {
    return new this.Nope();
  }
}

const cases = {
  callMember() {
    o.f();
  },
  callDeepMember() {
    o.a.b.c();
  },
  callBare() {
    let f;
    f();
  },
  callComputedString() {
    o["f"]();
  },
  callComputedNumber() {
    o[1]();
  },
  callComputedName() {
    o[k]();
  },
  callIndexed() {
    arr[0]();
  },
  callThisMember() {
    new Other().method();
  },
  callParenthesized() {
    (o.f)();
  },
  callOptional() {
    o?.f();
  },
  callNonFunctionValue() {
    const v = { f: 1 };
    v.f();
  },
  callResultOfCall() {
    const m = new Map();
    m.get(1)();
  },
  callAfterNewline() {
    o
      .g();
  },
  newBare() {
    const x = 1;
    new x();
  },
  newMember() {
    new o.F();
  },
  newThisMember() {
    Other.make();
  },
  newArrow() {
    const f = () => 1;
    new f();
  },
  newWithoutArguments() {
    new o.Nope;
  },
  classWithoutNew() {
    Plain();
  },
  readNull() {
    return null.x;
  },
  readUndefined() {
    return undefined.x;
  },
  readVariable() {
    return n.x;
  },
  readChain() {
    return o.x.y;
  },
  readDeepChain() {
    return o.a.b.c;
  },
  readComputedNumber() {
    return o[1].y;
  },
  readComputedString() {
    return o["a-b"].y;
  },
  readComputedNameOnNull() {
    return n[k];
  },
  readComputedNumberOnUndefined() {
    return u[0];
  },
  readAfterCall() {
    const m = new Map();
    return m.get(1).x;
  },
  readAfterCallChain() {
    const m = new Map();
    return m.get(1).x.y;
  },
  readMethodOnResult() {
    const m = new Map();
    m.get(1).x();
  },
  readInsideFunction() {
    return (function () {
      return this.x.y;
    })();
  },
  readNestedMember() {
    const p = { q: {} };
    return p.q.r.s;
  },
  readSymbolKey() {
    return n[Symbol.iterator];
  },
  writeChain() {
    o.x.y = 1;
  },
  writeNull() {
    null.x = 1;
  },
  writeVariable() {
    n.a = 1;
  },
  writeComputed() {
    u[k] = 1;
  },
  writeDeepChain() {
    const x = {};
    x.y.z.w = 1;
  },
  compoundWrite() {
    o.x.y += 1;
  },
  compoundComputed() {
    o.x[k] += 1;
  },
  inOperator() {
    return 1 in num;
  },
  inOperatorString() {
    return "a" in num;
  },
  instanceofNonObject() {
    return {} instanceof num;
  },
  instanceofNonCallable() {
    return {} instanceof {};
  },
  extendsNumber() {
    class B extends num {}
    return B;
  },
  extendsUndefined() {
    class B extends u {}
    return B;
  },
};

// Message only: jsse does not yet report V8's position for these (operator
// and heritage positions, and the last sub-expression of a compound
// assignment); see known-failures.md.
const messageOnly = new Set([
  "compoundWrite",
  "compoundComputed",
  "inOperator",
  "inOperatorString",
  "instanceofNonObject",
  "instanceofNonCallable",
  "extendsNumber",
  "extendsUndefined",
]);

for (const name of Object.keys(cases)) {
  try {
    cases[name]();
    console.log("## " + name + "\nno error");
  } catch (e) {
    const frame = messageOnly.has(name) ? "" : String(e.stack).split("\n")[1];
    const head = "## " + name + "\n" + e.name + ": " + e.message;
    console.log(frame ? head + "\n" + frame.trim() : head);
  }
}
