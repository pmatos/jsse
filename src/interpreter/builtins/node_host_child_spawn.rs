//! Process creation shared by the asynchronous and synchronous halves of the
//! `child_process` floor (issue #1236): decoding the spawn arguments the JS
//! module passes, and building the child with libuv's stdio model. Every
//! `pipe` is a `socketpair` (as `uv_spawn` makes them), so the parent's end is
//! duplex and `shutdown(SHUT_WR)` is how a child sees EOF on stdin.

use super::super::*;
use super::node_host_fs_args::{Res, arg, int, last_errno};
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

pub(super) const STDIO_IGNORE: i64 = -1;
pub(super) const STDIO_PIPE: i64 = -2;

pub(super) struct SpawnSpec {
    file: String,
    argv: Vec<String>,
    cwd: Option<String>,
    env: Option<Vec<(String, String)>>,
    pub(super) stdio: Vec<i64>,
    detached: bool,
    uid: Option<u32>,
    gid: Option<u32>,
}

pub(super) struct Spawned {
    pub(super) child: Child,
    pub(super) parents: Vec<Option<OwnedFd>>,
}

pub(super) fn list_of(interp: &mut Interpreter, v: &JsValue, what: &str) -> Res<Vec<JsValue>> {
    let items = v
        .as_object_id()
        .and_then(|id| interp.get_object_cell(id))
        .and_then(|cell| cell.borrow().array_elements().map(|e| e.to_vec()));
    items.ok_or_else(|| interp.create_type_error(&format!("{what} must be an array")))
}

fn strings_of(interp: &mut Interpreter, v: &JsValue, what: &str) -> Res<Vec<String>> {
    list_of(interp, v, what)?
        .iter()
        .map(|item| interp.to_string_lossy(item))
        .collect()
}

fn optional_id(interp: &mut Interpreter, v: &JsValue) -> Res<Option<u32>> {
    if v.is_undefined() || v.is_null() {
        return Ok(None);
    }
    Ok(Some(int(interp, v)? as u32))
}

/// `(file, argv, cwd, envPairs, stdio, flags, uid, gid)`, the arguments of
/// `ProcessWrap::Spawn`; `flags` bit 0 is `detached`.
pub(super) fn decode_spec(interp: &mut Interpreter, args: &[JsValue]) -> Res<SpawnSpec> {
    let file = interp.to_string_lossy(arg(args, 0))?;
    let argv = strings_of(interp, arg(args, 1), "args")?;
    let cwd = match arg(args, 2) {
        v if v.is_undefined() || v.is_null() => None,
        v => Some(interp.to_string_lossy(v)?),
    };
    let env = match arg(args, 3) {
        v if v.is_undefined() || v.is_null() => None,
        v => {
            let pairs = strings_of(interp, v, "envPairs")?;
            Some(
                pairs
                    .into_iter()
                    .filter_map(|pair| {
                        let at = pair.get(1..)?.find('=')? + 1;
                        Some((pair[..at].to_string(), pair[at + 1..].to_string()))
                    })
                    .collect(),
            )
        }
    };
    let mut stdio = Vec::new();
    for item in list_of(interp, arg(args, 4), "stdio")? {
        stdio.push(int(interp, &item)?);
    }
    let flags = int(interp, arg(args, 5))?;
    Ok(SpawnSpec {
        file,
        argv,
        cwd,
        env,
        stdio,
        detached: flags & 1 != 0,
        uid: optional_id(interp, arg(args, 6))?,
        gid: optional_id(interp, arg(args, 7))?,
    })
}

fn set_cloexec(fd: RawFd) {
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFD);
        libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
    }
}

fn socketpair() -> Result<(OwnedFd, OwnedFd), i32> {
    let mut fds = [0 as RawFd; 2];
    let rc = unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) };
    if rc != 0 {
        return Err(last_errno());
    }
    set_cloexec(fds[0]);
    set_cloexec(fds[1]);
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

/// A copy of `fd` numbered at least `min`, close-on-exec.
fn dup_above(fd: RawFd, min: RawFd) -> Result<OwnedFd, i32> {
    let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, min) };
    if copy < 0 {
        return Err(last_errno());
    }
    Ok(unsafe { OwnedFd::from_raw_fd(copy) })
}

/// The descriptor a child is given at stdio slot `index`, or `None` for `ignore`.
fn child_end(
    spec: i64,
    index: usize,
    parents: &mut Vec<Option<OwnedFd>>,
) -> Result<Option<OwnedFd>, i32> {
    match spec {
        STDIO_IGNORE => {
            parents.push(None);
            Ok(None)
        }
        STDIO_PIPE => {
            let (parent, child) = socketpair()?;
            parents.push(Some(parent));
            Ok(Some(child))
        }
        fd => {
            parents.push(None);
            Ok(Some(dup_above(fd as RawFd, (index as RawFd).max(3))?))
        }
    }
}

pub(super) fn spawn_process(spec: &SpawnSpec) -> Result<Spawned, i32> {
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();

    let mut cmd = Command::new(&spec.file);
    if let Some((first, rest)) = spec.argv.split_first() {
        cmd.arg0(first).args(rest);
    }
    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }
    if let Some(env) = &spec.env {
        cmd.env_clear();
        for (key, value) in env {
            cmd.env(key, value);
        }
    }
    if let Some(uid) = spec.uid {
        cmd.uid(uid);
    }
    if let Some(gid) = spec.gid {
        cmd.gid(gid);
    }

    let mut parents = Vec::new();
    let mut keep: Vec<OwnedFd> = Vec::new();
    let mut remap: Vec<(RawFd, RawFd)> = Vec::new();
    for (index, &slot) in spec.stdio.iter().enumerate() {
        if index < 3 && slot == index as i64 {
            parents.push(None);
            match index {
                0 => cmd.stdin(Stdio::inherit()),
                1 => cmd.stdout(Stdio::inherit()),
                _ => cmd.stderr(Stdio::inherit()),
            };
            continue;
        }
        let end = child_end(slot, index, &mut parents)?;
        if index < 3 {
            let stdio = end.map_or_else(Stdio::null, Stdio::from);
            match index {
                0 => cmd.stdin(stdio),
                1 => cmd.stdout(stdio),
                _ => cmd.stderr(stdio),
            };
        } else if let Some(end) = end {
            use std::os::fd::AsRawFd;
            let above = dup_above(end.as_raw_fd(), spec.stdio.len() as RawFd)?;
            remap.push((above.as_raw_fd(), index as RawFd));
            keep.push(above);
            keep.push(end);
        }
    }
    let detached = spec.detached;
    unsafe {
        cmd.pre_exec(move || {
            if detached && libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            for &(from, to) in &remap {
                if libc::dup2(from, to) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let child = cmd
        .spawn()
        .map_err(|e| e.raw_os_error().unwrap_or(libc::EIO))?;
    drop(keep);
    Ok(Spawned { child, parents })
}

pub(super) fn exit_parts(status: std::process::ExitStatus) -> (Option<i32>, Option<i32>) {
    use std::os::unix::process::ExitStatusExt;
    (status.code(), status.signal())
}
