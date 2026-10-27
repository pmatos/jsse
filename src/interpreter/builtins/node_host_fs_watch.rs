//! Native file-system watching for Node host mode (issue #1235): the floor
//! under `fs.watch`, on one inotify instance per interpreter.
//!
//! `__host_watch_add(path)` registers a watch and returns its id;
//! `__host_watch_remove(id)` drops it. One reader thread blocks on the inotify
//! descriptor and posts each event to the host-async completion queue as a
//! `ChildEvent::Watch`; the interpreter thread looks the watch descriptor up
//! when it delivers the event, so a watcher closed in between receives
//! nothing. Event bits follow libuv: bit 1 is attribute/modify, bit 2 is any
//! other event; bit 4 marks an event on the watched path itself, whose name
//! is then the path's basename. Watchers share a descriptor per inode, as in libuv. Where
//! inotify is missing (or `JSSE_NODE_WATCH_POLL` is set) `add` throws `ENOSYS`
//! and the JS side polls instead. Whether a watcher keeps the loop alive is
//! decided by JS through `__host_loop_ref`. See
//! `docs/adr/2026-10-27-0300-node-host-fs-watch.md`.

use super::super::*;
use super::node_host_fs_args::{arg, cpath, int, path_bytes, uint8array_from_vec};
use super::node_host_fs_error::uv_error;
use std::collections::HashMap;

struct Descriptor {
    ids: Vec<u64>,
    base: Vec<u8>,
}

#[derive(Default)]
pub(crate) struct WatchState {
    next_id: u64,
    inotify: Option<i32>,
    stop: Option<i32>,
    watches: HashMap<u64, i32>,
    descriptors: HashMap<i32, Descriptor>,
}

impl Drop for WatchState {
    fn drop(&mut self) {
        if let Some(stop) = self.stop {
            sys::stop(stop);
        }
    }
}

#[cfg(target_os = "linux")]
mod sys {
    use super::super::node_host_child::{ChildEvent, Completions, post};
    use super::super::node_host_fs_args::last_errno;
    use std::ffi::CStr;

    const MASK: u32 = libc::IN_ATTRIB
        | libc::IN_CREATE
        | libc::IN_MODIFY
        | libc::IN_DELETE
        | libc::IN_DELETE_SELF
        | libc::IN_MOVE_SELF
        | libc::IN_MOVED_FROM
        | libc::IN_MOVED_TO;
    const CHANGE: u32 = libc::IN_ATTRIB | libc::IN_MODIFY;
    const HEADER: usize = 16;

    pub(super) const AVAILABLE: bool = true;

    /// Returns the inotify descriptor and the write end of the pipe whose
    /// closing ends the reader thread, which then closes both of its ends.
    pub(super) fn start(queue: Completions) -> Result<(i32, i32), i32> {
        let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
        if fd < 0 {
            return Err(last_errno());
        }
        let mut ends = [0i32; 2];
        if unsafe { libc::pipe2(ends.as_mut_ptr(), libc::O_CLOEXEC) } < 0 {
            let errno = last_errno();
            unsafe { libc::close(fd) };
            return Err(errno);
        }
        let [reader, writer] = ends;
        std::thread::spawn(move || {
            read_events(fd, reader, &queue);
            unsafe {
                libc::close(fd);
                libc::close(reader);
            }
        });
        Ok((fd, writer))
    }

    pub(super) fn stop(writer: i32) {
        unsafe { libc::close(writer) };
    }

    pub(super) fn add(fd: i32, path: &CStr) -> Result<i32, i32> {
        let wd = unsafe { libc::inotify_add_watch(fd, path.as_ptr(), MASK) };
        if wd < 0 { Err(last_errno()) } else { Ok(wd) }
    }

    pub(super) fn remove(fd: i32, wd: i32) {
        unsafe { libc::inotify_rm_watch(fd, wd) };
    }

    fn read_events(fd: i32, stop: i32, queue: &Completions) {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let mut fds = [
                libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: stop,
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];
            let ready = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
            if ready < 0 && last_errno() == libc::EINTR {
                continue;
            }
            if ready < 0 || fds[1].revents != 0 {
                return;
            }
            let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n < 0 && last_errno() == libc::EINTR {
                continue;
            }
            if n <= 0 {
                return;
            }
            let mut at = 0;
            let end = n as usize;
            while at + HEADER <= end {
                let word = |i: usize| {
                    let bytes = &buf[at + i..at + i + 4];
                    u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
                };
                let (wd, mask, len) = (word(0) as i32, word(4), word(12) as usize);
                let name_end = (at + HEADER + len).min(end);
                let raw = &buf[at + HEADER..name_end];
                let name = raw.split(|b| *b == 0).next().unwrap_or(&[]).to_vec();
                at = name_end;
                if wd < 0 {
                    continue;
                }
                let mut events = 0;
                if mask & CHANGE != 0 {
                    events |= 1;
                }
                if mask & !CHANGE != 0 {
                    events |= 2;
                }
                post(queue, ChildEvent::Watch { wd, events, name });
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod sys {
    use super::super::node_host_child::Completions;
    use std::ffi::CStr;

    pub(super) const AVAILABLE: bool = false;

    pub(super) fn start(_: Completions) -> Result<(i32, i32), i32> {
        Err(libc::ENOSYS)
    }

    pub(super) fn stop(_: i32) {}

    pub(super) fn add(_: i32, _: &CStr) -> Result<i32, i32> {
        Err(libc::ENOSYS)
    }

    pub(super) fn remove(_: i32, _: i32) {}
}

impl Interpreter {
    /// Hand one inotify event to every watch registered on its descriptor.
    pub(super) fn watch_deliver(&mut self, wd: i32, events: u32, name: Vec<u8>) {
        let Some(dispatch) = self.scheduler.node_loop.host_dispatch.clone() else {
            return;
        };
        let Some(descriptor) = self.node_host.watch.descriptors.get(&wd) else {
            return;
        };
        let ids = descriptor.ids.clone();
        let base = descriptor.base.clone();
        for id in ids {
            if self.pending_exit.is_some() {
                return;
            }
            if self.node_host.watch.watches.get(&id) != Some(&wd) {
                continue;
            }
            let own = name.is_empty();
            let bytes = if own { &base } else { &name };
            let events = if own { events | 4 } else { events };
            let file = uint8array_from_vec(self, bytes.clone());
            let args = [
                JsValue::number(6.0),
                JsValue::number(id as f64),
                JsValue::number(f64::from(events)),
                file,
            ];
            let result = self.node_call(&dispatch, &args);
            self.node_settle(result);
            self.node_checkpoint();
        }
    }

    pub(super) fn setup_node_host_fs_watch(&mut self) {
        // __host_watch_add(path) -> id; throws the uv error of a failed watch
        self.host_fs_fn("__host_watch_add", 1, |interp, args| {
            let path = path_bytes(interp, arg(args, 0))?;
            let c_path = cpath(interp, &path)?;
            let forced = std::env::var_os("JSSE_NODE_WATCH_POLL").is_some();
            let fail = |interp: &mut Interpreter, errno| {
                uv_error(interp, errno, "watch", Some(&path), None)
            };
            if forced || !sys::AVAILABLE {
                return Err(fail(interp, libc::ENOSYS));
            }
            let fd = match interp.node_host.watch.inotify {
                Some(fd) => fd,
                None => {
                    let (fd, stop) = sys::start(interp.agent_async_completions.clone())
                        .map_err(|errno| fail(interp, errno))?;
                    interp.node_host.watch.inotify = Some(fd);
                    interp.node_host.watch.stop = Some(stop);
                    fd
                }
            };
            let wd = sys::add(fd, &c_path).map_err(|errno| fail(interp, errno))?;
            let state = &mut interp.node_host.watch;
            state.next_id += 1;
            let id = state.next_id;
            let base = path.rsplit(|b| *b == b'/').next().unwrap_or(&[]).to_vec();
            let entry = state.descriptors.entry(wd).or_insert(Descriptor {
                ids: Vec::new(),
                base,
            });
            entry.ids.push(id);
            state.watches.insert(id, wd);
            Ok(JsValue::number(id as f64))
        });

        // __host_watch_remove(id)
        self.host_fs_fn("__host_watch_remove", 1, |interp, args| {
            let id = int(interp, arg(args, 0))? as u64;
            let state = &mut interp.node_host.watch;
            let Some(wd) = state.watches.remove(&id) else {
                return Ok(JsValue::UNDEFINED);
            };
            let Some(descriptor) = state.descriptors.get_mut(&wd) else {
                return Ok(JsValue::UNDEFINED);
            };
            descriptor.ids.retain(|other| *other != id);
            if descriptor.ids.is_empty() {
                state.descriptors.remove(&wd);
                if let Some(fd) = state.inotify {
                    sys::remove(fd, wd);
                }
            }
            Ok(JsValue::UNDEFINED)
        });
    }
}
