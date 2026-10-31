// V8 text of call spread, array pattern and tagged template errors (#1512),
// run verbatim on Node and on `jsse --node`. Every case prints the error's name
// and message; the tagged template cases also print the top stack frame. The
// position of a spread or pattern error is open (#1511).
const five = 5;
const u = undefined;
const n = null;
const obj = {};
const bad = { [Symbol.iterator]: 1 };
const o = { p: 5, f() {} };
function f() {
  return 5;
}
const tag = () => 1;

const cases = {
  spreadNumber() {
    f(...five);
  },
  spreadLiteral() {
    f(...5);
  },
  spreadObject() {
    f(...obj);
  },
  spreadBadIterator() {
    f(...bad);
  },
  spreadUndefined() {
    f(...u);
  },
  spreadUndefinedLiteral() {
    f(...undefined);
  },
  spreadNull() {
    f(...n);
  },
  spreadMember() {
    o.f(...u);
  },
  spreadMemberNumber() {
    o.f(...five);
  },
  spreadSecond() {
    f(1, ...five);
  },
  spreadNew() {
    new f(...five);
  },
  arraySpreadBad() {
    return [...bad];
  },
  arrayPatternVariable() {
    var [a] = five;
  },
  arrayPatternLet() {
    let [a] = five;
  },
  arrayPatternRest() {
    var [...r] = five;
  },
  arrayPatternObject() {
    var [a] = obj;
  },
  arrayPatternLiteral() {
    var [a] = 5;
  },
  arrayPatternObjectLiteral() {
    var [a] = {};
  },
  arrayPatternCall() {
    var [a] = f();
  },
  arrayPatternMember() {
    var [a] = o.p;
  },
  arrayPatternBad() {
    var [a] = bad;
  },
  nestedPattern() {
    var [[a]] = [5];
  },
  nestedPatternNull() {
    var [[a]] = [null];
  },
  forOfBad() {
    for (const x of bad) {
    }
  },
  tagNotFunction() {
    o.g`a`;
  },
  tagMember() {
    o.p`a${1}b`;
  },
  tagResultNotFunction() {
    tag`a`();
  },
  tagVariable() {
    five`a`;
  },
};
for (const [name, fn] of Object.entries(cases)) {
  try {
    fn();
    console.log(name, "-> no error");
  } catch (e) {
    const top = (e.stack.split("\n")[1] || "").replace(/\(.*[\\/]/, "(");
    const frame = name.startsWith("tag") ? top : "";
    console.log([name, "->", `${e.name}: ${e.message}`, frame].filter(Boolean).join(" "));
  }
}
