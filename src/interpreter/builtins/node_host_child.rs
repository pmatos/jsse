//! Asynchronous half of the `child_process` floor for Node host mode (issue
//! #1236): `spawn` with its stdio pipes, and `kill`.
//!
//! Worker threads (one per child waiting for exit, one per read pipe, one per
//! written pipe) hold plain Rust data only. Each pushes a `ChildEvent` onto the
//! host-async completion queue; the closure that runs it on the interpreter
//! thread calls the single JS dispatcher installed by `__host_cp_start`, which
//! looks the child up in a JS-owned table. Nothing JS-visible is captured by a
//! thread, so the only GC root is the dispatcher itself (`NodeLoopState`).
//! Whether a child keeps the loop alive is decided by JS through
//! `__host_loop_ref`. See `docs/adr/2026-10-26-2300-node-host-child-process.md`.

use super::super::*;
use super::node_host_child_spawn::{decode_spec, reap, signal_unless_exited, spawn_process};
use super::node_host_fs_args::{Res, arg, int, last_errno, uint8array_from_vec, view_of};
use std::collections::{HashMap, VecDeque};
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};

pub(super) type Completions = Arc<(
    Mutex<Vec<Box<dyn FnOnce(&mut Interpreter) + Send>>>,
    Condvar,
)>;

pub(crate) enum ChildEvent {
    Exit {
        id: u64,
        code: i32,
        signal: i32,
    },
    Data {
        id: u64,
        index: usize,
        bytes: Vec<u8>,
    },
    Eof {
        id: u64,
        index: usize,
    },
    Written {
        id: u64,
        index: usize,
        errno: i32,
    },
    Signal(i32),
}

const CHUNK: usize = 64 * 1024;

/// One-chunk-at-a-time flow control for a read pipe: the reader thread takes a
/// token before each `read`, and JS adds one per `_read`, so an unconsumed
/// stream applies back-pressure to the child instead of buffering without bound.
#[derive(Default)]
struct Flow {
    state: Mutex<(u32, bool)>,
    wake: Condvar,
}

enum WriteMsg {
    Data(Vec<u8>),
    End,
}

struct Endpoint {
    fd: Arc<OwnedFd>,
    flow: Arc<Flow>,
    reading: bool,
    writer: Option<Sender<WriteMsg>>,
    open: bool,
}

struct ChildCtl {
    pid: i32,
    exited: Arc<Mutex<bool>>,
    endpoints: Vec<Option<Endpoint>>,
}

#[derive(Default)]
pub(crate) struct ChildState {
    next_id: u64,
    children: HashMap<u64, ChildCtl>,
    inbox: VecDeque<ChildEvent>,
}

impl ChildState {
    pub(crate) fn has_events(&self) -> bool {
        !self.inbox.is_empty()
    }
}

pub(super) fn post(queue: &Completions, event: ChildEvent) {
    let (lock, wake) = &**queue;
    lock.lock()
        .unwrap()
        .push(Box::new(move |interp: &mut Interpreter| {
            interp.node_host.child.inbox.push_back(event);
        }));
    wake.notify_one();
}

fn spawn_waiter(
    queue: Completions,
    id: u64,
    exited: Arc<Mutex<bool>>,
    mut child: std::process::Child,
) {
    std::thread::spawn(move || {
        let (code, signal) = reap(&mut child, &exited);
        post(
            &queue,
            ChildEvent::Exit {
                id,
                code: code.unwrap_or(0),
                signal: signal.unwrap_or(0),
            },
        );
    });
}

fn spawn_reader(queue: Completions, id: u64, index: usize, fd: Arc<OwnedFd>, flow: Arc<Flow>) {
    std::thread::spawn(move || {
        let mut buf = vec![0u8; CHUNK];
        loop {
            {
                let mut state = flow.state.lock().unwrap();
                while state.0 == 0 && !state.1 {
                    state = flow.wake.wait(state).unwrap();
                }
                if state.1 {
                    return;
                }
                state.0 -= 1;
            }
            let n = unsafe { libc::read(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
            if n < 0 && last_errno() == libc::EINTR {
                flow.state.lock().unwrap().0 += 1;
                continue;
            }
            if n <= 0 {
                if !flow.state.lock().unwrap().1 {
                    post(&queue, ChildEvent::Eof { id, index });
                }
                return;
            }
            let bytes = buf[..n as usize].to_vec();
            post(&queue, ChildEvent::Data { id, index, bytes });
        }
    });
}

fn write_all(fd: RawFd, mut bytes: &[u8]) -> Result<(), i32> {
    while !bytes.is_empty() {
        let n = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        if n < 0 {
            if last_errno() == libc::EINTR {
                continue;
            }
            return Err(last_errno());
        }
        bytes = &bytes[n as usize..];
    }
    Ok(())
}

fn spawn_writer(
    queue: Completions,
    id: u64,
    index: usize,
    fd: Arc<OwnedFd>,
    messages: Receiver<WriteMsg>,
) {
    std::thread::spawn(move || {
        let mut failed = 0;
        for message in messages {
            match message {
                WriteMsg::Data(bytes) => {
                    if failed == 0
                        && let Err(errno) = write_all(fd.as_raw_fd(), &bytes)
                    {
                        failed = errno;
                    }
                    post(
                        &queue,
                        ChildEvent::Written {
                            id,
                            index,
                            errno: failed,
                        },
                    );
                }
                WriteMsg::End => {
                    unsafe { libc::shutdown(fd.as_raw_fd(), libc::SHUT_WR) };
                    return;
                }
            }
        }
    });
}

impl Interpreter {
    /// Deliver the events that were waiting when the poll phase began. They are
    /// stashed by the completion closures (which any microtask drain runs) and
    /// handed to JS only here, so a `nextTick` queued by user code still runs
    /// before the event it was queued ahead of.
    pub(crate) fn node_pump_host_events(&mut self) {
        for _ in 0..self.node_host.child.inbox.len() {
            if self.pending_exit.is_some() {
                return;
            }
            if let Some(event) = self.node_host.child.inbox.pop_front() {
                self.child_deliver(event);
            }
        }
    }

    fn child_deliver(&mut self, event: ChildEvent) {
        let Some(dispatch) = self.scheduler.node_loop.host_dispatch.clone() else {
            return;
        };
        let (kind, id, x, y) = match event {
            ChildEvent::Exit { id, code, signal } => (
                1.0,
                id,
                JsValue::number(f64::from(code)),
                JsValue::number(f64::from(signal)),
            ),
            ChildEvent::Data { id, index, bytes } => (
                2.0,
                id,
                JsValue::number(index as f64),
                uint8array_from_vec(self, bytes),
            ),
            ChildEvent::Eof { id, index } => {
                (3.0, id, JsValue::number(index as f64), JsValue::UNDEFINED)
            }
            ChildEvent::Written { id, index, errno } => (
                4.0,
                id,
                JsValue::number(index as f64),
                JsValue::number(f64::from(errno)),
            ),
            ChildEvent::Signal(signum) => (
                5.0,
                0,
                JsValue::number(f64::from(signum)),
                JsValue::UNDEFINED,
            ),
        };
        let args = [JsValue::number(kind), JsValue::number(id as f64), x, y];
        let result = self.node_call(&dispatch, &args);
        self.node_settle(result);
        self.node_checkpoint();
    }

    fn child_endpoint(&mut self, id: u64, index: usize) -> Option<&mut Endpoint> {
        self.node_host
            .child
            .children
            .get_mut(&id)?
            .endpoints
            .get_mut(index)?
            .as_mut()
    }

    pub(super) fn setup_node_host_child(&mut self) {
        // __host_cp_start(dispatch): the JS function every event is delivered to
        self.host_fs_fn("__host_cp_start", 1, |interp, args| {
            interp.scheduler.node_loop.host_dispatch = Some(arg(args, 0).clone());
            Ok(JsValue::UNDEFINED)
        });

        // __host_cp_spawn(file, argv, cwd, envPairs, stdio, flags, uid, gid)
        //   -> [id, pid] | negative errno
        self.host_fs_fn("__host_cp_spawn", 8, |interp, args| {
            let spec = decode_spec(interp, args)?;
            let spawned = match spawn_process(&spec) {
                Ok(spawned) => spawned,
                Err(errno) => return Ok(JsValue::number(-f64::from(errno))),
            };
            let queue = interp.agent_async_completions.clone();
            let id = interp.node_host.child.next_id + 1;
            interp.node_host.child.next_id = id;
            let pid = spawned.child.id() as i32;
            let exited = Arc::new(Mutex::new(false));
            let endpoints = spawned
                .parents
                .into_iter()
                .map(|parent| {
                    parent.map(|fd| Endpoint {
                        fd: Arc::new(fd),
                        flow: Arc::new(Flow::default()),
                        reading: false,
                        writer: None,
                        open: true,
                    })
                })
                .collect();
            spawn_waiter(queue, id, exited.clone(), spawned.child);
            interp.node_host.child.children.insert(
                id,
                ChildCtl {
                    pid,
                    exited,
                    endpoints,
                },
            );
            Ok(interp.create_array(vec![
                JsValue::number(id as f64),
                JsValue::number(f64::from(pid)),
            ]))
        });

        // __host_cp_read(id, index): allow one more chunk from a pipe
        self.host_fs_fn("__host_cp_read", 2, |interp, args| {
            let id = int(interp, arg(args, 0))? as u64;
            let index = int(interp, arg(args, 1))? as usize;
            let queue = interp.agent_async_completions.clone();
            if let Some(end) = interp.child_endpoint(id, index)
                && end.open
            {
                {
                    let mut state = end.flow.state.lock().unwrap();
                    state.0 = 1;
                    end.flow.wake.notify_one();
                }
                if !end.reading {
                    end.reading = true;
                    spawn_reader(queue, id, index, end.fd.clone(), end.flow.clone());
                }
            }
            Ok(JsValue::UNDEFINED)
        });

        // __host_cp_write(id, index, bytes): queue a write; `Written` follows
        self.host_fs_fn("__host_cp_write", 3, |interp, args| {
            let id = int(interp, arg(args, 0))? as u64;
            let index = int(interp, arg(args, 1))? as usize;
            let view = view_of(interp, arg(args, 2))
                .ok_or_else(|| interp.create_type_error("data must be a Uint8Array"))?;
            let bytes = view
                .buffer
                .borrow()
                .with_read(|b| {
                    b.get(view.offset..view.offset + view.len)
                        .map(<[u8]>::to_vec)
                })
                .unwrap_or_default();
            let queue = interp.agent_async_completions.clone();
            let Some(end) = interp.child_endpoint(id, index) else {
                return Ok(JsValue::FALSE);
            };
            if !end.open {
                return Ok(JsValue::FALSE);
            }
            let sender = end.writer.get_or_insert_with(|| {
                let (tx, rx) = channel();
                spawn_writer(queue, id, index, end.fd.clone(), rx);
                tx
            });
            Ok(JsValue::boolean(sender.send(WriteMsg::Data(bytes)).is_ok()))
        });

        // __host_cp_shutdown(id, index): no more writes; the child sees EOF
        self.host_fs_fn("__host_cp_shutdown", 2, |interp, args| {
            let id = int(interp, arg(args, 0))? as u64;
            let index = int(interp, arg(args, 1))? as usize;
            if let Some(end) = interp.child_endpoint(id, index) {
                match end.writer.take() {
                    Some(sender) => {
                        let _ = sender.send(WriteMsg::End);
                    }
                    None => unsafe {
                        libc::shutdown(end.fd.as_raw_fd(), libc::SHUT_WR);
                    },
                }
            }
            Ok(JsValue::UNDEFINED)
        });

        // __host_cp_close(id, index): release the pipe; frees a finished child
        self.host_fs_fn("__host_cp_close", 2, |interp, args| {
            let id = int(interp, arg(args, 0))? as u64;
            let index = int(interp, arg(args, 1))? as usize;
            if let Some(end) = interp.child_endpoint(id, index) {
                end.open = false;
                end.writer = None;
                end.flow.state.lock().unwrap().1 = true;
                end.flow.wake.notify_one();
                unsafe { libc::shutdown(end.fd.as_raw_fd(), libc::SHUT_RDWR) };
            }
            if let Some(child) = interp.node_host.child.children.get_mut(&id) {
                child.endpoints.get_mut(index).map(Option::take);
            }
            Ok(JsValue::UNDEFINED)
        });

        // __host_cp_release(id): the child exited and JS is done with it
        self.host_fs_fn("__host_cp_release", 1, |interp, args| {
            let id = int(interp, arg(args, 0))? as u64;
            if let Some(child) = interp.node_host.child.children.remove(&id) {
                for end in child.endpoints.into_iter().flatten() {
                    end.flow.state.lock().unwrap().1 = true;
                    end.flow.wake.notify_one();
                    unsafe { libc::shutdown(end.fd.as_raw_fd(), libc::SHUT_RDWR) };
                }
            }
            Ok(JsValue::UNDEFINED)
        });

        // __host_cp_kill(id, signal) -> 0 | negative errno
        self.host_fs_fn("__host_cp_kill", 2, |interp, args| {
            let id = int(interp, arg(args, 0))? as u64;
            let signal = int(interp, arg(args, 1))? as i32;
            let Some(child) = interp.node_host.child.children.get(&id) else {
                return Ok(JsValue::number(-f64::from(libc::ESRCH)));
            };
            Ok(JsValue::number(f64::from(signal_unless_exited(
                child.pid,
                signal,
                &child.exited,
            ))))
        });
    }
}
