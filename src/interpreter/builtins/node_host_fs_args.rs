//! Shared plumbing for the `__host_fs_*` floor (issue #1217): argument
//! decoding (paths and byte views), raw descriptor syscalls and the
//! zero-copy hand-off of a read buffer to JavaScript.

use super::super::*;
use std::cell::{Cell, RefCell};
use std::ffi::{CString, OsStr};
use std::fs::File;
use std::mem::ManuallyDrop;
use std::os::fd::FromRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::rc::Rc;

pub(super) type Res<T> = Result<T, JsValue>;

pub(super) fn arg(args: &[JsValue], i: usize) -> &JsValue {
    args.get(i).unwrap_or(JsValue::undefined_ref())
}

pub(super) fn js_str(s: &str) -> JsValue {
    JsValue::string(JsString::from_str(s))
}

pub(super) fn int(interp: &mut Interpreter, v: &JsValue) -> Res<i64> {
    let n = interp.to_number_value(v)?;
    Ok(if n.is_finite() { n as i64 } else { 0 })
}

/// A byte window onto a typed array's ArrayBuffer.
pub(super) struct View {
    pub(super) buffer: Rc<RefCell<BufferData>>,
    pub(super) offset: usize,
    pub(super) len: usize,
}

pub(super) fn view_of(interp: &mut Interpreter, v: &JsValue) -> Option<View> {
    let cell = interp.get_object_cell(v.as_object_id()?)?;
    let obj = cell.borrow();
    let ta = obj.typed_array_info()?;
    if ta.is_detached.get() {
        return None;
    }
    Some(View {
        buffer: ta.buffer.clone(),
        offset: ta.byte_offset,
        len: ta.byte_length,
    })
}

pub(super) fn bad_arg(interp: &mut Interpreter, what: &str) -> JsValue {
    interp.create_type_error(&format!("{what} must be a string or Uint8Array"))
}

pub(super) fn path_bytes(interp: &mut Interpreter, v: &JsValue) -> Res<Vec<u8>> {
    if v.is_string() {
        return Ok(interp.to_string_lossy(v)?.into_bytes());
    }
    match view_of(interp, v) {
        Some(view) => view.buffer.borrow().with_read(|b| {
            b.get(view.offset..view.offset + view.len)
                .map(<[u8]>::to_vec)
                .ok_or(())
        }),
        None => Err(()),
    }
    .map_err(|()| bad_arg(interp, "path"))
}

pub(super) fn as_path(bytes: &[u8]) -> &Path {
    Path::new(OsStr::from_bytes(bytes))
}

pub(super) fn cpath(interp: &mut Interpreter, bytes: &[u8]) -> Res<CString> {
    CString::new(bytes).map_err(|_| interp.create_type_error("path must not contain null bytes"))
}

pub(super) fn last_errno() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EIO)
}

pub(super) fn interrupted() -> bool {
    last_errno() == libc::EINTR
}

pub(super) fn open_fd(path: &CString, flags: i32, mode: u32) -> Result<i32, i32> {
    loop {
        let fd =
            unsafe { libc::open(path.as_ptr(), flags | libc::O_CLOEXEC, mode as libc::c_uint) };
        if fd >= 0 {
            return Ok(fd);
        }
        if !interrupted() {
            return Err(last_errno());
        }
    }
}

pub(super) fn close_fd(fd: i32) -> Result<(), i32> {
    if unsafe { libc::close(fd) } == 0 {
        Ok(())
    } else {
        Err(last_errno())
    }
}

/// `read`/`pread` into `buf`; a negative `pos` reads at the current offset.
pub(super) fn read_fd(fd: i32, buf: &mut [u8], pos: i64) -> Result<usize, i32> {
    loop {
        let n = unsafe {
            if pos >= 0 {
                libc::pread(fd, buf.as_mut_ptr().cast(), buf.len(), pos as libc::off_t)
            } else {
                libc::read(fd, buf.as_mut_ptr().cast(), buf.len())
            }
        };
        if n >= 0 {
            return Ok(n as usize);
        }
        if !interrupted() {
            return Err(last_errno());
        }
    }
}

pub(super) fn write_fd(fd: i32, buf: &[u8], pos: i64) -> Result<usize, i32> {
    loop {
        let n = unsafe {
            if pos >= 0 {
                libc::pwrite(fd, buf.as_ptr().cast(), buf.len(), pos as libc::off_t)
            } else {
                libc::write(fd, buf.as_ptr().cast(), buf.len())
            }
        };
        if n >= 0 {
            return Ok(n as usize);
        }
        if !interrupted() {
            return Err(last_errno());
        }
    }
}

pub(super) fn write_all(fd: i32, mut buf: &[u8]) -> Result<(), i32> {
    while !buf.is_empty() {
        let n = write_fd(fd, buf, -1)?;
        if n == 0 {
            return Err(libc::EIO);
        }
        buf = &buf[n..];
    }
    Ok(())
}

/// Read to EOF from the current offset into a vector sized from `hint`, so a
/// regular file is read with one allocation and no regrowth.
pub(super) fn read_to_end(fd: i32, hint: usize) -> Result<Vec<u8>, i32> {
    let mut buf: Vec<u8> = Vec::new();
    buf.try_reserve_exact(hint.saturating_add(1))
        .map_err(|_| libc::ENOMEM)?;
    loop {
        if buf.len() == buf.capacity() {
            let grow = buf.capacity().max(8192);
            buf.try_reserve(grow).map_err(|_| libc::ENOMEM)?;
        }
        let spare = buf.spare_capacity_mut();
        let n = loop {
            let n = unsafe { libc::read(fd, spare.as_mut_ptr().cast(), spare.len()) };
            if n >= 0 {
                break n as usize;
            }
            if !interrupted() {
                return Err(last_errno());
            }
        };
        if n == 0 {
            return Ok(buf);
        }
        unsafe { buf.set_len(buf.len() + n) };
    }
}

/// A directory entry or link target: a string (invalid UTF-8 replaced, as Node
/// decodes it), or the raw bytes when the caller asked for `buffer` encoding.
pub(super) fn name_value(interp: &mut Interpreter, bytes: &[u8], as_buffer: bool) -> JsValue {
    if as_buffer {
        uint8array_from_vec(interp, bytes.to_vec())
    } else {
        js_str(&String::from_utf8_lossy(bytes))
    }
}

pub(super) fn uint8array_from_vec(interp: &mut Interpreter, bytes: Vec<u8>) -> JsValue {
    let len = bytes.len();
    let buf_rc = Rc::new(RefCell::new(BufferData::Owned(bytes)));
    let detached = Rc::new(Cell::new(false));
    let ta_info = TypedArrayInfo {
        kind: TypedArrayKind::Uint8,
        buffer: buf_rc.clone(),
        byte_offset: 0,
        byte_length: len,
        array_length: len,
        is_detached: detached.clone(),
        is_length_tracking: false,
        buffer_object_id: None,
    };
    let ab_id = interp.create_object_id();
    let ab_proto = interp.realm().arraybuffer_prototype;
    {
        let mut ab = interp.get_object_cell_expect(ab_id).borrow_mut();
        ab.class_name = "ArrayBuffer";
        ab.set_proto(ab_proto);
        ab.kind = ObjectKind::ArrayBuffer(Box::new(ArrayBufferData {
            data: buf_rc,
            detached: Some(detached),
            max_byte_length: None,
            is_shared: false,
            is_immutable: false,
            sab_shared: None,
        }));
    }
    interp.gc_track_external_bytes(len);
    let proto_id = interp.realm().uint8array_prototype.unwrap();
    let id = interp.create_typed_array_object_with_proto(ta_info, JsValue::object(ab_id), proto_id);
    JsValue::object(id)
}

pub(super) fn fd_metadata(fd: i32) -> std::io::Result<std::fs::Metadata> {
    ManuallyDrop::new(unsafe { File::from_raw_fd(fd) }).metadata()
}

impl Interpreter {
    pub(super) fn host_fs_fn(
        &mut self,
        name: &'static str,
        arity: usize,
        f: impl Fn(&mut Interpreter, &[JsValue]) -> Res<JsValue> + 'static,
    ) {
        let func = self.create_function(JsFunction::native(
            name.to_string(),
            arity,
            move |interp, _this, args| match f(interp, args) {
                Ok(v) => Completion::Normal(v),
                Err(e) => Completion::Throw(e),
            },
        ));
        self.install_host_global(name, func);
    }
}
