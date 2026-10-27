//! Terminal and standard-stream floor for Node host mode (issue #1232): what
//! `process.stdin`/`stdout`/`stderr`, `tty` and `readline` cannot do in
//! JavaScript. `__host_tty` offers handle classification (libuv's
//! `uv_guess_handle`), window size, raw mode, a synchronous full write to a
//! descriptor, and an on-demand reader for fd 0 that wakes the Node event loop
//! (`node_event_loop.rs`) when bytes arrive. See
//! `docs/adr/2026-10-26-2230-node-host-tty.md`.

use super::super::*;
use super::node_host_fs_args::{
    arg, int, js_str, last_errno, read_fd, uint8array_from_vec, view_of,
};
use super::node_host_fs_error::uv_error;
use std::collections::VecDeque;
use std::io::Write;
use std::sync::Mutex;
use std::sync::mpsc::Sender;

enum StdinMsg {
    Data(Vec<u8>),
    Eof,
    Error(i32),
}

/// fd 0 reader state, owned by `NodeLoopState`. A thread reads one chunk per
/// request; its bytes land in `pending` through the completion queue and are
/// delivered from the loop's poll phase, never from inside a microtask drain.
#[derive(Default)]
pub(crate) struct StdinState {
    cb: Option<JsValue>,
    reading: bool,
    unrefed: bool,
    eof: bool,
    requested: bool,
    pending: VecDeque<StdinMsg>,
    tx: Option<Sender<()>>,
}

impl StdinState {
    pub(crate) fn for_each_root(&self, mut visit: impl FnMut(&JsValue)) {
        if let Some(cb) = &self.cb {
            visit(cb);
        }
    }

    /// A started, ref'd reader that has not hit end of input keeps the loop up.
    pub(crate) fn keeps_alive(&self) -> bool {
        self.reading && !self.eof && !self.unrefed
    }

    pub(crate) fn has_deliverable(&self) -> bool {
        self.reading && !self.pending.is_empty()
    }

    fn request(&mut self) {
        if self.requested || self.eof {
            return;
        }
        if let Some(tx) = &self.tx
            && tx.send(()).is_ok()
        {
            self.requested = true;
        }
    }
}

static ORIGINAL_TERMIOS: Mutex<Vec<(i32, libc::termios)>> = Mutex::new(Vec::new());
static RESTORE_HOOK: std::sync::Once = std::sync::Once::new();

extern "C" fn restore_terminal() {
    if let Ok(guard) = ORIGINAL_TERMIOS.lock() {
        for (fd, original) in guard.iter() {
            unsafe { libc::tcsetattr(*fd, libc::TCSADRAIN, original) };
        }
    }
}

fn guess_handle_type(fd: i32) -> &'static str {
    if fd < 0 {
        return "INVALID";
    }
    if unsafe { libc::isatty(fd) } == 1 {
        return "TTY";
    }
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } != 0 {
        return "INVALID";
    }
    match st.st_mode & libc::S_IFMT {
        libc::S_IFREG | libc::S_IFCHR => "FILE",
        libc::S_IFIFO => "PIPE",
        libc::S_IFSOCK => {
            let mut ss: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
            let mut len = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
            let rc = unsafe { libc::getsockname(fd, (&raw mut ss).cast(), &mut len) };
            if rc == 0 && i32::from(ss.ss_family) != libc::AF_UNIX {
                "TCP"
            } else {
                "PIPE"
            }
        }
        _ => "UNKNOWN",
    }
}

fn wait_fd(fd: i32, events: i16) {
    let mut p = libc::pollfd {
        fd,
        events,
        revents: 0,
    };
    unsafe { libc::poll(&mut p, 1, -1) };
}

/// Write every byte, waiting out a non-blocking descriptor's `EAGAIN`.
fn write_blocking(fd: i32, mut buf: &[u8]) -> Result<(), i32> {
    while !buf.is_empty() {
        let n = unsafe { libc::write(fd, buf.as_ptr().cast(), buf.len()) };
        if n > 0 {
            buf = &buf[n as usize..];
            continue;
        }
        match last_errno() {
            libc::EINTR => {}
            libc::EAGAIN => wait_fd(fd, libc::POLLOUT),
            e => return Err(e),
        }
    }
    Ok(())
}

fn read_chunk(buf: &mut [u8]) -> StdinMsg {
    loop {
        match read_fd(0, buf, -1) {
            Ok(0) => return StdinMsg::Eof,
            Ok(n) => return StdinMsg::Data(buf[..n].to_vec()),
            Err(libc::EAGAIN) => wait_fd(0, libc::POLLIN),
            Err(e) => return StdinMsg::Error(e),
        }
    }
}

fn guess_handle(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let fd = match int(interp, arg(args, 0)) {
        Ok(n) => n as i32,
        Err(e) => return Completion::Throw(e),
    };
    Completion::Normal(js_str(guess_handle_type(fd)))
}

fn is_tty(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let fd = match int(interp, arg(args, 0)) {
        Ok(n) => n as i32,
        Err(e) => return Completion::Throw(e),
    };
    Completion::Normal(JsValue::boolean(
        fd >= 0 && unsafe { libc::isatty(fd) } == 1,
    ))
}

fn get_window_size(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let fd = match int(interp, arg(args, 0)) {
        Ok(n) => n as i32,
        Err(e) => return Completion::Throw(e),
    };
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws) } != 0 {
        return Completion::Normal(JsValue::UNDEFINED);
    }
    Completion::Normal(interp.create_array(vec![
        JsValue::number(f64::from(ws.ws_col)),
        JsValue::number(f64::from(ws.ws_row)),
    ]))
}

/// `setRawMode(fd, raw)` -> 0, or a uv-style error object (`code`, `errno`).
fn set_raw_mode(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let fd = match int(interp, arg(args, 0)) {
        Ok(n) => n as i32,
        Err(e) => return Completion::Throw(e),
    };
    let raw = interp.to_boolean_val(arg(args, 1));
    let mut guard = ORIGINAL_TERMIOS.lock().unwrap_or_else(|e| e.into_inner());
    let mut current: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(fd, &mut current) } != 0 {
        return Completion::Normal(uv_error(interp, last_errno(), "setRawMode", None, None));
    }
    let saved = guard.iter().position(|(f, _)| *f == fd);
    let target = match (raw, saved) {
        (true, saved) => {
            if saved.is_none() {
                guard.push((fd, current));
                RESTORE_HOOK.call_once(|| {
                    unsafe { libc::atexit(restore_terminal) };
                });
            }
            let mut t = current;
            t.c_iflag &= !(libc::BRKINT | libc::ICRNL | libc::INPCK | libc::ISTRIP | libc::IXON);
            t.c_oflag |= libc::ONLCR;
            t.c_cflag |= libc::CS8;
            t.c_lflag &= !(libc::ECHO | libc::ICANON | libc::IEXTEN | libc::ISIG);
            t.c_cc[libc::VMIN] = 1;
            t.c_cc[libc::VTIME] = 0;
            t
        }
        (false, Some(i)) => guard[i].1,
        (false, None) => return Completion::Normal(JsValue::number(0.0)),
    };
    if unsafe { libc::tcsetattr(fd, libc::TCSADRAIN, &target) } != 0 {
        return Completion::Normal(uv_error(interp, last_errno(), "setRawMode", None, None));
    }
    Completion::Normal(JsValue::number(0.0))
}

/// `write(fd, stringOrBytes)`: a synchronous, complete write; throws a uv-style
/// error (`EPIPE`, `EBADF`, ...) when it cannot finish.
fn write(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let fd = match int(interp, arg(args, 0)) {
        Ok(n) => n as i32,
        Err(e) => return Completion::Throw(e),
    };
    let data = arg(args, 1);
    let bytes = if data.is_string() {
        match interp.to_string_lossy(data) {
            Ok(s) => s.into_bytes(),
            Err(e) => return Completion::Throw(e),
        }
    } else {
        let Some(view) = view_of(interp, data) else {
            return Completion::Throw(
                interp.create_type_error("data must be a string or Uint8Array"),
            );
        };
        view.buffer.borrow().with_read(|b| {
            b.get(view.offset..view.offset + view.len)
                .map(<[u8]>::to_vec)
                .unwrap_or_default()
        })
    };
    if fd == 1 {
        let _ = std::io::stdout().flush();
    } else if fd == 2 {
        let _ = std::io::stderr().flush();
    }
    match write_blocking(fd, &bytes) {
        Ok(()) => Completion::Normal(JsValue::number(bytes.len() as f64)),
        Err(e) => Completion::Throw(uv_error(interp, e, "write", None, None)),
    }
}

/// `stdinStart(callback)`: callback(chunk) per read, callback(null) at end of
/// input, callback(undefined, error) on a read failure.
fn stdin_start(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let completions = interp.agent_async_completions.clone();
    let st = &mut interp.scheduler.node_loop.stdin;
    st.cb = Some(arg(args, 0).clone());
    st.reading = true;
    if st.tx.is_none() && !st.eof {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        st.tx = Some(tx);
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 65536];
            while rx.recv().is_ok() {
                let msg = read_chunk(&mut buf);
                let last = !matches!(msg, StdinMsg::Data(_));
                let (queue, cvar) = &*completions;
                queue
                    .lock()
                    .unwrap()
                    .push(Box::new(move |interp: &mut Interpreter| {
                        interp.scheduler.node_loop.stdin.pending.push_back(msg);
                    }));
                cvar.notify_all();
                if last {
                    break;
                }
            }
        });
    }
    if st.pending.is_empty() {
        st.request();
    }
    Completion::Normal(JsValue::UNDEFINED)
}

fn stdin_stop(interp: &mut Interpreter, _this: &JsValue, _args: &[JsValue]) -> Completion {
    interp.scheduler.node_loop.stdin.reading = false;
    Completion::Normal(JsValue::UNDEFINED)
}

fn stdin_ref(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    let refed = interp.to_boolean_val(arg(args, 0));
    interp.scheduler.node_loop.stdin.unrefed = !refed;
    Completion::Normal(JsValue::UNDEFINED)
}

impl Interpreter {
    pub(super) fn setup_node_host_tty(&mut self) {
        let binding = self.create_object_id();
        type Native = fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion;
        let functions: [(&str, usize, Native); 8] = [
            ("guessHandleType", 1, guess_handle),
            ("isTTY", 1, is_tty),
            ("getWindowSize", 1, get_window_size),
            ("setRawMode", 2, set_raw_mode),
            ("write", 2, write),
            ("stdinStart", 1, stdin_start),
            ("stdinStop", 0, stdin_stop),
            ("stdinRef", 1, stdin_ref),
        ];
        for (name, arity, native) in functions {
            let f = self.create_function(JsFunction::native(String::new(), arity, native));
            self.get_object_cell_expect(binding)
                .borrow_mut()
                .insert_value(name, f);
        }
        self.install_host_global("__host_tty", JsValue::object(binding));
    }

    /// Hand every chunk the reader has queued to the JS callback, each followed
    /// by Node's tick and microtask checkpoint, then ask for the next read.
    pub(crate) fn node_stdin_deliver(&mut self) {
        while self.pending_exit.is_none() {
            let st = &mut self.scheduler.node_loop.stdin;
            if !st.has_deliverable() {
                return;
            }
            let (Some(msg), Some(cb)) = (st.pending.pop_front(), st.cb.clone()) else {
                return;
            };
            st.requested = false;
            if !matches!(msg, StdinMsg::Data(_)) {
                st.eof = true;
                st.reading = false;
            }
            let args = match msg {
                StdinMsg::Data(bytes) => vec![uint8array_from_vec(self, bytes)],
                StdinMsg::Eof => vec![JsValue::NULL],
                StdinMsg::Error(errno) => {
                    vec![
                        JsValue::UNDEFINED,
                        uv_error(self, errno, "read", None, None),
                    ]
                }
            };
            let result = self.node_call(&cb, &args);
            self.node_settle(result);
            self.node_checkpoint();
            let st = &mut self.scheduler.node_loop.stdin;
            if st.reading && st.pending.is_empty() {
                st.request();
            }
        }
    }
}
