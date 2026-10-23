//! Structure-aware input for the `reentrancy` fuzz target: raw fuzzer bytes
//! decode into a [`Plan`] (which operation is re-entered, by which Proxy trap,
//! generator step, accessor or callback, and what the callback does), and the
//! plan renders to a deterministic JS program that goes through the `execute`
//! oracle.

mod hosts;
mod ops;

use hosts::{Action, Host};
use ops::{Op, Shape};

const MAX_SCENARIOS: usize = 4;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Scenario {
    pub(crate) op: Op,
    pub(crate) host: Host,
    pub(crate) hook: usize,
    pub(crate) action: Action,
    pub(crate) step: usize,
    pub(crate) shape: Shape,
}

#[derive(Debug)]
pub(crate) struct Plan {
    pub(crate) scenarios: Vec<Scenario>,
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    /// Next byte reduced modulo `n`; reading past the end yields 0 so every
    /// input decodes.
    fn next(&mut self, n: usize) -> usize {
        let byte = self.data.get(self.pos).copied().unwrap_or(0);
        self.pos += 1;
        usize::from(byte) % n
    }
}

const PRELUDE: &str = "\
var log = [], hits = 0, acts = 0, depth = 0, bad = false, snapping = false, STEP = @STEP@;
function trace(t) { if (log.length < 40) log.push(t); }
var E = {};
var T = @INIT@, W = T, G, CB = function () { trace('cb'); return 0; };
var gen = function* () { for (var i = 0; i < 3; i++) { trace('g' + i); yield i; } };
function act() {
  if (snapping || acts >= 4 || hits <= STEP) return;
  acts++;
  @ACT@
}
function fire(tag) { trace(tag); hits++; act(); }
function acc(f) {
  ['1', 'a'].forEach(function (k) {
    try { Object.defineProperty(T, k, { get: f, set: f, configurable: true, enumerable: true }); } catch (e) { trace('D' + e.name); }
  });
}
function iterable() { try { T[Symbol.iterator] = function () { return gen(); }; } catch (e) { trace('I' + e.name); } }
function isColl() { return T instanceof Map || T instanceof Set; }
function grow() {
  if (Array.isArray(T)) Array.prototype.push.call(T, 5, 6);
  else if (T instanceof Map) T.set(9, 9);
  else if (T instanceof Set) T.add(9);
  else if (ArrayBuffer.isView(T)) T.buffer.resize(8);
  else T.z = 1;
}
function shrink() {
  if (Array.isArray(T)) T.length = 1;
  else if (isColl()) T.clear();
  else if (ArrayBuffer.isView(T)) T.buffer.resize(1);
  else delete T.a;
}
function drop() {
  if (Array.isArray(T)) delete T[1];
  else if (isColl()) T.delete(1);
  else if (ArrayBuffer.isView(T)) T.buffer.transfer();
  else delete T.b;
}
function snap() {
  try {
    return Reflect.ownKeys(T).map(String).join('|') + '#' +
      (isColl() ? Array.from(T).map(String).join(';') : '') + '#' +
      (T.length !== undefined ? T.length : '') + '#' +
      (ArrayBuffer.isView(T) ? T.buffer.byteLength : '') + '#' +
      Object.isFrozen(T) + Object.isExtensible(T);
  } catch (e) { return e.name; }
}
@SETUP@
if (!G) G = gen();
function runop() { @OP@ }
try { runop(); } catch (e) { trace('E' + e.name); }
setTimeout(function () { snapping = true; print(log.join()); print(snap()); }, 0);
";

impl Plan {
    /// Total decoder: any byte string yields a plan of 1..=4 scenarios.
    pub(crate) fn decode(data: &[u8]) -> Plan {
        let mut cur = Cursor { data, pos: 0 };
        let count = 1 + cur.next(MAX_SCENARIOS);
        let scenarios = (0..count)
            .map(|_| {
                let op = Op::ALL[cur.next(Op::ALL.len())];
                let host = Host::ALL[cur.next(Host::ALL.len())];
                let hook = cur.next(256);
                let action = Action::ALL[cur.next(Action::ALL.len())];
                let step = cur.next(4);
                let shapes = op.shapes();
                let shape = shapes[cur.next(256) % shapes.len()];
                Scenario {
                    op,
                    host,
                    hook,
                    action,
                    step,
                    shape,
                }
            })
            .collect();
        Plan { scenarios }
    }

    pub(crate) fn render(&self) -> String {
        self.scenarios
            .iter()
            .map(Scenario::render)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Scenario {
    pub(crate) fn render(&self) -> String {
        let body = self.action.body();
        let act = if self.action.propagates() {
            body.to_string()
        } else {
            format!("try {{ {body} }} catch (e) {{ trace('A' + e.name); }}")
        };
        let program = PRELUDE
            .replace("@STEP@", &self.step.to_string())
            .replace("@INIT@", self.shape.init())
            .replace("@ACT@", &act)
            .replace("@SETUP@", &self.host.setup(self.hook))
            .replace("@OP@", self.op.body());
        format!("try {{ (function () {{\n{program}}})(); }} catch (e) {{ print('S' + e.name); }}")
    }
}

#[cfg(test)]
mod tests;
