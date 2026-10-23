//! Who calls back into the engine (`Host`) and what the callback does
//! (`Action`) in the `reentrancy` fuzz target.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Host {
    ProxyTrap,
    Generator,
    Accessor,
    Callback,
}

impl Host {
    pub(crate) const ALL: [Host; 4] = [
        Host::ProxyTrap,
        Host::Generator,
        Host::Accessor,
        Host::Callback,
    ];

    pub(crate) fn hook_count(self) -> usize {
        match self {
            Host::ProxyTrap => PROXY_TRAPS.len(),
            Host::Generator => 5,
            Host::Accessor => 2,
            Host::Callback => 3,
        }
    }

    /// JS statements run after `T`, `E`, `W` and the prelude helpers exist.
    /// They install the re-entry point selected by `hook` and may reassign
    /// `W`, `gen`, `CB`.
    pub(crate) fn setup(self, hook: usize) -> String {
        match self {
            Host::ProxyTrap => {
                let (trap, bogus) = PROXY_TRAPS[hook];
                let coll = "(t instanceof Map || t instanceof Set)";
                let value = if trap == "get" {
                    format!(
                        "var v = Reflect.get(a[0], a[1], {coll} ? a[0] : a[2]); \
                         return bad ? 1 : (typeof v === 'function' && {coll}) ? v.bind(a[0]) : v;"
                    )
                } else {
                    format!("return bad ? ({bogus}) : Reflect.{trap}(...a);")
                };
                format!(
                    "W = new Proxy(T, {{ {trap}: function (...a) {{ var t = a[0]; fire('{trap}'); {value} }} }});"
                )
            }
            Host::Generator => match hook {
                0 => "gen = function* () { for (var i = 0; i < 3; i++) { fire('gy' + i); yield i; } }; \
                      iterable();"
                    .into(),
                1 => "gen = function* () { try { for (var i = 0; i < 3; i++) { trace('g' + i); yield i; } } \
                      finally { fire('gf'); } }; iterable();"
                    .into(),
                2 => "function* inner() { fire('gi'); yield 1; fire('gj'); yield 2; } \
                      gen = function* () { yield 0; yield* inner(); }; iterable();"
                    .into(),
                3 => "gen = function* () { for (var i = 0; i < 3; i++) { fire('gy' + i); yield i; } }; \
                      G = gen(); CB = function () { trace('cb'); return G.next().value; };"
                    .into(),
                _ => "gen = function* () { for (var i = 0; i < 3; i++) { fire('gy' + i); yield i; } }; \
                      G = gen(); acc(function () { return G.next().value; });"
                    .into(),
            },
            Host::Accessor => match hook {
                0 => "acc(function () { fire('acc'); return 7; });".into(),
                _ => "var P = Object.create(Object.getPrototypeOf(T)); \
                      ['1', 'a'].forEach(function (k) { Object.defineProperty(P, k, \
                      { get: function () { fire('pacc'); return 7; }, \
                        set: function () { fire('pacc'); }, configurable: true }); }); \
                      try { Object.setPrototypeOf(T, P); } catch (e) { trace('P' + e.name); }"
                    .into(),
            },
            Host::Callback => match hook {
                0 => "CB = function () { fire('cb'); return 0; };".into(),
                1 => "E.valueOf = function () { fire('vo'); return 1; }; \
                      E.toString = function () { fire('ts'); return 'e'; };"
                    .into(),
                _ => "E.toJSON = function () { fire('tj'); return 1; }; \
                      E[Symbol.iterator] = function* () { fire('ei'); yield 1; };"
                    .into(),
            },
        }
    }
}

/// Proxy trap and the invariant-violating value it returns once the
/// `BadResult` action has run.
const PROXY_TRAPS: [(&str, &str); 11] = [
    ("get", "1"),
    ("set", "true"),
    ("has", "false"),
    ("deleteProperty", "true"),
    ("ownKeys", "[]"),
    ("getOwnPropertyDescriptor", "undefined"),
    ("defineProperty", "true"),
    ("getPrototypeOf", "null"),
    ("setPrototypeOf", "true"),
    ("isExtensible", "!Reflect.isExtensible(a[0])"),
    ("preventExtensions", "true"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Nothing,
    Grow,
    Shrink,
    Drop,
    Redefine,
    Freeze,
    PreventExtensions,
    Reenter,
    GenNext,
    GenReturn,
    GenThrow,
    Throw,
    BadResult,
    KeysAndSort,
    SetViaReceiver,
    DeleteViaReceiver,
}

impl Action {
    pub(crate) const ALL: [Action; 16] = [
        Action::Nothing,
        Action::Grow,
        Action::Shrink,
        Action::Drop,
        Action::Redefine,
        Action::Freeze,
        Action::PreventExtensions,
        Action::Reenter,
        Action::GenNext,
        Action::GenReturn,
        Action::GenThrow,
        Action::Throw,
        Action::BadResult,
        Action::KeysAndSort,
        Action::SetViaReceiver,
        Action::DeleteViaReceiver,
    ];

    /// An action that throws out of the callback instead of being swallowed.
    pub(crate) fn propagates(self) -> bool {
        self == Action::Throw
    }

    pub(crate) fn body(self) -> &'static str {
        match self {
            Action::Nothing => "trace('n');",
            Action::Grow => "grow();",
            Action::Shrink => "shrink();",
            Action::Drop => "drop();",
            Action::Redefine => {
                "Object.defineProperty(T, '1', { value: 0, writable: false, configurable: false });"
            }
            Action::Freeze => "Object.freeze(T);",
            Action::PreventExtensions => "Object.preventExtensions(T);",
            Action::Reenter => "if (depth < 3) { depth++; try { runop(); } finally { depth--; } }",
            Action::GenNext => "G.next();",
            Action::GenReturn => "G.return(1);",
            Action::GenThrow => "G.throw(new Error('g'));",
            Action::Throw => "throw new RangeError('t');",
            Action::BadResult => "bad = true;",
            Action::KeysAndSort => "Object.keys(W); Array.prototype.sort.call(W);",
            Action::SetViaReceiver => "W[1] = 0;",
            Action::DeleteViaReceiver => "delete W[1];",
        }
    }
}
