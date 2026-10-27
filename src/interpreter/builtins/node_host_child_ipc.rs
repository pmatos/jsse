//! IPC channels for Node host mode (issue #1237): adopting the descriptor a
//! forked child inherited (`NODE_CHANNEL_FD`) as a one-pipe handle of the
//! `child_process` floor, and reading and writing whole messages on a channel
//! end. A message is a line of JSON text (`mode` 0, what Node writes for
//! `serialization: 'json'`) or a 4-byte big-endian length followed by that many
//! bytes of text (`mode` 1, 'advanced'). The reader thread cuts the stream into
//! messages and posts them a chunk at a time, so JS never handles raw bytes.
//! See `docs/adr/2026-10-26-2330-node-host-fork-ipc.md`.

use super::super::*;
use super::node_host_child::{
    CHUNK, ChildCtl, ChildEvent, Completions, Endpoint, Flow, post, write_endpoint,
};
use super::node_host_fs_args::{arg, int, last_errno};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::{Arc, Mutex};

const MAX_FRAME: usize = 256 * 1024 * 1024;

/// Cut the complete messages off the front of `carry`; `scanned` is how much
/// of a line in progress is already known to hold no newline. `None` when a
/// frame header announces more than `MAX_FRAME` bytes.
fn take_messages(carry: &mut Vec<u8>, scanned: &mut usize, frames: bool) -> Option<Vec<String>> {
    let mut items = Vec::new();
    let mut start = 0;
    if frames {
        while carry.len() >= start + 4 {
            let head: [u8; 4] = carry[start..start + 4].try_into().unwrap();
            let len = u32::from_be_bytes(head) as usize;
            if len > MAX_FRAME {
                return None;
            }
            let end = start + 4 + len;
            if carry.len() < end {
                break;
            }
            items.push(String::from_utf8_lossy(&carry[start + 4..end]).into_owned());
            start = end;
        }
    } else {
        let mut from = *scanned;
        while let Some(at) = carry[from..].iter().position(|&b| b == b'\n') {
            let end = from + at;
            if end > start {
                items.push(String::from_utf8_lossy(&carry[start..end]).into_owned());
            }
            start = end + 1;
            from = start;
        }
    }
    carry.drain(..start);
    *scanned = if frames { 0 } else { carry.len() };
    Some(items)
}

fn spawn_message_reader(
    queue: Completions,
    id: u64,
    index: usize,
    fd: Arc<OwnedFd>,
    flow: Arc<Flow>,
    frames: bool,
) {
    std::thread::spawn(move || {
        let mut buf = vec![0u8; CHUNK];
        let mut carry = Vec::new();
        let mut scanned = 0;
        loop {
            let n = unsafe { libc::read(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
            if n < 0 && last_errno() == libc::EINTR {
                continue;
            }
            if n <= 0 {
                if !flow.is_closed() {
                    post(&queue, ChildEvent::Eof { id, index });
                }
                return;
            }
            carry.extend_from_slice(&buf[..n as usize]);
            let Some(items) = take_messages(&mut carry, &mut scanned, frames) else {
                post(&queue, ChildEvent::Eof { id, index });
                return;
            };
            if !items.is_empty() {
                post(&queue, ChildEvent::Messages { id, index, items });
            }
        }
    });
}

impl Interpreter {
    pub(super) fn setup_node_host_child_ipc(&mut self) {
        // __host_cp_adopt(fd) -> id | negative errno: endpoint 0 of a new
        // handle owns `fd`, which stays open until `__host_cp_release`
        self.host_fs_fn("__host_cp_adopt", 1, |interp, args| {
            let fd = int(interp, arg(args, 0))? as i32;
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
            if flags < 0 {
                return Ok(JsValue::number(-f64::from(libc::EBADF)));
            }
            unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) };
            let state = &mut interp.node_host.child;
            let id = state.next_id + 1;
            state.next_id = id;
            let owned = unsafe { OwnedFd::from_raw_fd(fd) };
            state.children.insert(
                id,
                ChildCtl {
                    pid: 0,
                    exited: Arc::new(Mutex::new(true)),
                    endpoints: vec![Some(Endpoint::new(Arc::new(owned)))],
                },
            );
            Ok(JsValue::number(id as f64))
        });

        // __host_cp_ipc_read(id, index, mode): start delivering whole messages
        self.host_fs_fn("__host_cp_ipc_read", 3, |interp, args| {
            let id = int(interp, arg(args, 0))? as u64;
            let index = int(interp, arg(args, 1))? as usize;
            let frames = int(interp, arg(args, 2))? == 1;
            let queue = interp.agent_async_completions.clone();
            if let Some(end) = interp.child_endpoint(id, index)
                && end.open
                && !end.reading
            {
                end.reading = true;
                spawn_message_reader(queue, id, index, end.fd.clone(), end.flow.clone(), frames);
            }
            Ok(JsValue::UNDEFINED)
        });

        // __host_cp_ipc_write(id, index, mode, text) -> 0 | true | false: one
        // message; 0 when the pipe took it whole, true when `Written` follows
        self.host_fs_fn("__host_cp_ipc_write", 4, |interp, args| {
            let id = int(interp, arg(args, 0))? as u64;
            let index = int(interp, arg(args, 1))? as usize;
            let frames = int(interp, arg(args, 2))? == 1;
            let text = interp.to_string_lossy(arg(args, 3))?;
            if frames && text.len() > MAX_FRAME {
                return Err(interp.create_range_error("IPC message is too large"));
            }
            let mut bytes = Vec::with_capacity(text.len() + 4);
            if frames {
                bytes.extend_from_slice(&(text.len() as u32).to_be_bytes());
                bytes.extend_from_slice(text.as_bytes());
            } else {
                bytes.extend_from_slice(text.as_bytes());
                bytes.push(b'\n');
            }
            Ok(write_endpoint(interp, id, index, bytes, true))
        });
    }
}
