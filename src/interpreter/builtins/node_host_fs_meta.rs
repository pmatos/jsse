//! Metadata and namespace half of the synchronous file system floor (issue
//! #1217): `stat`, `access`, directory listing, links, `mkdir`/`rmdir`/
//! `unlink`/`rename` and the platform constants. Errors use Node's
//! uvException shape (`node_host_fs_error.rs`).

use super::super::*;
use super::node_host_fs_args::*;
use super::node_host_fs_error::{io_error, uv_error};
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};

fn stat_values(md: &std::fs::Metadata) -> Vec<f64> {
    let birth = md
        .created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| (d.as_secs() as i64, i64::from(d.subsec_nanos())))
        .unwrap_or((md.ctime(), md.ctime_nsec()));
    [
        md.dev() as i64,
        i64::from(md.mode()),
        md.nlink() as i64,
        i64::from(md.uid()),
        i64::from(md.gid()),
        md.rdev() as i64,
        md.blksize() as i64,
        md.ino() as i64,
        md.size() as i64,
        md.blocks() as i64,
        md.atime(),
        md.atime_nsec(),
        md.mtime(),
        md.mtime_nsec(),
        md.ctime(),
        md.ctime_nsec(),
        birth.0,
        birth.1,
    ]
    .iter()
    .map(|&n| n as f64)
    .collect()
}

impl Interpreter {
    pub(super) fn setup_node_host_fs_meta(&mut self) {
        // (pathOrFd, kind) with kind 0 stat, 1 lstat, 2 fstat -> number[18]
        self.host_fs_fn("__host_fs_stat", 2, |interp, args| {
            let kind = int(interp, arg(args, 1))?;
            let (md, syscall, path) = if kind == 2 {
                let fd = int(interp, arg(args, 0))? as i32;
                (fd_metadata(fd), "fstat", None)
            } else {
                let bytes = path_bytes(interp, arg(args, 0))?;
                let p = as_path(&bytes);
                let md = if kind == 1 {
                    std::fs::symlink_metadata(p)
                } else {
                    std::fs::metadata(p)
                };
                (md, if kind == 1 { "lstat" } else { "stat" }, Some(bytes))
            };
            match md {
                Ok(md) => {
                    let items = stat_values(&md).into_iter().map(JsValue::number).collect();
                    Ok(interp.create_array(items))
                }
                Err(e) => Err(io_error(interp, &e, syscall, path.as_deref(), None)),
            }
        });

        // Never throws and allocates no error: the hot path of `existsSync`.
        self.host_fs_fn("__host_fs_exists", 1, |interp, args| {
            let Ok(bytes) = path_bytes(interp, arg(args, 0)) else {
                return Ok(JsValue::boolean(false));
            };
            let Ok(c) = CString::new(bytes) else {
                return Ok(JsValue::boolean(false));
            };
            Ok(JsValue::boolean(
                unsafe { libc::access(c.as_ptr(), libc::F_OK) } == 0,
            ))
        });

        self.host_fs_fn("__host_fs_access", 2, |interp, args| {
            let bytes = path_bytes(interp, arg(args, 0))?;
            let c = cpath(interp, &bytes)?;
            let mode = int(interp, arg(args, 1))? as i32;
            if unsafe { libc::access(c.as_ptr(), mode) } != 0 {
                return Err(uv_error(interp, last_errno(), "access", Some(&bytes), None));
            }
            Ok(JsValue::UNDEFINED)
        });

        // Names sorted bytewise, like libuv's scandir.
        // (path, asBuffers) -> string[] | Uint8Array[]
        self.host_fs_fn("__host_fs_readdir", 2, |interp, args| {
            let bytes = path_bytes(interp, arg(args, 0))?;
            let as_buffer = arg(args, 1).as_boolean() == Some(true);
            let fail = |interp: &mut Interpreter, e: &std::io::Error| {
                io_error(interp, e, "scandir", Some(&bytes), None)
            };
            let dir = std::fs::read_dir(as_path(&bytes)).map_err(|e| fail(interp, &e))?;
            let mut names = Vec::new();
            for entry in dir {
                names.push(entry.map_err(|e| fail(interp, &e))?.file_name());
            }
            names.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            let items = names
                .iter()
                .map(|n| name_value(interp, n.as_bytes(), as_buffer))
                .collect();
            Ok(interp.create_array(items))
        });

        self.host_fs_fn("__host_fs_readlink", 1, |interp, args| {
            let bytes = path_bytes(interp, arg(args, 0))?;
            match std::fs::read_link(as_path(&bytes)) {
                Ok(target) => Ok(js_str(&target.to_string_lossy())),
                Err(e) => Err(io_error(interp, &e, "readlink", Some(&bytes), None)),
            }
        });

        self.host_fs_fn("__host_fs_realpath", 2, |interp, args| {
            let bytes = path_bytes(interp, arg(args, 0))?;
            let as_buffer = arg(args, 1).as_boolean() == Some(true);
            match std::fs::canonicalize(as_path(&bytes)) {
                Ok(real) => Ok(name_value(interp, real.as_os_str().as_bytes(), as_buffer)),
                Err(e) => Err(io_error(interp, &e, "realpath", Some(&bytes), None)),
            }
        });

        self.host_fs_fn("__host_fs_mkdir", 2, |interp, args| {
            let bytes = path_bytes(interp, arg(args, 0))?;
            let mode = int(interp, arg(args, 1))? as u32;
            std::fs::DirBuilder::new()
                .mode(mode)
                .create(as_path(&bytes))
                .map_err(|e| io_error(interp, &e, "mkdir", Some(&bytes), None))?;
            Ok(JsValue::UNDEFINED)
        });

        self.host_fs_fn("__host_fs_rmdir", 1, |interp, args| {
            let bytes = path_bytes(interp, arg(args, 0))?;
            std::fs::remove_dir(as_path(&bytes))
                .map_err(|e| io_error(interp, &e, "rmdir", Some(&bytes), None))?;
            Ok(JsValue::UNDEFINED)
        });

        self.host_fs_fn("__host_fs_unlink", 1, |interp, args| {
            let bytes = path_bytes(interp, arg(args, 0))?;
            std::fs::remove_file(as_path(&bytes))
                .map_err(|e| io_error(interp, &e, "unlink", Some(&bytes), None))?;
            Ok(JsValue::UNDEFINED)
        });

        self.host_fs_fn("__host_fs_rename", 2, |interp, args| {
            let from = path_bytes(interp, arg(args, 0))?;
            let to = path_bytes(interp, arg(args, 1))?;
            std::fs::rename(as_path(&from), as_path(&to))
                .map_err(|e| io_error(interp, &e, "rename", Some(&from), Some(&to)))?;
            Ok(JsValue::UNDEFINED)
        });

        // The platform's O_*, S_I* and *_OK values, for `fs.constants`.
        self.host_fs_fn("__host_fs_constants", 0, |interp, _args| {
            let obj_id = interp.create_object_id();
            let mut o = interp.get_object_cell_expect(obj_id).borrow_mut();
            let values: &[(&str, i64)] = &[
                ("O_RDONLY", libc::O_RDONLY as i64),
                ("O_WRONLY", libc::O_WRONLY as i64),
                ("O_RDWR", libc::O_RDWR as i64),
                ("S_IFMT", libc::S_IFMT as i64),
                ("S_IFREG", libc::S_IFREG as i64),
                ("S_IFDIR", libc::S_IFDIR as i64),
                ("S_IFCHR", libc::S_IFCHR as i64),
                ("S_IFBLK", libc::S_IFBLK as i64),
                ("S_IFIFO", libc::S_IFIFO as i64),
                ("S_IFLNK", libc::S_IFLNK as i64),
                ("S_IFSOCK", libc::S_IFSOCK as i64),
                ("O_CREAT", libc::O_CREAT as i64),
                ("O_EXCL", libc::O_EXCL as i64),
                ("O_NOCTTY", libc::O_NOCTTY as i64),
                ("O_TRUNC", libc::O_TRUNC as i64),
                ("O_APPEND", libc::O_APPEND as i64),
                ("O_DIRECTORY", libc::O_DIRECTORY as i64),
                ("O_NOFOLLOW", libc::O_NOFOLLOW as i64),
                ("O_SYNC", libc::O_SYNC as i64),
                ("O_DSYNC", libc::O_DSYNC as i64),
                ("O_NONBLOCK", libc::O_NONBLOCK as i64),
                ("S_IRWXU", libc::S_IRWXU as i64),
                ("S_IRUSR", libc::S_IRUSR as i64),
                ("S_IWUSR", libc::S_IWUSR as i64),
                ("S_IXUSR", libc::S_IXUSR as i64),
                ("S_IRWXG", libc::S_IRWXG as i64),
                ("S_IRGRP", libc::S_IRGRP as i64),
                ("S_IWGRP", libc::S_IWGRP as i64),
                ("S_IXGRP", libc::S_IXGRP as i64),
                ("S_IRWXO", libc::S_IRWXO as i64),
                ("S_IROTH", libc::S_IROTH as i64),
                ("S_IWOTH", libc::S_IWOTH as i64),
                ("S_IXOTH", libc::S_IXOTH as i64),
                ("F_OK", libc::F_OK as i64),
                ("R_OK", libc::R_OK as i64),
                ("W_OK", libc::W_OK as i64),
                ("X_OK", libc::X_OK as i64),
            ];
            for (name, value) in values {
                o.set_property_value(name, JsValue::number(*value as f64));
            }
            drop(o);
            Ok(JsValue::object(obj_id))
        });
    }
}
