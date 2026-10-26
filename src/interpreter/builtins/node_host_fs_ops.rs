//! The long tail of the synchronous file system floor (issue #1218): links,
//! permissions, ownership, timestamps, truncate/fsync, `mkdtemp`, `statfs`,
//! `copyfile` and a typed directory listing. Every call fails with Node's
//! uvException shape (`node_host_fs_error.rs`), using the syscall names libuv
//! reports (`utime`, `futime`, `copyfile`, ...).

use super::super::*;
use super::node_host_fs_args::*;
use super::node_host_fs_error::{io_error, uv_error};
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::FileTypeExt;

fn uv_dirent_type(t: &std::fs::FileType) -> f64 {
    if t.is_file() {
        1.0
    } else if t.is_dir() {
        2.0
    } else if t.is_symlink() {
        3.0
    } else if t.is_fifo() {
        4.0
    } else if t.is_socket() {
        5.0
    } else if t.is_char_device() {
        6.0
    } else if t.is_block_device() {
        7.0
    } else {
        0.0
    }
}

/// libuv's `uv__fs_to_timespec`: nanosecond resolution, floored for
/// negative times, infinity means "now" and NaN leaves the time alone.
fn timespec_of(time: f64) -> libc::timespec {
    if !time.is_finite() {
        return libc::timespec {
            tv_sec: 0,
            tv_nsec: if time.is_nan() {
                libc::UTIME_OMIT
            } else {
                libc::UTIME_NOW
            } as _,
        };
    }
    let mut sec = time as libc::time_t;
    let mut nsec = ((time - sec as f64) * 1e9) as i64;
    if nsec < 0 {
        nsec += 1_000_000_000;
        sec -= 1;
    }
    libc::timespec {
        tv_sec: sec,
        tv_nsec: nsec as _,
    }
}

#[cfg(not(target_vendor = "apple"))]
fn sync_data(fd: i32) -> i32 {
    unsafe { libc::fdatasync(fd) }
}

#[cfg(target_vendor = "apple")]
fn sync_data(fd: i32) -> i32 {
    unsafe { libc::fsync(fd) }
}

fn file_stat(fd: i32) -> Result<libc::stat, i32> {
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } == 0 {
        Ok(st)
    } else {
        Err(last_errno())
    }
}

/// libuv's `uv_fs_copyfile`: the destination is created with the source's
/// mode, a self-copy is a no-op, and `excl` makes an existing target EEXIST.
#[allow(clippy::unnecessary_cast)] // mode_t is u16 on macOS
fn copy_file(src: &CString, dst: &CString, excl: bool) -> Result<(), i32> {
    let src_fd = open_fd(src, libc::O_RDONLY, 0)?;
    let result = (|| {
        let st = file_stat(src_fd)?;
        let flags = libc::O_WRONLY | libc::O_CREAT | if excl { libc::O_EXCL } else { 0 };
        let dst_fd = open_fd(dst, flags, st.st_mode as u32 & 0o7777)?;
        let copied = (|| {
            let dst_st = file_stat(dst_fd)?;
            if dst_st.st_dev == st.st_dev && dst_st.st_ino == st.st_ino {
                return Ok(());
            }
            if unsafe { libc::fchmod(dst_fd, st.st_mode as libc::mode_t & 0o7777) } != 0
                || unsafe { libc::ftruncate(dst_fd, 0) } != 0
            {
                return Err(last_errno());
            }
            #[cfg(target_os = "linux")]
            {
                let mut offset: libc::off_t = 0;
                let mut left = st.st_size as usize;
                while left > 0 {
                    let n = unsafe {
                        libc::copy_file_range(
                            src_fd,
                            &mut offset,
                            dst_fd,
                            std::ptr::null_mut(),
                            left,
                            0,
                        )
                    };
                    if n > 0 {
                        left -= n as usize;
                        continue;
                    }
                    let errno = last_errno();
                    if n == 0 {
                        return Ok(());
                    }
                    if errno == libc::EINTR {
                        continue;
                    }
                    if !matches!(
                        errno,
                        libc::EXDEV | libc::ENOSYS | libc::EOPNOTSUPP | libc::EINVAL
                    ) {
                        return Err(errno);
                    }
                    unsafe { libc::lseek(src_fd, offset, libc::SEEK_SET) };
                    break;
                }
                if left == 0 {
                    return Ok(());
                }
            }
            let mut buf = vec![0u8; 1 << 16];
            loop {
                let n = read_fd(src_fd, &mut buf, -1)?;
                if n == 0 {
                    return Ok(());
                }
                write_all(dst_fd, &buf[..n])?;
            }
        })();
        let closed = close_fd(dst_fd);
        if copied.is_err() {
            unsafe { libc::unlink(dst.as_ptr()) };
        }
        copied.and(closed)
    })();
    let closed = close_fd(src_fd);
    result.and(closed)
}

fn mkdir_errno(path: &[u8], mode: u32) -> i32 {
    let Ok(c) = CString::new(path) else {
        return libc::EINVAL;
    };
    if unsafe { libc::mkdir(c.as_ptr(), mode as libc::mode_t) } == 0 {
        0
    } else {
        last_errno()
    }
}

/// libuv/Node's `MKDirpSync`: the first directory created, or the errno that
/// stopped the walk. Parents are created by stacking the failed path and its
/// dirname, so a path component that is a file surfaces as ENOTDIR/EEXIST.
fn mkdir_p(path: &[u8], mode: u32) -> Result<Option<Vec<u8>>, i32> {
    let mut stack = vec![path.to_vec()];
    let mut first: Option<Vec<u8>> = None;
    while let Some(next) = stack.pop() {
        let mut err = mkdir_errno(&next, mode);
        loop {
            match err {
                0 => {
                    first.get_or_insert_with(|| next.clone());
                    if stack.is_empty() {
                        return Ok(first);
                    }
                }
                libc::EACCES | libc::ENOSPC | libc::ENOTDIR | libc::EPERM => return Err(err),
                libc::ENOENT => {
                    let cut = next.iter().rposition(|&b| b == b'/').unwrap_or(next.len());
                    let dirname = next[..cut].to_vec();
                    if dirname != next {
                        stack.push(next.clone());
                        stack.push(dirname);
                    } else if stack.is_empty() {
                        err = libc::EEXIST;
                        continue;
                    }
                }
                _ => match std::fs::metadata(as_path(&next)) {
                    Ok(md) if !md.is_dir() => {
                        return Err(if err == libc::EEXIST && !stack.is_empty() {
                            libc::ENOTDIR
                        } else {
                            libc::EEXIST
                        });
                    }
                    Ok(_) => {}
                    Err(e) => return Err(e.raw_os_error().unwrap_or(libc::EIO)),
                },
            }
            break;
        }
    }
    Ok(first)
}

impl Interpreter {
    pub(super) fn setup_node_host_fs_ops(&mut self) {
        // (path, mode) -> first directory created | undefined
        self.host_fs_fn("__host_fs_mkdirp", 2, |interp, args| {
            let bytes = path_bytes(interp, arg(args, 0))?;
            let mode = int(interp, arg(args, 1))? as u32;
            match mkdir_p(&bytes, mode) {
                Ok(Some(first)) => Ok(name_value(interp, &first, false)),
                Ok(None) => Ok(JsValue::UNDEFINED),
                Err(e) => Err(uv_error(interp, e, "mkdir", Some(&bytes), None)),
            }
        });

        // (target, path): the error carries the target as `path`, the link as `dest`
        self.host_fs_fn("__host_fs_symlink", 2, |interp, args| {
            let target = path_bytes(interp, arg(args, 0))?;
            let link = path_bytes(interp, arg(args, 1))?;
            std::os::unix::fs::symlink(as_path(&target), as_path(&link))
                .map_err(|e| io_error(interp, &e, "symlink", Some(&target), Some(&link)))?;
            Ok(JsValue::UNDEFINED)
        });

        self.host_fs_fn("__host_fs_link", 2, |interp, args| {
            let from = path_bytes(interp, arg(args, 0))?;
            let to = path_bytes(interp, arg(args, 1))?;
            std::fs::hard_link(as_path(&from), as_path(&to))
                .map_err(|e| io_error(interp, &e, "link", Some(&from), Some(&to)))?;
            Ok(JsValue::UNDEFINED)
        });

        // (pathOrFd, mode, isFd)
        self.host_fs_fn("__host_fs_chmod", 3, |interp, args| {
            let mode = int(interp, arg(args, 1))? as libc::mode_t;
            if arg(args, 2).as_boolean() == Some(true) {
                let fd = int(interp, arg(args, 0))? as i32;
                if unsafe { libc::fchmod(fd, mode) } != 0 {
                    return Err(uv_error(interp, last_errno(), "fchmod", None, None));
                }
            } else {
                let bytes = path_bytes(interp, arg(args, 0))?;
                let c = cpath(interp, &bytes)?;
                if unsafe { libc::chmod(c.as_ptr(), mode) } != 0 {
                    return Err(uv_error(interp, last_errno(), "chmod", Some(&bytes), None));
                }
            }
            Ok(JsValue::UNDEFINED)
        });

        // (pathOrFd, uid, gid, kind) with kind 0 chown, 1 lchown, 2 fchown
        self.host_fs_fn("__host_fs_chown", 4, |interp, args| {
            let uid = int(interp, arg(args, 1))? as libc::uid_t;
            let gid = int(interp, arg(args, 2))? as libc::gid_t;
            let kind = int(interp, arg(args, 3))?;
            if kind == 2 {
                let fd = int(interp, arg(args, 0))? as i32;
                if unsafe { libc::fchown(fd, uid, gid) } != 0 {
                    return Err(uv_error(interp, last_errno(), "fchown", None, None));
                }
                return Ok(JsValue::UNDEFINED);
            }
            let bytes = path_bytes(interp, arg(args, 0))?;
            let c = cpath(interp, &bytes)?;
            let (rc, name) = if kind == 1 {
                (unsafe { libc::lchown(c.as_ptr(), uid, gid) }, "lchown")
            } else {
                (unsafe { libc::chown(c.as_ptr(), uid, gid) }, "chown")
            };
            if rc != 0 {
                return Err(uv_error(interp, last_errno(), name, Some(&bytes), None));
            }
            Ok(JsValue::UNDEFINED)
        });

        // (pathOrFd, atimeSeconds, mtimeSeconds, kind) with kind 0 utime,
        // 1 lutime, 2 futime
        self.host_fs_fn("__host_fs_utimes", 4, |interp, args| {
            let atime = interp.to_number_value(arg(args, 1))?;
            let mtime = interp.to_number_value(arg(args, 2))?;
            let kind = int(interp, arg(args, 3))?;
            let times = [timespec_of(atime), timespec_of(mtime)];
            if kind == 2 {
                let fd = int(interp, arg(args, 0))? as i32;
                if unsafe { libc::futimens(fd, times.as_ptr()) } != 0 {
                    return Err(uv_error(interp, last_errno(), "futime", None, None));
                }
                return Ok(JsValue::UNDEFINED);
            }
            let bytes = path_bytes(interp, arg(args, 0))?;
            let c = cpath(interp, &bytes)?;
            let (flag, name) = if kind == 1 {
                (libc::AT_SYMLINK_NOFOLLOW, "lutime")
            } else {
                (0, "utime")
            };
            if unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), times.as_ptr(), flag) } != 0 {
                return Err(uv_error(interp, last_errno(), name, Some(&bytes), None));
            }
            Ok(JsValue::UNDEFINED)
        });

        self.host_fs_fn("__host_fs_ftruncate", 2, |interp, args| {
            let fd = int(interp, arg(args, 0))? as i32;
            let len = int(interp, arg(args, 1))? as libc::off_t;
            if unsafe { libc::ftruncate(fd, len) } != 0 {
                return Err(uv_error(interp, last_errno(), "ftruncate", None, None));
            }
            Ok(JsValue::UNDEFINED)
        });

        // (fd, dataOnly)
        self.host_fs_fn("__host_fs_fsync", 2, |interp, args| {
            let fd = int(interp, arg(args, 0))? as i32;
            let (rc, name) = if arg(args, 1).as_boolean() == Some(true) {
                (sync_data(fd), "fdatasync")
            } else {
                (unsafe { libc::fsync(fd) }, "fsync")
            };
            if rc != 0 {
                return Err(uv_error(interp, last_errno(), name, None, None));
            }
            Ok(JsValue::UNDEFINED)
        });

        // (prefix, asBuffer) -> the created directory
        self.host_fs_fn("__host_fs_mkdtemp", 2, |interp, args| {
            let mut template = path_bytes(interp, arg(args, 0))?;
            template.extend_from_slice(b"XXXXXX");
            let as_buffer = arg(args, 1).as_boolean() == Some(true);
            let c = cpath(interp, &template)?;
            let mut raw = c.into_bytes_with_nul();
            if unsafe { libc::mkdtemp(raw.as_mut_ptr().cast()) }.is_null() {
                return Err(uv_error(
                    interp,
                    last_errno(),
                    "mkdtemp",
                    Some(&template),
                    None,
                ));
            }
            raw.pop();
            Ok(name_value(interp, &raw, as_buffer))
        });

        // path -> [type, bsize, frsize, blocks, bfree, bavail, files, ffree]
        self.host_fs_fn("__host_fs_statfs", 1, |interp, args| {
            let bytes = path_bytes(interp, arg(args, 0))?;
            let c = cpath(interp, &bytes)?;
            let mut st: libc::statfs = unsafe { std::mem::zeroed() };
            if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
                return Err(uv_error(interp, last_errno(), "statfs", Some(&bytes), None));
            }
            #[cfg(target_os = "linux")]
            let frsize = st.f_frsize as f64;
            #[cfg(not(target_os = "linux"))]
            let frsize = st.f_bsize as f64;
            let items = [
                st.f_type as f64,
                st.f_bsize as f64,
                frsize,
                st.f_blocks as f64,
                st.f_bfree as f64,
                st.f_bavail as f64,
                st.f_files as f64,
                st.f_ffree as f64,
            ]
            .into_iter()
            .map(JsValue::number)
            .collect();
            Ok(interp.create_array(items))
        });

        // (src, dest, flags) with flags bit 0 COPYFILE_EXCL
        self.host_fs_fn("__host_fs_copyfile", 3, |interp, args| {
            let src = path_bytes(interp, arg(args, 0))?;
            let dst = path_bytes(interp, arg(args, 1))?;
            let flags = int(interp, arg(args, 2))?;
            let (cs, cd) = (cpath(interp, &src)?, cpath(interp, &dst)?);
            copy_file(&cs, &cd, flags & 1 != 0)
                .map_err(|e| uv_error(interp, e, "copyfile", Some(&src), Some(&dst)))?;
            Ok(JsValue::UNDEFINED)
        });

        // (path, asBuffers, forOpendir) -> [names, direntTypes], names sorted
        // bytewise; `opendir` failures name the syscall but not the path
        self.host_fs_fn("__host_fs_readdir_types", 3, |interp, args| {
            let bytes = path_bytes(interp, arg(args, 0))?;
            let as_buffer = arg(args, 1).as_boolean() == Some(true);
            let opendir = arg(args, 2).as_boolean() == Some(true);
            let fail = |interp: &mut Interpreter, e: &std::io::Error| {
                if opendir {
                    io_error(interp, e, "opendir", None, None)
                } else {
                    io_error(interp, e, "scandir", Some(&bytes), None)
                }
            };
            let dir = std::fs::read_dir(as_path(&bytes)).map_err(|e| fail(interp, &e))?;
            let mut entries = Vec::new();
            for entry in dir {
                let entry = entry.map_err(|e| fail(interp, &e))?;
                let kind = entry.file_type().map(|t| uv_dirent_type(&t)).unwrap_or(0.0);
                entries.push((entry.file_name(), kind));
            }
            entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            let mut names = Vec::new();
            let mut kinds = Vec::new();
            for (name, kind) in &entries {
                names.push(name_value(interp, name.as_bytes(), as_buffer));
                kinds.push(JsValue::number(*kind));
            }
            let names = interp.create_array(names);
            let kinds = interp.create_array(kinds);
            Ok(interp.create_array(vec![names, kinds]))
        });
    }
}
