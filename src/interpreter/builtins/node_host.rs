//! Node host-compat "syscall floor" (issue #229).
//!
//! The genuinely-cannot-be-pure-JS primitives — byte I/O, OS entropy, a
//! monotonic high-resolution clock, and the process-exit path — exposed to the
//! Node prelude only and gated behind the `--node` CLI flag. When the gate is
//! off (the default, and always the case under test262) none of these globals
//! are installed and every host-floor check elsewhere in the interpreter is
//! inert, so the default global environment is byte-identical to a build
//! without this feature.
//!
//! The primitives are installed as **non-enumerable** properties named
//! `__host_*` on the global object. They are internal plumbing: the higher-level
//! `process` / `Buffer` / `performance` surfaces are built on top of them in the
//! JS shim (issues #230+).

use super::super::*;
use super::typedarray::create_uint8array_from_bytes;
use std::io::Write;

impl Interpreter {
    /// Turn on the Node host floor and install the `__host_*` primitives.
    ///
    /// Must run after `setup_globals` (so the global object exists) and before
    /// the Node prelude/main are executed. Never called under test262.
    pub fn enable_node_host(&mut self) {
        self.node_host_enabled = true;
        self.host_clock_start = Some(std::time::Instant::now());
        self.setup_node_host();
    }

    /// Install a `__host_*` primitive as a non-enumerable, writable,
    /// configurable property on the realm's global object. This makes it
    /// reachable both as a bare identifier and as `globalThis.__host_*`.
    fn install_host_global(&mut self, name: &str, val: JsValue) {
        let Some(gid) = self.realm().global_object else {
            return;
        };
        self.get_object_cell_expect(gid)
            .borrow_mut()
            .insert_property(
                name.to_string(),
                PropertyDescriptor::data(val, true, false, true),
            );
    }

    fn setup_node_host(&mut self) {
        // __host_write(fd, str) -> bytesWritten
        //
        // Byte-accurate write of the UTF-8 encoding of `str` (from jsse's
        // internal UTF-16; lone surrogates become U+FFFD, matching Node) to
        // stdout (fd 1) or stderr (fd 2). No forced newline. Backs
        // process.stdout/stderr.write.
        let write_fn = self.create_function(JsFunction::native(
            "__host_write".to_string(),
            2,
            |interp, _this, args| {
                let fd = match interp.to_number_value(args.first().unwrap_or(&JsValue::Undefined)) {
                    Ok(n) => n,
                    Err(e) => return Completion::Throw(e),
                };
                let s = match interp.to_string_value(args.get(1).unwrap_or(&JsValue::Undefined)) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let bytes = s.as_bytes();
                let written = bytes.len();
                if fd == 2.0 {
                    let mut err = std::io::stderr().lock();
                    let _ = err.write_all(bytes);
                    let _ = err.flush();
                } else {
                    let mut out = std::io::stdout().lock();
                    let _ = out.write_all(bytes);
                    let _ = out.flush();
                }
                Completion::Normal(JsValue::Number(written as f64))
            },
        ));
        self.install_host_global("__host_write", write_fn);

        // __host_exit(code)
        //
        // Records the process exit code and unwinds uncatchably. The returned
        // sentinel Throw stops execution; `pending_exit` — not the thrown value
        // — is the signal that try/catch, the microtask drain loop, and `main`
        // all honor, so the exit is not swallowed by user `catch`/`finally` or
        // by a Promise reaction. Backs process.exit.
        let exit_fn = self.create_function(JsFunction::native(
            "__host_exit".to_string(),
            1,
            |interp, _this, args| {
                let code = match args.first() {
                    None | Some(JsValue::Undefined) => 0,
                    Some(v) => match interp.to_number_value(v) {
                        Ok(n) if n.is_finite() => n.trunc() as i64 as i32,
                        Ok(_) => 0,
                        Err(e) => return Completion::Throw(e),
                    },
                };
                interp.pending_exit = Some(code);
                Completion::Throw(JsValue::Undefined)
            },
        ));
        self.install_host_global("__host_exit", exit_fn);

        // __host_hrtime() -> BigInt
        //
        // Monotonic nanoseconds since the host floor was enabled. A BigInt keeps
        // full nanosecond precision, backing both performance.now (÷ 1e6) and
        // process.hrtime.bigint().
        let hrtime_fn = self.create_function(JsFunction::native(
            "__host_hrtime".to_string(),
            0,
            |interp, _this, _args| {
                let ns = interp
                    .host_clock_start
                    .map(|start| start.elapsed().as_nanos())
                    .unwrap_or(0);
                Completion::Normal(JsValue::BigInt(JsBigInt {
                    value: num_bigint::BigInt::from(ns),
                }))
            },
        ));
        self.install_host_global("__host_hrtime", hrtime_fn);

        // __host_random_bytes(n) -> Uint8Array
        //
        // A fresh Uint8Array of `n` cryptographically-secure bytes from the OS
        // entropy source. Backs getRandomValues / crypto.randomBytes. (jsse's
        // Math.random is deterministic, so this is the only real entropy.)
        let random_fn = self.create_function(JsFunction::native(
            "__host_random_bytes".to_string(),
            1,
            |interp, _this, args| {
                let n_f = match interp
                    .to_integer_or_infinity_value(args.first().unwrap_or(&JsValue::Undefined))
                {
                    Ok(n) => n,
                    Err(e) => return Completion::Throw(e),
                };
                // Match Node's crypto.randomBytes ceiling (2**31 - 1 bytes).
                if !(0.0..=2_147_483_647.0).contains(&n_f) {
                    return Completion::Throw(
                        interp.create_range_error("__host_random_bytes: size out of range"),
                    );
                }
                let mut buf = vec![0u8; n_f as usize];
                if getrandom::fill(&mut buf).is_err() {
                    return Completion::Throw(interp.create_error(
                        "Error",
                        "__host_random_bytes: OS entropy source unavailable",
                    ));
                }
                create_uint8array_from_bytes(interp, &buf)
            },
        ));
        self.install_host_global("__host_random_bytes", random_fn);
    }
}
