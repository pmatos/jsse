//! Process identity and signals for Node host mode (issue #1236):
//! `process.kill`, `process.pid` and `process.on('SIGINT' ...)`.
//!
//! A watched signal gets a handler that writes its number to a self-pipe (the
//! only async-signal-safe way out); one thread reads that pipe and posts a
//! `ChildEvent::Signal` to the host-async completion queue, which the JS
//! dispatcher turns into `process.emit(name)`. Unwatching restores the default
//! disposition. Watchers never keep the event loop alive, as in Node.

use super::super::*;
use super::node_host_child::{ChildEvent, post};
use super::node_host_fs_args::{arg, int, last_errno};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicI32, Ordering};

static SIGNAL_PIPE: AtomicI32 = AtomicI32::new(-1);
static READER: OnceLock<()> = OnceLock::new();

extern "C" fn on_signal(signal: libc::c_int) {
    let fd = SIGNAL_PIPE.load(Ordering::Relaxed);
    if fd >= 0 {
        let byte = signal as u8;
        unsafe { libc::write(fd, (&raw const byte).cast(), 1) };
    }
}

fn start_reader(interp: &Interpreter) -> Result<(), i32> {
    if READER.get().is_some() {
        return Ok(());
    }
    let mut fds = [0 as libc::c_int; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(last_errno());
    }
    for fd in fds {
        unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) };
    }
    let (read_fd, write_fd) = (fds[0], fds[1]);
    if READER.set(()).is_err() {
        unsafe {
            libc::close(read_fd);
            libc::close(write_fd);
        }
        return Ok(());
    }
    SIGNAL_PIPE.store(write_fd, Ordering::Relaxed);
    let queue = interp.agent_async_completions.clone();
    std::thread::spawn(move || {
        let mut byte = 0u8;
        loop {
            let n = unsafe { libc::read(read_fd, (&raw mut byte).cast(), 1) };
            if n == 1 {
                post(&queue, ChildEvent::Signal(i32::from(byte)));
            } else if n == 0 || last_errno() != libc::EINTR {
                return;
            }
        }
    });
    Ok(())
}

fn watch(interp: &Interpreter, signal: i32, on: bool) -> Result<(), i32> {
    if on {
        start_reader(interp)?;
    }
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = if on {
        on_signal as *const () as usize
    } else {
        libc::SIG_DFL
    };
    action.sa_flags = libc::SA_RESTART;
    unsafe { libc::sigemptyset(&mut action.sa_mask) };
    if unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) } != 0 {
        return Err(last_errno());
    }
    Ok(())
}

impl Interpreter {
    pub(super) fn setup_node_host_signal(&mut self) {
        // __host_getpid()
        self.host_fs_fn("__host_getpid", 0, |_, _| {
            Ok(JsValue::number(f64::from(unsafe { libc::getpid() })))
        });

        // __host_kill(pid, signal) -> 0 | negative errno
        self.host_fs_fn("__host_kill", 2, |interp, args| {
            let pid = int(interp, arg(args, 0))? as i32;
            let signal = int(interp, arg(args, 1))? as i32;
            let rc = unsafe { libc::kill(pid, signal) };
            Ok(JsValue::number(if rc == 0 {
                0.0
            } else {
                -f64::from(last_errno())
            }))
        });

        // __host_signal_watch(signal, on) -> 0 | negative errno
        self.host_fs_fn("__host_signal_watch", 2, |interp, args| {
            let signal = int(interp, arg(args, 0))? as i32;
            let on = interp.to_boolean_val(arg(args, 1));
            Ok(JsValue::number(match watch(interp, signal, on) {
                Ok(()) => 0.0,
                Err(errno) => -f64::from(errno),
            }))
        });
    }
}
