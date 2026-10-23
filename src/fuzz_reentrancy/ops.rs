//! The re-entered operations and target shapes of the `reentrancy` fuzz target.
//! Each operation is a JS fragment over `W` (the receiver, possibly a Proxy
//! around the target `T`) and `CB` (a callback that may re-enter).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    Array,
    Sparse,
    Holes,
    Map,
    Set,
    Object,
    Typed,
}

impl Shape {
    pub(crate) const ALL: [Shape; 7] = [
        Shape::Array,
        Shape::Sparse,
        Shape::Holes,
        Shape::Map,
        Shape::Set,
        Shape::Object,
        Shape::Typed,
    ];

    pub(crate) fn init(self) -> &'static str {
        match self {
            Shape::Array => "[E, 2, 3, 4]",
            Shape::Sparse => "(function () { var a = [1, , 3]; a[5] = E; return a; })()",
            Shape::Holes => "(function () { var a = new Array(4); a[2] = E; return a; })()",
            Shape::Map => "new Map([[1, E], [2, 2], [3, 3]])",
            Shape::Set => "new Set([1, E, 3])",
            Shape::Object => "({ a: 1, b: E, 1: 2, 2: 3 })",
            Shape::Typed => "new Uint8Array(new ArrayBuffer(4, { maxByteLength: 8 }))",
        }
    }
}

const ARRAYISH: &[Shape] = &[Shape::Array, Shape::Sparse, Shape::Holes];
const MAP: &[Shape] = &[Shape::Map];
const SET: &[Shape] = &[Shape::Set];
const COLLECTION: &[Shape] = &[Shape::Map, Shape::Set];
const TYPED: &[Shape] = &[Shape::Typed];
const ITERABLE: &[Shape] = &[
    Shape::Array,
    Shape::Sparse,
    Shape::Map,
    Shape::Set,
    Shape::Typed,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    Get,
    Set,
    Has,
    Delete,
    ForIn,
    Keys,
    Entries,
    Assign,
    DefineProperty,
    Freeze,
    Descriptors,
    OwnKeys,
    SetPrototype,
    ToPrimitive,
    Spread,
    Destructure,
    ArrayFrom,
    YieldStar,
    PromiseAll,
    Json,
    Sort,
    Map,
    Filter,
    Splice,
    Concat,
    Flat,
    Join,
    Reverse,
    Push,
    Apply,
    MapForEach,
    SetForEach,
    ForOf,
    NewCollection,
    SetMethods,
    CollectionIterator,
    TypedFill,
    TypedSet,
    TypedSlice,
    TypedSort,
    Replace,
}

impl Op {
    pub(crate) const ALL: [Op; 41] = [
        Op::Get,
        Op::Set,
        Op::Has,
        Op::Delete,
        Op::ForIn,
        Op::Keys,
        Op::Entries,
        Op::Assign,
        Op::DefineProperty,
        Op::Freeze,
        Op::Descriptors,
        Op::OwnKeys,
        Op::SetPrototype,
        Op::ToPrimitive,
        Op::Spread,
        Op::Destructure,
        Op::ArrayFrom,
        Op::YieldStar,
        Op::PromiseAll,
        Op::Json,
        Op::Sort,
        Op::Map,
        Op::Filter,
        Op::Splice,
        Op::Concat,
        Op::Flat,
        Op::Join,
        Op::Reverse,
        Op::Push,
        Op::Apply,
        Op::MapForEach,
        Op::SetForEach,
        Op::ForOf,
        Op::NewCollection,
        Op::SetMethods,
        Op::CollectionIterator,
        Op::TypedFill,
        Op::TypedSet,
        Op::TypedSlice,
        Op::TypedSort,
        Op::Replace,
    ];

    /// Target shapes this operation is meaningful on; the plan's shape byte
    /// indexes into this list.
    pub(crate) fn shapes(self) -> &'static [Shape] {
        match self {
            Op::Get
            | Op::Set
            | Op::Has
            | Op::Delete
            | Op::ForIn
            | Op::Keys
            | Op::Entries
            | Op::Assign
            | Op::DefineProperty
            | Op::Freeze
            | Op::Descriptors
            | Op::OwnKeys
            | Op::SetPrototype
            | Op::ToPrimitive
            | Op::Json
            | Op::Replace => &Shape::ALL,
            Op::Spread
            | Op::Destructure
            | Op::ArrayFrom
            | Op::YieldStar
            | Op::PromiseAll
            | Op::ForOf => ITERABLE,
            Op::Sort
            | Op::Map
            | Op::Filter
            | Op::Splice
            | Op::Concat
            | Op::Flat
            | Op::Join
            | Op::Reverse
            | Op::Push
            | Op::Apply => ARRAYISH,
            Op::MapForEach => MAP,
            Op::SetForEach | Op::SetMethods => SET,
            Op::NewCollection | Op::CollectionIterator => COLLECTION,
            Op::TypedFill | Op::TypedSet | Op::TypedSlice | Op::TypedSort => TYPED,
        }
    }

    /// JS statements performing the operation. May use `trace`, `W`, `CB`, `E`.
    pub(crate) fn body(self) -> &'static str {
        match self {
            Op::Get => "W[1]; W.a; W.length;",
            Op::Set => "W[1] = 7; W.a = 8;",
            Op::Has => "1 in W; 'a' in W;",
            Op::Delete => "delete W[1]; delete W.a;",
            Op::ForIn => "for (var k in W) { trace(k); }",
            Op::Keys => "Object.keys(W);",
            Op::Entries => "Object.entries(W);",
            Op::Assign => "Object.assign({}, W); Object.assign(W, { x: 1, 1: 2 });",
            Op::DefineProperty => {
                "Object.defineProperty(W, '1', { value: 3, configurable: true, writable: true, enumerable: true });"
            }
            Op::Freeze => "Object.freeze(W); Object.isFrozen(W);",
            Op::Descriptors => "Object.getOwnPropertyDescriptors(W);",
            Op::OwnKeys => "Reflect.ownKeys(W);",
            Op::SetPrototype => "Object.setPrototypeOf(W, Object.getPrototypeOf(W));",
            Op::ToPrimitive => "'' + W; W == 1; String(W);",
            Op::Spread => "[...W];",
            Op::Destructure => "var [p, q, r] = W;",
            Op::ArrayFrom => "Array.from(W, CB);",
            Op::YieldStar => "function* d() { yield* W; } for (var x of d()) { trace('x'); }",
            Op::PromiseAll => "Promise.all(W).then(CB, CB);",
            Op::Json => "JSON.stringify(W, CB); JSON.parse('[1, {\"a\": 2}, 3]', CB);",
            Op::Sort => "Array.prototype.sort.call(W, CB); Array.prototype.sort.call(W);",
            Op::Map => "Array.prototype.map.call(W, CB);",
            Op::Filter => "Array.prototype.filter.call(W, CB);",
            Op::Splice => "Array.prototype.splice.call(W, 1, 1, 9, 9);",
            Op::Concat => "[].concat(W, W);",
            Op::Flat => "[W, [W]].flat(2);",
            Op::Join => "Array.prototype.join.call(W);",
            Op::Reverse => "Array.prototype.reverse.call(W); Array.prototype.includes.call(W, 3);",
            Op::Push => "Array.prototype.push.call(W, 1, 2); Array.prototype.pop.call(W);",
            Op::Apply => "Math.max.apply(null, W); Reflect.construct(Array, W);",
            Op::MapForEach | Op::SetForEach => "W.forEach(CB);",
            Op::ForOf => "for (var y of W) { trace('y'); }",
            Op::NewCollection => "new Map(W); new Set(W);",
            Op::SetMethods => "new Set([1, 2]).union(W); new Set([1, 2, 3]).isSubsetOf(W);",
            Op::CollectionIterator => {
                "var it = W.values(); it.next(); it.next(); it.next(); it.next();"
            }
            Op::TypedFill => "W.fill(E);",
            Op::TypedSet => "W.set([E, E]);",
            Op::TypedSlice => "W.slice(E, E);",
            Op::TypedSort => "W.sort(CB);",
            Op::Replace => "'a-b-c'.replace(/-/g, CB);",
        }
    }
}
