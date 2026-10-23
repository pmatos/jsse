//! Host hooks for the in-process `execute` fuzz target: captured `print` /
//! `console.*` output, a virtual timer clock and a fixed `Math.random` seed.
//! All inert by default.

use super::Interpreter;

const MIX: u64 = 0x9E37_79B9_7F4A_7C15;

#[derive(Default)]
pub(crate) struct FuzzHost {
    capture: Option<String>,
    pub(crate) math_random_seed: Option<u64>,
}

#[derive(Clone, Copy)]
pub(crate) enum HostStream {
    Out,
    Err,
}

impl Interpreter {
    /// Capture `print` / `console.*` output into a buffer instead of writing
    /// to the process stdout/stderr; each line is prefixed `out: ` / `err: `.
    pub(crate) fn capture_host_output(&mut self) {
        self.fuzz_host.capture = Some(String::new());
    }

    pub(crate) fn take_captured_host_output(&mut self) -> String {
        self.fuzz_host.capture.take().unwrap_or_default()
    }

    /// Fire timers in deadline order without sleeping: when nothing is due the
    /// event loop jumps straight to the earliest deadline.
    pub(crate) fn use_virtual_timers(&mut self) {
        self.scheduler.timers_mut().use_virtual_clock();
    }

    /// Seed `Math.random` in every realm, current and future, from `seed`
    /// and the realm's index in this interpreter.
    pub(crate) fn set_math_random_seed(&mut self, seed: u64) {
        self.fuzz_host.math_random_seed = Some(seed);
        for index in 0..self.realms.len() {
            self.seed_new_realm_random(index);
        }
    }

    pub(crate) fn seed_new_realm_random(&mut self, realm_index: usize) {
        if let Some(seed) = self.fuzz_host.math_random_seed {
            self.realms[realm_index].math_random_state =
                seed.wrapping_add((realm_index as u64).wrapping_mul(MIX));
        }
    }

    /// Write one line of host output: to the capture buffer when one is
    /// installed, otherwise straight to the process stream.
    pub(crate) fn host_write_line(&mut self, stream: HostStream, line: &str) {
        if let Some(buf) = self.fuzz_host.capture.as_mut() {
            buf.push_str(match stream {
                HostStream::Out => "out: ",
                HostStream::Err => "err: ",
            });
            buf.push_str(line);
            buf.push('\n');
            return;
        }
        match stream {
            HostStream::Out => println!("{line}"),
            HostStream::Err => {
                use std::io::Write as _;
                let _ = std::io::stderr().write_all(format!("{line}\n").as_bytes());
            }
        }
    }

    /// Jump the virtual clock to the earliest armed timer and fire what is due
    /// there. False when there is no timer to fire.
    pub(crate) fn fast_forward_to_next_timer(&mut self) -> bool {
        let Some(at) = self.scheduler.next_timer_deadline() else {
            return false;
        };
        let now = self.scheduler.timers_mut().advance_clock_to(at);
        self.run_due_timers_at(now)
    }
}
