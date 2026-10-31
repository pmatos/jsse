//! The thread-pool half of async `fs` for Node host mode (issue #1234): a
//! `Job` is plain owned data (paths, fds, bytes), so a worker can run the same
//! syscalls the synchronous floor makes without touching the interpreter. The
//! result comes back as a `Reply` or an `AsyncError`, and the main thread
//! turns both into JS values (`node_host_fs_async.rs`).

use super::node_host_fs_args::*;
use super::node_host_fs_meta::stat_values;
use super::node_host_fs_more as more;
use super::node_host_fs_ops::{copy_file, mkdir_p, sync_data, uv_dirent_type};
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::DirBuilderExt;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};

type Task = Box<dyn FnOnce() + Send>;

const WORKERS: usize = 4;

/// Run `task` on one of a fixed pool of worker threads, started on first use.
pub(super) fn spawn(task: Task) {
    static POOL: OnceLock<Sender<Task>> = OnceLock::new();
    let sender = POOL.get_or_init(|| {
        let (sender, receiver) = channel::<Task>();
        let receiver = Arc::new(Mutex::new(receiver));
        for index in 0..WORKERS {
            let receiver = receiver.clone();
            let _ = std::thread::Builder::new()
                .name(format!("jsse-fs-{index}"))
                .spawn(move || {
                    loop {
                        let next = receiver.lock().unwrap().recv();
                        match next {
                            Ok(task) => task(),
                            Err(_) => break,
                        }
                    }
                });
        }
        sender
    });
    if let Err(failed) = sender.send(task) {
        (failed.0)();
    }
}

pub(super) struct AsyncError {
    pub(super) errno: i32,
    pub(super) syscall: &'static str,
    pub(super) path: Option<Vec<u8>>,
    pub(super) dest: Option<Vec<u8>>,
}

impl AsyncError {
    pub(super) fn new(errno: i32, syscall: &'static str) -> Self {
        AsyncError {
            errno,
            syscall,
            path: None,
            dest: None,
        }
    }

    pub(super) fn at(errno: i32, syscall: &'static str, path: &[u8]) -> Self {
        AsyncError {
            path: Some(path.to_vec()),
            ..Self::new(errno, syscall)
        }
    }

    pub(super) fn io(e: &std::io::Error, syscall: &'static str, path: &[u8]) -> Self {
        Self::at(e.raw_os_error().unwrap_or(libc::EIO), syscall, path)
    }
}

/// A whole-file operand: a caller-owned descriptor or a path opened here.
pub(super) enum Target {
    Fd(i32),
    Path(Vec<u8>),
}

pub(super) enum Job {
    Open(Vec<u8>, i32, u32),
    Close(i32),
    Read(i32, usize, i64),
    Write(i32, Vec<u8>, i64),
    ReadFile(Target, i32),
    WriteFile(Target, Vec<u8>, i32, u32),
    Stat(Target, i64),
    Access(Vec<u8>, i32),
    Readdir(Vec<u8>, bool),
    Readlink(Vec<u8>),
    Realpath(Vec<u8>),
    Mkdir(Vec<u8>, u32),
    MkdirP(Vec<u8>, u32),
    Rmdir(Vec<u8>),
    Unlink(Vec<u8>),
    Rename(Vec<u8>, Vec<u8>),
    CopyFile(Vec<u8>, Vec<u8>, bool),
    Ftruncate(i32, i64),
    Fsync(i32, bool),
    Chmod(Target, u32),
    Chown(Target, u32, u32, bool),
    Utimes(Target, f64, f64, bool),
    Link(Vec<u8>, Vec<u8>),
    Symlink(Vec<u8>, Vec<u8>),
    Mkdtemp(Vec<u8>),
    Statfs(Vec<u8>),
    RemoveAll(Vec<u8>),
    ReaddirTree(Vec<u8>),
}

pub(super) enum Reply {
    Unit,
    Number(f64),
    Bytes(Vec<u8>),
    Stat(Vec<i128>),
    Path(Option<Vec<u8>>),
    Dirents(Vec<Vec<u8>>, Vec<f64>),
    Numbers(Vec<f64>),
    Tree(more::Tree),
}

pub(super) type Outcome = Result<Reply, AsyncError>;

fn cstring(bytes: &[u8]) -> Result<CString, AsyncError> {
    CString::new(bytes).map_err(|_| AsyncError::new(libc::EINVAL, "open"))
}

fn open_target(target: &Target, flags: i32, mode: u32) -> Result<(i32, bool), AsyncError> {
    match target {
        Target::Fd(fd) => Ok((*fd, false)),
        Target::Path(path) => open_fd(&cstring(path)?, flags, mode)
            .map(|fd| (fd, true))
            .map_err(|errno| AsyncError::at(errno, "open", path)),
    }
}

fn close_after(fd: i32, owned: bool, ok: bool) -> Result<(), AsyncError> {
    match close_fd(fd) {
        Err(errno) if owned && ok => Err(AsyncError::new(errno, "close")),
        _ => Ok(()),
    }
}

fn path_of(target: &Target) -> Option<Vec<u8>> {
    match target {
        Target::Path(path) => Some(path.clone()),
        Target::Fd(_) => None,
    }
}

fn read_file(target: &Target, flags: i32) -> Outcome {
    let (fd, owned) = open_target(target, flags, 0o666)?;
    let result = (|| {
        let md = fd_metadata(fd)
            .map_err(|e| AsyncError::new(e.raw_os_error().unwrap_or(libc::EIO), "fstat"))?;
        let hint = if md.is_file() { md.len() as usize } else { 0 };
        read_to_end(fd, hint).map_err(|errno| AsyncError {
            path: path_of(target),
            ..AsyncError::new(errno, "read")
        })
    })();
    close_after(fd, owned, result.is_ok())?;
    result.map(Reply::Bytes)
}

fn write_file(target: &Target, data: &[u8], flags: i32, mode: u32) -> Outcome {
    let (fd, owned) = open_target(target, flags, mode)?;
    let result = write_all(fd, data).map_err(|errno| AsyncError::new(errno, "write"));
    close_after(fd, owned, result.is_ok())?;
    result.map(|()| Reply::Unit)
}

fn stat(target: &Target, kind: i64) -> Outcome {
    let (md, syscall, path) = match target {
        Target::Fd(fd) => (fd_metadata(*fd), "fstat", None),
        Target::Path(path) => {
            let p = as_path(path);
            if kind == 1 {
                (std::fs::symlink_metadata(p), "lstat", Some(path))
            } else {
                (std::fs::metadata(p), "stat", Some(path))
            }
        }
    };
    match md {
        Ok(md) => Ok(Reply::Stat(stat_values(&md))),
        Err(e) => Err(AsyncError {
            path: path.cloned(),
            ..AsyncError::new(e.raw_os_error().unwrap_or(libc::EIO), syscall)
        }),
    }
}

fn readdir(path: &[u8], opendir: bool) -> Outcome {
    let fail = |e: &std::io::Error| {
        if opendir {
            AsyncError::io(e, "opendir", path)
        } else {
            AsyncError::io(e, "scandir", path)
        }
    };
    let dir = std::fs::read_dir(as_path(path)).map_err(|e| fail(&e))?;
    let mut entries = Vec::new();
    for entry in dir {
        let entry = entry.map_err(|e| fail(&e))?;
        let kind = entry.file_type().map(|t| uv_dirent_type(&t)).unwrap_or(0.0);
        entries.push((entry.file_name().as_bytes().to_vec(), kind));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let (names, kinds) = entries.into_iter().unzip();
    Ok(Reply::Dirents(names, kinds))
}

fn path_reply(
    result: std::io::Result<std::path::PathBuf>,
    syscall: &'static str,
    path: &[u8],
) -> Outcome {
    match result {
        Ok(p) => Ok(Reply::Path(Some(p.as_os_str().as_bytes().to_vec()))),
        Err(e) => Err(AsyncError::io(&e, syscall, path)),
    }
}

fn unit(result: std::io::Result<()>, syscall: &'static str, path: &[u8]) -> Outcome {
    result
        .map(|()| Reply::Unit)
        .map_err(|e| AsyncError::io(&e, syscall, path))
}

fn status(rc: i32, syscall: &'static str) -> Outcome {
    if rc == 0 {
        Ok(Reply::Unit)
    } else {
        Err(AsyncError::new(last_errno(), syscall))
    }
}

/// Run `job`, reporting a panic in a syscall helper as `EIO` so the request
/// still completes and the event loop can exit.
pub(super) fn run(job: Job) -> Outcome {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| execute(job)))
        .unwrap_or_else(|_| Err(AsyncError::new(libc::EIO, "fs")))
}

fn execute(job: Job) -> Outcome {
    match job {
        Job::Open(path, flags, mode) => open_fd(&cstring(&path)?, flags, mode)
            .map(|fd| Reply::Number(f64::from(fd)))
            .map_err(|errno| AsyncError::at(errno, "open", &path)),
        Job::Close(fd) => close_fd(fd)
            .map(|()| Reply::Unit)
            .map_err(|errno| AsyncError::new(errno, "close")),
        Job::Read(fd, len, pos) => {
            let mut buf = Vec::new();
            buf.try_reserve_exact(len)
                .map_err(|_| AsyncError::new(libc::ENOMEM, "read"))?;
            buf.resize(len, 0);
            let n = read_fd(fd, &mut buf, pos).map_err(|errno| AsyncError::new(errno, "read"))?;
            buf.truncate(n);
            Ok(Reply::Bytes(buf))
        }
        Job::Write(fd, data, pos) => write_fd(fd, &data, pos)
            .map(|n| Reply::Number(n as f64))
            .map_err(|errno| AsyncError::new(errno, "write")),
        Job::ReadFile(target, flags) => read_file(&target, flags),
        Job::WriteFile(target, data, flags, mode) => write_file(&target, &data, flags, mode),
        Job::Stat(target, kind) => stat(&target, kind),
        Job::Access(path, mode) => {
            let c = cstring(&path)?;
            if unsafe { libc::access(c.as_ptr(), mode) } == 0 {
                Ok(Reply::Unit)
            } else {
                Err(AsyncError::at(last_errno(), "access", &path))
            }
        }
        Job::Readdir(path, opendir) => readdir(&path, opendir),
        Job::Readlink(path) => path_reply(std::fs::read_link(as_path(&path)), "readlink", &path),
        Job::Realpath(path) => path_reply(std::fs::canonicalize(as_path(&path)), "realpath", &path),
        Job::Mkdir(path, mode) => unit(
            std::fs::DirBuilder::new().mode(mode).create(as_path(&path)),
            "mkdir",
            &path,
        ),
        Job::MkdirP(path, mode) => mkdir_p(&path, mode)
            .map(Reply::Path)
            .map_err(|errno| AsyncError::at(errno, "mkdir", &path)),
        Job::Rmdir(path) => unit(std::fs::remove_dir(as_path(&path)), "rmdir", &path),
        Job::Unlink(path) => unit(std::fs::remove_file(as_path(&path)), "unlink", &path),
        Job::Rename(from, to) => std::fs::rename(as_path(&from), as_path(&to))
            .map(|()| Reply::Unit)
            .map_err(|e| AsyncError {
                dest: Some(to.clone()),
                ..AsyncError::io(&e, "rename", &from)
            }),
        Job::CopyFile(src, dst, excl) => copy_file(&cstring(&src)?, &cstring(&dst)?, excl)
            .map(|()| Reply::Unit)
            .map_err(|errno| AsyncError {
                dest: Some(dst.clone()),
                ..AsyncError::at(errno, "copyfile", &src)
            }),
        Job::Ftruncate(fd, len) => status(
            unsafe { libc::ftruncate(fd, len as libc::off_t) },
            "ftruncate",
        ),
        Job::Fsync(fd, data_only) => {
            if data_only {
                status(sync_data(fd), "fdatasync")
            } else {
                status(unsafe { libc::fsync(fd) }, "fsync")
            }
        }
        Job::Chmod(target, mode) => more::chmod(&target, mode),
        Job::Chown(target, uid, gid, link) => more::chown(&target, uid, gid, link),
        Job::Utimes(target, atime, mtime, link) => more::utimes(&target, atime, mtime, link),
        Job::Link(from, to) => more::link(&from, &to),
        Job::Symlink(target, link) => more::symlink(&target, &link),
        Job::Mkdtemp(prefix) => more::mkdtemp(&prefix),
        Job::Statfs(path) => more::statfs(&path),
        Job::RemoveAll(path) => more::remove_all(&path).map(|()| Reply::Unit),
        Job::ReaddirTree(path) => more::readdir_tree(&path).map(Reply::Tree),
    }
}
