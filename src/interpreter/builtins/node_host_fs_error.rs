//! errno-shaped errors for the `__host_fs_*` floor (issue #1217): Node's
//! `code`, `errno`, `syscall`, `path`, `dest` and message layout, so the JS
//! `fs` module only has to let them through.

use super::super::*;
use super::node_host_fs_args::js_str;

/// `(code, libuv description)` for an errno; anything unlisted is `UNKNOWN`.
fn describe(errno: i32) -> (&'static str, &'static str) {
    match errno {
        libc::E2BIG => ("E2BIG", "argument list too long"),
        libc::EACCES => ("EACCES", "permission denied"),
        libc::EAGAIN => ("EAGAIN", "resource temporarily unavailable"),
        libc::EBADF => ("EBADF", "bad file descriptor"),
        libc::EBUSY => ("EBUSY", "resource busy or locked"),
        libc::EEXIST => ("EEXIST", "file already exists"),
        libc::EFBIG => ("EFBIG", "file too large"),
        libc::EINVAL => ("EINVAL", "invalid argument"),
        libc::EIO => ("EIO", "i/o error"),
        libc::EISDIR => ("EISDIR", "illegal operation on a directory"),
        libc::ELOOP => ("ELOOP", "too many symbolic links encountered"),
        libc::EMFILE => ("EMFILE", "too many open files"),
        libc::EMLINK => ("EMLINK", "too many links"),
        libc::ENAMETOOLONG => ("ENAMETOOLONG", "name too long"),
        libc::ENFILE => ("ENFILE", "file table overflow"),
        libc::ENODEV => ("ENODEV", "no such device"),
        libc::ENOENT => ("ENOENT", "no such file or directory"),
        libc::ENOMEM => ("ENOMEM", "not enough memory"),
        libc::ENOSPC => ("ENOSPC", "no space left on device"),
        libc::ENOSYS => ("ENOSYS", "function not implemented"),
        libc::ENOTDIR => ("ENOTDIR", "not a directory"),
        libc::ENOTEMPTY => ("ENOTEMPTY", "directory not empty"),
        libc::ENOTTY => ("ENOTTY", "inappropriate ioctl for device"),
        libc::ENXIO => ("ENXIO", "no such device or address"),
        libc::EPERM => ("EPERM", "operation not permitted"),
        libc::EPIPE => ("EPIPE", "broken pipe"),
        libc::ERANGE => ("ERANGE", "result too large"),
        libc::EROFS => ("EROFS", "read-only file system"),
        libc::ESPIPE => ("ESPIPE", "invalid seek"),
        libc::ETXTBSY => ("ETXTBSY", "text file is busy"),
        libc::EXDEV => ("EXDEV", "cross-device link not permitted"),
        _ => ("UNKNOWN", "unknown error"),
    }
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// An `Error` carrying Node's uvException shape for `errno` (positive, as the
/// OS reports it; the property is negated like libuv's).
pub(super) fn uv_error(
    interp: &mut Interpreter,
    errno: i32,
    syscall: &str,
    path: Option<&[u8]>,
    dest: Option<&[u8]>,
) -> JsValue {
    let (code, desc) = describe(errno);
    let mut msg = format!("{code}: {desc}, {syscall}");
    if let Some(p) = path {
        msg.push_str(&format!(" '{}'", lossy(p)));
    }
    if let Some(d) = dest {
        msg.push_str(&format!(" -> '{}'", lossy(d)));
    }
    let error = interp.create_error("Error", &msg);
    if let Some(id) = error.as_object_id() {
        let mut o = interp.get_object_cell_expect(id).borrow_mut();
        let mut props = vec![
            ("errno", JsValue::number(-f64::from(errno))),
            ("code", js_str(code)),
            ("syscall", js_str(syscall)),
        ];
        if let Some(p) = path {
            props.push(("path", js_str(&lossy(p))));
        }
        if let Some(d) = dest {
            props.push(("dest", js_str(&lossy(d))));
        }
        for (name, value) in props {
            o.insert_property(
                name.to_string(),
                PropertyDescriptor::data(value, true, true, true),
            );
        }
    }
    error
}

pub(super) fn io_error(
    interp: &mut Interpreter,
    err: &std::io::Error,
    syscall: &str,
    path: Option<&[u8]>,
    dest: Option<&[u8]>,
) -> JsValue {
    uv_error(
        interp,
        err.raw_os_error().unwrap_or(libc::EIO),
        syscall,
        path,
        dest,
    )
}
