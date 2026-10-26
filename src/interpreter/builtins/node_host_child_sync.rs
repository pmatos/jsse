//! Synchronous half of the `child_process` floor (issue #1236): `spawnSync`.
//!
//! The call spawns the child, then feeds its input pipes and drains its output
//! pipes on their own threads, so a large `input` plus large output cannot
//! deadlock, and waits for exit with the optional `timeout`. `maxBuffer`
//! overruns and timeouts kill the child with `killSignal` and report
//! `ENOBUFS` / `ETIMEDOUT`, as libuv's `spawn_sync` does.

use super::super::*;
use super::node_host_child_spawn::{
    SpawnSpec, decode_spec, list_of, reap, signal_unless_exited, spawn_process,
};
use super::node_host_fs_args::{Res, arg, int, last_errno, uint8array_from_vec, view_of};
use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CHUNK: usize = 64 * 1024;

enum Wake {
    Exit(Option<i32>, Option<i32>),
    Overflow,
    ReaderDone,
    WriteFailed(i32),
}

type Output = Arc<Mutex<Vec<u8>>>;

fn input_bytes(interp: &mut Interpreter, v: &JsValue) -> Res<Option<Vec<u8>>> {
    if v.is_undefined() || v.is_null() {
        return Ok(None);
    }
    let view = view_of(interp, v)
        .ok_or_else(|| interp.create_type_error("stdio input must be a Uint8Array"))?;
    Ok(view.buffer.borrow().with_read(|b| {
        b.get(view.offset..view.offset + view.len)
            .map(<[u8]>::to_vec)
    }))
}

fn feed(fd: Arc<OwnedFd>, input: Option<Vec<u8>>, wake: Sender<Wake>) {
    std::thread::spawn(move || {
        let mut rest = input.as_deref().unwrap_or(&[]);
        while !rest.is_empty() {
            let n = unsafe { libc::write(fd.as_raw_fd(), rest.as_ptr().cast(), rest.len()) };
            if n < 0 {
                let errno = last_errno();
                if errno == libc::EINTR {
                    continue;
                }
                let _ = wake.send(Wake::WriteFailed(errno));
                break;
            }
            rest = &rest[n as usize..];
        }
        unsafe { libc::shutdown(fd.as_raw_fd(), libc::SHUT_WR) };
    });
}

fn drain(fd: Arc<OwnedFd>, limit: Option<usize>, out: Output, wake: Sender<Wake>) {
    std::thread::spawn(move || {
        let mut buf = vec![0u8; CHUNK];
        let mut overflowed = false;
        loop {
            let n = unsafe { libc::read(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
            if n < 0 && last_errno() == libc::EINTR {
                continue;
            }
            if n <= 0 {
                let _ = wake.send(Wake::ReaderDone);
                return;
            }
            // After an overflow the child is being killed; keep reading so it
            // cannot block on a full pipe, but keep nothing.
            if overflowed {
                continue;
            }
            let mut out = out.lock().unwrap();
            out.extend_from_slice(&buf[..n as usize]);
            if limit.is_some_and(|max| out.len() > max) {
                overflowed = true;
                let _ = wake.send(Wake::Overflow);
            }
        }
    });
}

fn run(
    spec: &SpawnSpec,
    inputs: Vec<Option<Vec<u8>>>,
    timeout: Option<Duration>,
    kill_signal: i32,
    max_buffer: Option<usize>,
) -> Result<SyncResult, i32> {
    let spawned = spawn_process(spec)?;
    let pid = spawned.child.id() as i32;
    let (tx, rx) = channel();
    let mut outputs: Vec<Option<Output>> = Vec::new();
    let mut readers = 0;
    let mut inputs = inputs.into_iter();
    for (index, parent) in spawned.parents.into_iter().enumerate() {
        let input = inputs.next().flatten();
        let Some(fd) = parent.map(Arc::new) else {
            outputs.push(None);
            continue;
        };
        if index == 0 || input.is_some() {
            feed(fd.clone(), input, tx.clone());
        }
        if index == 0 {
            outputs.push(None);
        } else {
            let out = Output::default();
            drain(fd, max_buffer, out.clone(), tx.clone());
            outputs.push(Some(out));
            readers += 1;
        }
    }
    let exited = Arc::new(Mutex::new(false));
    let mut child = spawned.child;
    let exit_tx = tx.clone();
    let reaper_flag = exited.clone();
    std::thread::spawn(move || {
        let (code, signal) = reap(&mut child, &reaper_flag);
        let _ = exit_tx.send(Wake::Exit(code, signal));
    });
    drop(tx);

    let mut error = 0;
    let mut killed = false;
    let mut status = None;
    let mut deadline = timeout.map(|t| Instant::now() + t);
    let kill = |killed: &mut bool| {
        if !*killed {
            *killed = true;
            signal_unless_exited(pid, kill_signal, &exited);
        }
    };
    while status.is_none() || readers > 0 {
        let message = match deadline {
            Some(at) => rx.recv_timeout(at.saturating_duration_since(Instant::now())),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match message {
            Ok(Wake::Exit(code, signal)) => status = Some((code, signal)),
            Ok(Wake::ReaderDone) => readers -= 1,
            Ok(Wake::Overflow) => {
                if error == 0 {
                    error = -libc::ENOBUFS;
                }
                kill(&mut killed);
            }
            Ok(Wake::WriteFailed(errno)) => {
                if error == 0 {
                    error = -errno;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if error == 0 {
                    error = -libc::ETIMEDOUT;
                }
                deadline = None;
                kill(&mut killed);
                // A pipe some other process still holds open is not waited for.
                if status.is_some() {
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    let (code, signal) = status.unwrap_or((None, None));
    let output = outputs
        .into_iter()
        .map(|out| out.map(|out| std::mem::take(&mut *out.lock().unwrap())))
        .collect();
    Ok(SyncResult {
        error,
        pid,
        code,
        signal,
        output,
    })
}

struct SyncResult {
    error: i32,
    pid: i32,
    code: Option<i32>,
    signal: Option<i32>,
    output: Vec<Option<Vec<u8>>>,
}

impl Interpreter {
    pub(super) fn setup_node_host_child_sync(&mut self) {
        // __host_cp_spawn_sync(file, argv, cwd, envPairs, stdio, flags, uid, gid,
        //   inputs, timeoutMs, killSignal, maxBuffer)
        //   -> [errno, pid, status, signal, ...output per stdio slot]
        self.host_fs_fn("__host_cp_spawn_sync", 12, |interp, args| {
            let spec = decode_spec(interp, args)?;
            let mut inputs = Vec::new();
            for item in list_of(interp, arg(args, 8), "inputs")? {
                inputs.push(input_bytes(interp, &item)?);
            }
            let timeout = interp.to_number_value(arg(args, 9))?;
            let timeout = (timeout.is_finite() && timeout > 0.0)
                .then(|| Duration::from_millis(timeout as u64));
            let kill_signal = int(interp, arg(args, 10))? as i32;
            let max = interp.to_number_value(arg(args, 11))?;
            let max_buffer = (max.is_finite() && max > 0.0).then_some(max as usize);
            let result = match run(&spec, inputs, timeout, kill_signal, max_buffer) {
                Ok(result) => result,
                Err(errno) => {
                    return Ok(interp.create_array(vec![JsValue::number(-f64::from(errno))]));
                }
            };
            let number = |n: i32| JsValue::number(f64::from(n));
            let option = |n: Option<i32>| n.map_or(JsValue::NULL, number);
            let mut fields = vec![
                number(result.error),
                number(result.pid),
                option(result.code),
                option(result.signal),
            ];
            for bytes in result.output {
                fields.push(match bytes {
                    Some(bytes) => uint8array_from_vec(interp, bytes),
                    None => JsValue::NULL,
                });
            }
            Ok(interp.create_array(fields))
        });
    }
}
