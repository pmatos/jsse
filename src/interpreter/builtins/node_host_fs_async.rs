//! Async `fs` floor for Node host mode (issue #1234): `__host_fs_async(op,
//! callback, ...args)` decodes the arguments into an owned `Job`, runs it on
//! the worker pool (`node_host_fs_jobs.rs`) and calls `callback(err, value)`
//! from the poll phase of the Node event loop. The request is registered with
//! the loop for its whole life, which roots the callback and a destination
//! buffer and keeps the loop alive. See `docs/adr/2026-10-26-2300-node-host-fs-async.md`.

use super::super::*;
use super::node_host_fs_args::*;
use super::node_host_fs_error::uv_error;
use super::node_host_fs_jobs::{AsyncError, Job, Reply, Target, run, spawn};
use super::node_host_fs_meta::stat_items;

/// How the main thread turns a worker's `Reply` into the callback's value.
enum Post {
    Plain,
    ReadInto(usize, usize),
    Text,
    Stat(bool),
    Names(bool),
    Dirents(bool),
    Path(bool),
}

fn flag(args: &[JsValue], i: usize) -> bool {
    arg(args, i).as_boolean() == Some(true)
}

fn target_of(interp: &mut Interpreter, v: &JsValue) -> Res<Target> {
    match v.as_number() {
        Some(n) => Ok(Target::Fd(n as i32)),
        None => Ok(Target::Path(path_bytes(interp, v)?)),
    }
}

fn window(interp: &mut Interpreter, args: &[JsValue]) -> Res<(View, usize, usize)> {
    let view = view_of(interp, arg(args, 1)).ok_or_else(|| bad_arg(interp, "buffer"))?;
    let off = int(interp, arg(args, 2))?.max(0) as usize;
    let len = int(interp, arg(args, 3))?.max(0) as usize;
    if off.saturating_add(len) > view.len {
        return Err(interp.create_range_error("buffer window exceeds the buffer"));
    }
    Ok((view, off, len))
}

fn fd_arg(interp: &mut Interpreter, args: &[JsValue], i: usize) -> Res<i32> {
    Ok(int(interp, arg(args, i))? as i32)
}

/// `(job, post, buffer to keep alive)` for `op`; `a` are the call's arguments
/// after the callback.
fn decode(interp: &mut Interpreter, op: &str, a: &[JsValue]) -> Res<(Job, Post, Option<JsValue>)> {
    let path = |interp: &mut Interpreter, i: usize| path_bytes(interp, arg(a, i));
    let raw = flag(a, 1);
    Ok(match op {
        "open" => {
            let p = path(interp, 0)?;
            let flags = int(interp, arg(a, 1))? as i32;
            let mode = int(interp, arg(a, 2))? as u32;
            (Job::Open(p, flags, mode), Post::Plain, None)
        }
        "close" => (Job::Close(fd_arg(interp, a, 0)?), Post::Plain, None),
        "read" => {
            let fd = fd_arg(interp, a, 0)?;
            let (_, off, len) = window(interp, a)?;
            let pos = int(interp, arg(a, 4))?;
            (
                Job::Read(fd, len, pos),
                Post::ReadInto(off, len),
                Some(arg(a, 1).clone()),
            )
        }
        "write" => {
            let fd = fd_arg(interp, a, 0)?;
            let (view, off, len) = window(interp, a)?;
            let pos = int(interp, arg(a, 4))?;
            let start = view.offset + off;
            let data = view
                .buffer
                .borrow()
                .with_read(|b| b.get(start..start + len).map(<[u8]>::to_vec))
                .ok_or_else(|| bad_arg(interp, "buffer"))?;
            (Job::Write(fd, data, pos), Post::Plain, None)
        }
        "read_file" => {
            let target = target_of(interp, arg(a, 0))?;
            let flags = int(interp, arg(a, 1))? as i32;
            let post = if flag(a, 2) { Post::Text } else { Post::Plain };
            (Job::ReadFile(target, flags), post, None)
        }
        "write_file" => {
            let target = target_of(interp, arg(a, 0))?;
            let data = if arg(a, 1).is_string() {
                interp.to_string_lossy(arg(a, 1))?.into_bytes()
            } else {
                path_bytes(interp, arg(a, 1))?
            };
            let flags = int(interp, arg(a, 2))? as i32;
            let mode = int(interp, arg(a, 3))? as u32;
            (Job::WriteFile(target, data, flags, mode), Post::Plain, None)
        }
        "stat" => {
            let target = target_of(interp, arg(a, 0))?;
            let kind = int(interp, arg(a, 1))?;
            (Job::Stat(target, kind), Post::Stat(flag(a, 2)), None)
        }
        "access" => {
            let p = path(interp, 0)?;
            let mode = int(interp, arg(a, 1))? as i32;
            (Job::Access(p, mode), Post::Plain, None)
        }
        "readdir" => (
            Job::Readdir(path(interp, 0)?, false),
            Post::Names(raw),
            None,
        ),
        "readdir_types" => (
            Job::Readdir(path(interp, 0)?, flag(a, 2)),
            Post::Dirents(raw),
            None,
        ),
        "readlink" => (Job::Readlink(path(interp, 0)?), Post::Path(raw), None),
        "realpath" => (Job::Realpath(path(interp, 0)?), Post::Path(raw), None),
        "mkdir" => {
            let mode = int(interp, arg(a, 1))? as u32;
            (Job::Mkdir(path(interp, 0)?, mode), Post::Plain, None)
        }
        "mkdirp" => {
            let mode = int(interp, arg(a, 1))? as u32;
            (Job::MkdirP(path(interp, 0)?, mode), Post::Path(false), None)
        }
        "rmdir" => (Job::Rmdir(path(interp, 0)?), Post::Plain, None),
        "unlink" => (Job::Unlink(path(interp, 0)?), Post::Plain, None),
        "rename" => (
            Job::Rename(path(interp, 0)?, path(interp, 1)?),
            Post::Plain,
            None,
        ),
        "copyfile" => {
            let flags = int(interp, arg(a, 2))?;
            (
                Job::CopyFile(path(interp, 0)?, path(interp, 1)?, flags & 1 != 0),
                Post::Plain,
                None,
            )
        }
        "ftruncate" => {
            let fd = fd_arg(interp, a, 0)?;
            let len = int(interp, arg(a, 1))?;
            (Job::Ftruncate(fd, len), Post::Plain, None)
        }
        "fsync" => (
            Job::Fsync(fd_arg(interp, a, 0)?, flag(a, 1)),
            Post::Plain,
            None,
        ),
        _ => return Err(interp.create_type_error(&format!("unknown async fs op {op}"))),
    })
}

fn value_of(interp: &mut Interpreter, post: Post, reply: Reply, keep: Option<&JsValue>) -> JsValue {
    match (post, reply) {
        (Post::ReadInto(off, len), Reply::Bytes(bytes)) => {
            let n = bytes.len().min(len);
            let mut copied = 0;
            if let Some(view) = keep.and_then(|dest| view_of(interp, dest)) {
                let start = view.offset + off;
                view.buffer.borrow_mut().with_write(|b| {
                    if let Some(window) = b.get_mut(start..start + n) {
                        window.copy_from_slice(&bytes[..n]);
                        copied = n;
                    }
                });
            }
            JsValue::number(copied as f64)
        }
        (Post::Text, Reply::Bytes(bytes)) => js_str(&String::from_utf8_lossy(&bytes)),
        (_, Reply::Bytes(bytes)) => uint8array_from_vec(interp, bytes),
        (_, Reply::Number(n)) => JsValue::number(n),
        (Post::Stat(big), Reply::Stat(values)) => {
            let items = stat_items(&values, big);
            interp.create_array(items)
        }
        (Post::Path(raw), Reply::Path(Some(bytes))) => name_value(interp, &bytes, raw),
        (Post::Dirents(raw), Reply::Dirents(names, kinds)) => {
            let names = names.iter().map(|n| name_value(interp, n, raw)).collect();
            let names = interp.create_array(names);
            let kinds = interp.create_array(kinds.into_iter().map(JsValue::number).collect());
            interp.create_array(vec![names, kinds])
        }
        (Post::Names(raw), Reply::Dirents(names, _)) => {
            let names = names.iter().map(|n| name_value(interp, n, raw)).collect();
            interp.create_array(names)
        }
        _ => JsValue::UNDEFINED,
    }
}

fn error_of(interp: &mut Interpreter, e: &AsyncError) -> JsValue {
    uv_error(
        interp,
        e.errno,
        e.syscall,
        e.path.as_deref(),
        e.dest.as_deref(),
    )
}

fn finish(interp: &mut Interpreter, id: u64, post: Post, outcome: Result<Reply, AsyncError>) {
    let roots = interp.scheduler.node_loop.end_request(id);
    let Some(callback) = roots.first() else {
        return;
    };
    let args = match outcome {
        Err(e) => vec![error_of(interp, &e)],
        Ok(reply) => {
            let value = value_of(interp, post, reply, roots.get(1));
            if value.is_undefined() {
                vec![JsValue::NULL]
            } else {
                vec![JsValue::NULL, value]
            }
        }
    };
    interp.node_dispatch(callback, &args);
}

impl Interpreter {
    pub(super) fn setup_node_host_fs_async(&mut self) {
        // (op, callback, ...args): callback(err) | callback(null, value)
        self.host_fs_fn("__host_fs_async", 2, |interp, args| {
            let op = interp.to_string_lossy(arg(args, 0))?;
            let callback = arg(args, 1).clone();
            let (job, post, keep) = decode(interp, &op, args.get(2..).unwrap_or(&[]))?;
            let mut roots = vec![callback];
            roots.extend(keep);
            let id = interp.scheduler.node_loop.begin_request(roots);
            let completions = interp.agent_async_completions.clone();
            spawn(Box::new(move || {
                let outcome = run(job);
                let (queue, wake) = &*completions;
                queue
                    .lock()
                    .unwrap()
                    .push(Box::new(move |interp: &mut Interpreter| {
                        interp
                            .scheduler
                            .node_loop
                            .push_completed(Box::new(move |interp| {
                                finish(interp, id, post, outcome)
                            }));
                    }));
                wake.notify_all();
            }));
            Ok(JsValue::UNDEFINED)
        });
    }
}
