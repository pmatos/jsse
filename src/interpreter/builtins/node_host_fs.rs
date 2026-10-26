//! Descriptor and whole-file half of the synchronous file system floor for
//! Node host mode (issue #1217): `open`/`close`/`read`/`write` on real OS
//! descriptors, and `readFile`/`writeFile` that move bytes between the OS and
//! the JS ArrayBuffer without an intermediate copy. The JS `fs` module
//! (`scripts/node-fs-shim.js`) validates arguments and handles options; see
//! `docs/adr/2026-10-26-0854-node-host-mode.md`.

use super::super::*;
use super::node_host_fs_args::*;
use super::node_host_fs_error::{io_error, uv_error};
use std::ffi::CString;

/// The operand of a whole-file call: a caller-owned descriptor, or a path
/// opened (and closed) here.
enum Target {
    Fd(i32),
    Path(Vec<u8>, CString),
}

fn target_of(interp: &mut Interpreter, v: &JsValue) -> Res<Target> {
    if let Some(n) = v.as_number() {
        return Ok(Target::Fd(n as i32));
    }
    let bytes = path_bytes(interp, v)?;
    let c = cpath(interp, &bytes)?;
    Ok(Target::Path(bytes, c))
}

/// Open `target` for a whole-file operation; `owned` says whether to close.
fn open_target(
    interp: &mut Interpreter,
    target: &Target,
    flags: i32,
    mode: u32,
) -> Res<(i32, bool)> {
    match target {
        Target::Fd(fd) => Ok((*fd, false)),
        Target::Path(bytes, c) => match open_fd(c, flags, mode) {
            Ok(fd) => Ok((fd, true)),
            Err(e) => Err(uv_error(interp, e, "open", Some(bytes), None)),
        },
    }
}

fn close_after(interp: &mut Interpreter, fd: i32, owned: bool, ok: bool) -> Res<()> {
    if owned
        && let Err(e) = close_fd(fd)
        && ok
    {
        return Err(uv_error(interp, e, "close", None, None));
    }
    Ok(())
}

impl Interpreter {
    pub(super) fn setup_node_host_fs(&mut self) {
        self.host_fs_fn("__host_fs_open", 3, |interp, args| {
            let bytes = path_bytes(interp, arg(args, 0))?;
            let c = cpath(interp, &bytes)?;
            let flags = int(interp, arg(args, 1))? as i32;
            let mode = int(interp, arg(args, 2))? as u32;
            match open_fd(&c, flags, mode) {
                Ok(fd) => Ok(JsValue::number(f64::from(fd))),
                Err(e) => Err(uv_error(interp, e, "open", Some(&bytes), None)),
            }
        });

        self.host_fs_fn("__host_fs_close", 1, |interp, args| {
            let fd = int(interp, arg(args, 0))? as i32;
            close_fd(fd).map_err(|e| uv_error(interp, e, "close", None, None))?;
            Ok(JsValue::UNDEFINED)
        });

        // (fd, view, offset, length, position) -> bytesRead
        self.host_fs_fn("__host_fs_read", 5, |interp, args| {
            let fd = int(interp, arg(args, 0))? as i32;
            let view = view_of(interp, arg(args, 1)).ok_or_else(|| bad_arg(interp, "buffer"))?;
            let off = int(interp, arg(args, 2))?.max(0) as usize;
            let len = int(interp, arg(args, 3))?.max(0) as usize;
            let pos = int(interp, arg(args, 4))?;
            if off.saturating_add(len) > view.len {
                return Err(interp.create_range_error("read window exceeds the buffer"));
            }
            let start = view.offset + off;
            let result =
                view.buffer
                    .borrow_mut()
                    .with_write(|b| match b.get_mut(start..start + len) {
                        Some(window) => read_fd(fd, window, pos),
                        None => Err(libc::EINVAL),
                    });
            match result {
                Ok(n) => Ok(JsValue::number(n as f64)),
                Err(e) => Err(uv_error(interp, e, "read", None, None)),
            }
        });

        self.host_fs_fn("__host_fs_write", 5, |interp, args| {
            let fd = int(interp, arg(args, 0))? as i32;
            let view = view_of(interp, arg(args, 1)).ok_or_else(|| bad_arg(interp, "buffer"))?;
            let off = int(interp, arg(args, 2))?.max(0) as usize;
            let len = int(interp, arg(args, 3))?.max(0) as usize;
            let pos = int(interp, arg(args, 4))?;
            if off.saturating_add(len) > view.len {
                return Err(interp.create_range_error("write window exceeds the buffer"));
            }
            let start = view.offset + off;
            let result = view
                .buffer
                .borrow()
                .with_read(|b| match b.get(start..start + len) {
                    Some(window) => write_fd(fd, window, pos),
                    None => Err(libc::EINVAL),
                });
            match result {
                Ok(n) => Ok(JsValue::number(n as f64)),
                Err(e) => Err(uv_error(interp, e, "write", None, None)),
            }
        });

        // (pathOrFd, flags, asString) -> Uint8Array | string
        self.host_fs_fn("__host_fs_read_file", 3, |interp, args| {
            let target = target_of(interp, arg(args, 0))?;
            let flags = int(interp, arg(args, 1))? as i32;
            let as_string = arg(args, 2).as_boolean() == Some(true);
            let (fd, owned) = open_target(interp, &target, flags, 0o666)?;
            let result = (|| {
                let md = fd_metadata(fd).map_err(|e| io_error(interp, &e, "fstat", None, None))?;
                let hint = if md.is_file() { md.len() as usize } else { 0 };
                read_to_end(fd, hint).map_err(|e| uv_error(interp, e, "read", None, None))
            })();
            close_after(interp, fd, owned, result.is_ok())?;
            let bytes = result?;
            if as_string {
                return Ok(JsValue::string(JsString::from_str(
                    &String::from_utf8_lossy(&bytes),
                )));
            }
            Ok(uint8array_from_vec(interp, bytes))
        });

        // (pathOrFd, string | Uint8Array, flags, mode)
        self.host_fs_fn("__host_fs_write_file", 4, |interp, args| {
            let target = target_of(interp, arg(args, 0))?;
            let data = arg(args, 1);
            let flags = int(interp, arg(args, 2))? as i32;
            let mode = int(interp, arg(args, 3))? as u32;
            let text = if data.is_string() {
                Some(interp.to_string_lossy(data)?)
            } else {
                None
            };
            let view = match text {
                Some(_) => None,
                None => Some(view_of(interp, data).ok_or_else(|| bad_arg(interp, "data"))?),
            };
            let (fd, owned) = open_target(interp, &target, flags, mode)?;
            let result = match (&text, &view) {
                (Some(s), _) => write_all(fd, s.as_bytes()),
                (None, Some(v)) => {
                    v.buffer
                        .borrow()
                        .with_read(|b| match b.get(v.offset..v.offset + v.len) {
                            Some(window) => write_all(fd, window),
                            None => Err(libc::EINVAL),
                        })
                }
                (None, None) => Ok(()),
            };
            let ok = result.is_ok();
            close_after(interp, fd, owned, ok)?;
            result.map_err(|e| uv_error(interp, e, "write", None, None))?;
            Ok(JsValue::UNDEFINED)
        });
    }
}
