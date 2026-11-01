//! The web-streams slice of the embedded builtin-module table (issue #1360).
//! Node's `internal/webstreams/{readablestream,writablestream,
//! transformstream,adapters}.js` are 25-118 KB, over the 20,000-byte
//! file budget, so each is stored as `part1.js`, `part2.js`, ... in a
//! directory named for the module and joined here at compile time: the module
//! source is the upstream text, cut at line boundaries, so declarations keep
//! sharing one module scope. See
//! `docs/adr/2026-10-31-0900-node-host-web-streams.md`.

macro_rules! single {
    ($name:literal) => {
        (
            $name,
            include_str!(concat!("../../../node-lib/", $name, ".js")),
        )
    };
}

macro_rules! joined {
    ($name:literal, [$($part:literal),+ $(,)?]) => {
        (
            $name,
            concat!($(include_str!(concat!("../../../node-lib/", $name, "/", $part, ".js"))),+),
        )
    };
}

pub(super) const WEBSTREAMS_MODULES: &[(&str, &str)] = &[
    single!("stream/web"),
    single!("stream/consumers"),
    single!("internal/encoding"),
    single!("internal/fs/webstream"),
    single!("internal/errors/webstreams"),
    single!("internal/per_context/safe_promise"),
    single!("internal/process/task_queues"),
    single!("internal/worker/io"),
    single!("internal/worker/message_event"),
    single!("internal/worker/js_transferable"),
    single!("internal/webstreams/compression"),
    single!("internal/webstreams/encoding"),
    single!("internal/webstreams/queuingstrategies"),
    single!("internal/webstreams/transfer"),
    single!("internal/webstreams/util"),
    joined!("internal/webstreams/adapters", ["part1", "part2"]),
    joined!(
        "internal/webstreams/readablestream",
        [
            "part1", "part2", "part3", "part4", "part5", "part6", "part7", "part8"
        ]
    ),
    joined!("internal/webstreams/transformstream", ["part1", "part2"]),
    joined!(
        "internal/webstreams/writablestream",
        ["part1", "part2", "part3"]
    ),
];
