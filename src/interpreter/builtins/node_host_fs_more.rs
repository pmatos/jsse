//! Pool jobs and floor primitives added by issue #1341: the metadata calls
//! (`chmod`, `chown`, `utimes`, `link`, `symlink`, `mkdtemp`, `statfs`) now run
//! on the worker pool like the other syscalls, and the two tree walks that used
//! to be JavaScript loops over single syscalls (`rm` recursive and recursive
//! `readdir`) are one job each, so a large tree no longer blocks the event
//! loop. Every function reports errors with the syscall and path the JS walk
//! it replaces reported, so `rmSync`/`readdirSync` keep their error shapes.

use super::super::*;
use super::node_host_fs_args::*;
use super::node_host_fs_error::uv_error;
use super::node_host_fs_jobs::{AsyncError, Outcome, Reply, Target};
use super::node_host_fs_ops::{timespec_of, uv_dirent_type};
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;

fn cstr(bytes: &[u8], syscall: &'static str) -> Result<CString, AsyncError> {
    CString::new(bytes).map_err(|_| AsyncError::at(libc::EINVAL, syscall, bytes))
}

fn failed(rc: i32, syscall: &'static str, path: Option<&[u8]>) -> Outcome {
    if rc == 0 {
        return Ok(Reply::Unit);
    }
    let errno = last_errno();
    Err(match path {
        Some(p) => AsyncError::at(errno, syscall, p),
        None => AsyncError::new(errno, syscall),
    })
}

pub(super) fn chmod(target: &Target, mode: u32) -> Outcome {
    match target {
        Target::Fd(fd) => failed(
            unsafe { libc::fchmod(*fd, mode as libc::mode_t) },
            "fchmod",
            None,
        ),
        Target::Path(p) => {
            let c = cstr(p, "chmod")?;
            failed(
                unsafe { libc::chmod(c.as_ptr(), mode as libc::mode_t) },
                "chmod",
                Some(p),
            )
        }
    }
}

pub(super) fn chown(target: &Target, uid: u32, gid: u32, link: bool) -> Outcome {
    match target {
        Target::Fd(fd) => failed(unsafe { libc::fchown(*fd, uid, gid) }, "fchown", None),
        Target::Path(p) => {
            let c = cstr(p, "chown")?;
            if link {
                failed(
                    unsafe { libc::lchown(c.as_ptr(), uid, gid) },
                    "lchown",
                    Some(p),
                )
            } else {
                failed(
                    unsafe { libc::chown(c.as_ptr(), uid, gid) },
                    "chown",
                    Some(p),
                )
            }
        }
    }
}

pub(super) fn utimes(target: &Target, atime: f64, mtime: f64, link: bool) -> Outcome {
    let times = [timespec_of(atime), timespec_of(mtime)];
    match target {
        Target::Fd(fd) => failed(
            unsafe { libc::futimens(*fd, times.as_ptr()) },
            "futime",
            None,
        ),
        Target::Path(p) => {
            let c = cstr(p, "utime")?;
            let (flag, name) = if link {
                (libc::AT_SYMLINK_NOFOLLOW, "lutime")
            } else {
                (0, "utime")
            };
            failed(
                unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), times.as_ptr(), flag) },
                name,
                Some(p),
            )
        }
    }
}

pub(super) fn link(from: &[u8], to: &[u8]) -> Outcome {
    std::fs::hard_link(as_path(from), as_path(to))
        .map(|()| Reply::Unit)
        .map_err(|e| AsyncError {
            dest: Some(to.to_vec()),
            ..AsyncError::io(&e, "link", from)
        })
}

pub(super) fn symlink(target: &[u8], link: &[u8]) -> Outcome {
    std::os::unix::fs::symlink(as_path(target), as_path(link))
        .map(|()| Reply::Unit)
        .map_err(|e| AsyncError {
            dest: Some(link.to_vec()),
            ..AsyncError::io(&e, "symlink", target)
        })
}

pub(super) fn mkdtemp(prefix: &[u8]) -> Outcome {
    let mut template = prefix.to_vec();
    template.extend_from_slice(b"XXXXXX");
    let mut raw = cstr(&template, "mkdtemp")?.into_bytes_with_nul();
    if unsafe { libc::mkdtemp(raw.as_mut_ptr().cast()) }.is_null() {
        return Err(AsyncError::at(last_errno(), "mkdtemp", &template));
    }
    raw.pop();
    Ok(Reply::Path(Some(raw)))
}

pub(super) fn statfs(path: &[u8]) -> Outcome {
    let c = cstr(path, "statfs")?;
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
        return Err(AsyncError::at(last_errno(), "statfs", path));
    }
    #[cfg(target_os = "linux")]
    let frsize = st.f_frsize as f64;
    #[cfg(not(target_os = "linux"))]
    let frsize = st.f_bsize as f64;
    Ok(Reply::Numbers(vec![
        st.f_type as f64,
        st.f_bsize as f64,
        frsize,
        st.f_blocks as f64,
        st.f_bfree as f64,
        st.f_bavail as f64,
        st.f_files as f64,
        st.f_ffree as f64,
    ]))
}

fn join(dir: &[u8], name: &[u8]) -> Vec<u8> {
    let mut path = dir.to_vec();
    path.push(b'/');
    path.extend_from_slice(name);
    path
}

/// `std::filesystem::remove_all`, depth first; the first failure aborts. The
/// errors name the syscall and entry the JavaScript `removeAll` they replace
/// reported (`lstat`, `scandir`, `rmdir`, `unlink`).
pub(super) fn remove_all(path: &[u8]) -> Result<(), AsyncError> {
    let md =
        std::fs::symlink_metadata(as_path(path)).map_err(|e| AsyncError::io(&e, "lstat", path))?;
    if md.is_dir() {
        let dir =
            std::fs::read_dir(as_path(path)).map_err(|e| AsyncError::io(&e, "scandir", path))?;
        let mut names = Vec::new();
        for entry in dir {
            let entry = entry.map_err(|e| AsyncError::io(&e, "scandir", path))?;
            names.push(entry.file_name().as_bytes().to_vec());
        }
        names.sort();
        for name in names {
            remove_all(&join(path, &name))?;
        }
        std::fs::remove_dir(as_path(path)).map_err(|e| AsyncError::io(&e, "rmdir", path))
    } else {
        std::fs::remove_file(as_path(path)).map_err(|e| AsyncError::io(&e, "unlink", path))
    }
}

pub(super) struct Tree {
    pub(super) names: Vec<Vec<u8>>,
    pub(super) kinds: Vec<f64>,
    pub(super) parents: Vec<f64>,
    pub(super) is_dir: Vec<bool>,
}

fn normalize(path: &[u8]) -> Vec<u8> {
    let absolute = path.first() == Some(&b'/');
    let mut out: Vec<&[u8]> = Vec::new();
    for seg in path.split(|&b| b == b'/') {
        if seg.is_empty() || seg == b"." {
            continue;
        }
        if seg == b".." && out.last().is_some_and(|last| *last != b"..") {
            out.pop();
        } else if seg != b".." || !absolute {
            out.push(seg);
        }
    }
    let joined = out.join(&b'/');
    match (absolute, joined.is_empty()) {
        (true, _) => [b"/".as_slice(), &joined].concat(),
        (false, true) => b".".to_vec(),
        (false, false) => joined,
    }
}

/// The breadth-first walk of `readdir(path, { recursive: true })`: every entry
/// of every directory in the order the JavaScript loop visited them, each with
/// its parent's position in the directory queue (the base is 0). A symlink or
/// entry of unknown type is a directory to descend into iff `stat` says so.
pub(super) fn readdir_tree(base: &[u8]) -> Result<Tree, AsyncError> {
    let mut tree = Tree {
        names: Vec::new(),
        kinds: Vec::new(),
        parents: Vec::new(),
        is_dir: Vec::new(),
    };
    let mut dirs = vec![base.to_vec()];
    let mut i = 0;
    while i < dirs.len() {
        let dir = dirs[i].clone();
        let fail = |e: &std::io::Error| AsyncError::io(e, "scandir", &dir);
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(as_path(&dir)).map_err(|e| fail(&e))? {
            let entry = entry.map_err(|e| fail(&e))?;
            let kind = entry.file_type().map(|t| uv_dirent_type(&t)).unwrap_or(0.0);
            entries.push((entry.file_name().as_bytes().to_vec(), kind));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, kind) in entries {
            let child = normalize(&join(&dir, &name));
            let descend = kind == 2.0
                || ((kind == 3.0 || kind == 0.0)
                    && std::fs::metadata(as_path(&child)).is_ok_and(|md| md.is_dir()));
            if descend {
                dirs.push(child);
            }
            tree.names.push(name);
            tree.kinds.push(kind);
            tree.parents.push(i as f64);
            tree.is_dir.push(descend);
        }
        i += 1;
    }
    Ok(tree)
}

fn throw(interp: &mut Interpreter, e: &AsyncError) -> JsValue {
    uv_error(
        interp,
        e.errno,
        e.syscall,
        e.path.as_deref(),
        e.dest.as_deref(),
    )
}

pub(super) fn tree_value(interp: &mut Interpreter, tree: Tree, raw: bool) -> JsValue {
    let names = tree
        .names
        .iter()
        .map(|n| name_value(interp, n, raw))
        .collect();
    let names = interp.create_array(names);
    let kinds = interp.create_array(tree.kinds.into_iter().map(JsValue::number).collect());
    let parents = interp.create_array(tree.parents.into_iter().map(JsValue::number).collect());
    let flags = tree.is_dir.into_iter().map(JsValue::boolean).collect();
    let flags = interp.create_array(flags);
    interp.create_array(vec![names, kinds, parents, flags])
}

impl Interpreter {
    pub(super) fn setup_node_host_fs_more(&mut self) {
        // path -> undefined; the first failing syscall throws
        self.host_fs_fn("__host_fs_rm_all", 1, |interp, args| {
            let path = path_bytes(interp, arg(args, 0))?;
            remove_all(&path).map_err(|e| throw(interp, &e))?;
            Ok(JsValue::UNDEFINED)
        });

        // (path, asBuffers) -> [names, direntTypes, parentIndexes, isDirectory]
        self.host_fs_fn("__host_fs_readdir_tree", 2, |interp, args| {
            let path = path_bytes(interp, arg(args, 0))?;
            let raw = arg(args, 1).as_boolean() == Some(true);
            let tree = readdir_tree(&path).map_err(|e| throw(interp, &e))?;
            Ok(tree_value(interp, tree, raw))
        });
    }
}
