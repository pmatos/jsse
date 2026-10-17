//! Deferred transparent huge pages (#1075).
//!
//! On a host with THP `enabled=always` the kernel backs every fresh 2 MiB
//! region with a zeroed huge page. That halves the page faults of a heap of
//! hundreds of megabytes and slows a short process, which touches a fraction of
//! each region (+17% over a test262 run, #1047). The binary therefore turns THP
//! off for the process before the allocator's first allocation
//! ([`disable_at_startup`], from a constructor) and the collector turns it back
//! on after a major collection finds the process resident set past
//! [`RESIDENT_THRESHOLD_BYTES`] ([`after_major_collection`]). Memory faulted
//! before that stays on 4 KiB pages; memory faulted after it may use huge
//! pages. On a host with THP `madvise` or `never` the disable changes nothing:
//! jemalloc's default `thp:default` never calls `madvise`.

use std::sync::atomic::{AtomicBool, Ordering};

/// A few MB above a bare process (~17 MB). It is checked only after a major
/// collection, which a short script rarely reaches.
const RESIDENT_THRESHOLD_BYTES: usize = 20 << 20;

struct Deferral {
    active: AtomicBool,
}

static DEFERRAL: Deferral = Deferral {
    active: AtomicBool::new(false),
};

impl Deferral {
    /// Leaves a flag that a parent process set on purpose alone.
    fn start(&self) {
        if !thp_disabled() && set_thp_disable(true) {
            self.active.store(true, Ordering::Relaxed);
        }
    }

    fn resume_if_resident(&self, resident_bytes: impl FnOnce() -> usize) {
        if self.active.load(Ordering::Relaxed)
            && resident_bytes() >= RESIDENT_THRESHOLD_BYTES
            && self.active.swap(false, Ordering::Relaxed)
        {
            set_thp_disable(false);
        }
    }
}

#[cfg(target_os = "linux")]
fn thp_prctl(option: i32, arg: usize) -> i32 {
    unsafe extern "C" {
        fn prctl(option: i32, ...) -> i32;
    }
    unsafe { prctl(option, arg, 0usize, 0usize, 0usize) }
}

#[cfg(target_os = "linux")]
fn set_thp_disable(disable: bool) -> bool {
    const PR_SET_THP_DISABLE: i32 = 41;
    thp_prctl(PR_SET_THP_DISABLE, disable as usize) == 0
}

/// The flag survives `fork` and `exec`, so a launcher may have set it already.
#[cfg(target_os = "linux")]
fn thp_disabled() -> bool {
    const PR_GET_THP_DISABLE: i32 = 42;
    thp_prctl(PR_GET_THP_DISABLE, 0) > 0
}

#[cfg(not(target_os = "linux"))]
fn set_thp_disable(_disable: bool) -> bool {
    false
}

#[cfg(not(target_os = "linux"))]
fn thp_disabled() -> bool {
    false
}

#[cfg(target_os = "linux")]
fn resident_bytes() -> usize {
    use std::io::Read;
    const SC_PAGESIZE: i32 = 30;
    unsafe extern "C" {
        fn sysconf(name: i32) -> isize;
    }
    let mut buf = [0u8; 128];
    let Ok(mut file) = std::fs::File::open("/proc/self/statm") else {
        return 0;
    };
    let n = file.read(&mut buf).unwrap_or(0);
    let page_size = usize::try_from(unsafe { sysconf(SC_PAGESIZE) }).unwrap_or(4096);
    std::str::from_utf8(&buf[..n])
        .ok()
        .and_then(|s| s.split_whitespace().nth(1))
        .and_then(|pages| pages.parse::<usize>().ok())
        .map_or(0, |pages| pages * page_size)
}

#[cfg(not(target_os = "linux"))]
fn resident_bytes() -> usize {
    0
}

/// Disable transparent huge pages for this process until the heap has grown.
/// Allocation-free, so it can run from a constructor before the allocator's
/// first use, when a pre-`main` allocation would otherwise fault a huge page.
pub fn disable_at_startup() {
    DEFERRAL.start();
}

/// Called after every major collection; re-enables huge pages once the process
/// has outgrown a short script. One atomic load while nothing is deferred.
pub(crate) fn after_major_collection() {
    DEFERRAL.resume_if_resident(resident_bytes);
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    fn thp_enabled() -> Option<bool> {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let line = status.lines().find(|l| l.starts_with("THP_enabled:"))?;
        Some(line.split_whitespace().nth(1)? == "1")
    }

    #[test]
    fn huge_pages_return_once_the_resident_set_reaches_the_threshold() {
        if thp_enabled() != Some(true) {
            return;
        }
        let deferral = Deferral {
            active: AtomicBool::new(false),
        };
        deferral.start();
        assert_eq!(thp_enabled(), Some(false));
        deferral.resume_if_resident(|| RESIDENT_THRESHOLD_BYTES - 1);
        assert_eq!(thp_enabled(), Some(false));
        deferral.resume_if_resident(|| RESIDENT_THRESHOLD_BYTES);
        assert_eq!(thp_enabled(), Some(true));
        deferral.resume_if_resident(|| panic!("measured after the deferral ended"));
    }

    #[test]
    fn nothing_is_measured_while_no_deferral_is_active() {
        let deferral = Deferral {
            active: AtomicBool::new(false),
        };
        deferral.resume_if_resident(|| panic!("measured with no deferral"));
    }
}
