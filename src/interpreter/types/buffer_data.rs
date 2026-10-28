//! Backing bytes of an `ArrayBuffer`/`SharedArrayBuffer`.

use super::SharedBufferInner;
use std::sync::Arc;

#[derive(Debug)]
pub(crate) enum BufferData {
    Owned(Vec<u8>),
    Shared(Arc<SharedBufferInner>),
    /// The linear memory of a WebAssembly instance (`--node`), see `ForeignBytes`.
    Foreign(ForeignBytes),
}

/// Bytes owned by someone else, a `(pointer, length)` window onto a WebAssembly linear
/// memory. The owner (`node_host_wasm_run::refresh_buffers`) replaces the buffer with an empty
/// detached one before any code can observe the memory moving or being freed (a grow, or the
/// store dropping), so the window is valid for as long as an `ArrayBuffer` can reach it.
#[derive(Debug)]
pub(crate) struct ForeignBytes {
    ptr: *mut u8,
    len: usize,
}

impl ForeignBytes {
    /// # Safety
    /// `ptr` must stay valid for `len` bytes until the owning `ArrayBuffer` is detached.
    pub(crate) unsafe fn new(ptr: *mut u8, len: usize) -> Self {
        ForeignBytes { ptr, len }
    }

    fn as_slice(&self) -> &[u8] {
        if self.len == 0 {
            return &[];
        }
        // SAFETY: the contract of `new`.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        if self.len == 0 {
            return &mut [];
        }
        // SAFETY: the contract of `new`.
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl BufferData {
    #[allow(clippy::len_without_is_empty)]
    pub(crate) fn len(&self) -> usize {
        match self {
            BufferData::Owned(v) => v.len(),
            BufferData::Shared(s) => s.len(),
            BufferData::Foreign(f) => f.len,
        }
    }

    pub fn resize(&mut self, new_len: usize, value: u8) {
        match self {
            BufferData::Owned(v) => v.resize(new_len, value),
            BufferData::Shared(s) => s.resize(new_len, value),
            // A WebAssembly memory buffer is never resizable.
            BufferData::Foreign(_) => {}
        }
    }

    pub(crate) fn to_vec(&self) -> Vec<u8> {
        match self {
            BufferData::Owned(v) => v.clone(),
            BufferData::Shared(s) => s.to_vec(),
            BufferData::Foreign(f) => f.as_slice().to_vec(),
        }
    }

    pub(crate) fn is_foreign(&self) -> bool {
        matches!(self, BufferData::Foreign(_))
    }

    #[allow(dead_code)]
    pub(crate) fn is_shared(&self) -> bool {
        matches!(self, BufferData::Shared(_))
    }

    #[allow(dead_code)]
    pub(crate) fn shared_inner(&self) -> Option<&Arc<SharedBufferInner>> {
        match self {
            BufferData::Shared(s) => Some(s),
            _ => None,
        }
    }

    pub(crate) fn with_read<R>(&self, f: impl FnOnce(&[u8]) -> R) -> R {
        match self {
            BufferData::Owned(v) => f(v),
            BufferData::Shared(s) => s.with_read(f),
            BufferData::Foreign(b) => f(b.as_slice()),
        }
    }

    pub(crate) fn with_write<R>(&mut self, f: impl FnOnce(&mut [u8]) -> R) -> R {
        match self {
            BufferData::Owned(v) => f(v),
            BufferData::Shared(s) => s.with_write(f),
            BufferData::Foreign(b) => f(b.as_mut_slice()),
        }
    }
}

impl Clone for BufferData {
    fn clone(&self) -> Self {
        BufferData::Owned(self.to_vec())
    }
}
