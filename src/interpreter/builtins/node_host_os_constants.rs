//! `os.constants` tables for the `os` binding (issue #1222). Unix values come
//! from the `libc` crate, so each target reports its own numbers; the order
//! is Node's (`node_constants.cc`), which `Object.keys` exposes. Only Linux
//! and macOS have errno tables; Windows has signals only.

pub(super) type Table = &'static [(&'static str, i32)];

#[cfg(unix)]
macro_rules! table {
    ($($(#[$m:meta])* $n:ident),* $(,)?) => {
        &[$($(#[$m])* (stringify!($n), libc::$n as i32)),*]
    };
}

pub(super) struct Constants {
    pub signals: Table,
    pub errno: Table,
    pub dlopen: Table,
}

#[cfg(unix)]
pub(super) fn host_constants() -> Constants {
    Constants {
        signals: SIGNALS,
        errno: ERRNO,
        dlopen: DLOPEN,
    }
}

#[cfg(not(unix))]
pub(super) fn host_constants() -> Constants {
    Constants {
        signals: WINDOWS_SIGNALS,
        errno: &[],
        dlopen: &[],
    }
}

#[cfg_attr(unix, allow(dead_code))]
const WINDOWS_SIGNALS: Table = &[
    ("SIGHUP", 1),
    ("SIGINT", 2),
    ("SIGILL", 4),
    ("SIGABRT", 22),
    ("SIGFPE", 8),
    ("SIGKILL", 9),
    ("SIGSEGV", 11),
    ("SIGTERM", 15),
    ("SIGBREAK", 21),
    ("SIGWINCH", 28),
];

#[cfg(unix)]
const SIGNALS: Table = &[
    ("SIGHUP", libc::SIGHUP),
    ("SIGINT", libc::SIGINT),
    ("SIGQUIT", libc::SIGQUIT),
    ("SIGILL", libc::SIGILL),
    ("SIGTRAP", libc::SIGTRAP),
    ("SIGABRT", libc::SIGABRT),
    ("SIGIOT", libc::SIGABRT),
    ("SIGBUS", libc::SIGBUS),
    ("SIGFPE", libc::SIGFPE),
    ("SIGKILL", libc::SIGKILL),
    ("SIGUSR1", libc::SIGUSR1),
    ("SIGSEGV", libc::SIGSEGV),
    ("SIGUSR2", libc::SIGUSR2),
    ("SIGPIPE", libc::SIGPIPE),
    ("SIGALRM", libc::SIGALRM),
    ("SIGTERM", libc::SIGTERM),
    ("SIGCHLD", libc::SIGCHLD),
    #[cfg(target_os = "linux")]
    ("SIGSTKFLT", libc::SIGSTKFLT),
    ("SIGCONT", libc::SIGCONT),
    ("SIGSTOP", libc::SIGSTOP),
    ("SIGTSTP", libc::SIGTSTP),
    ("SIGTTIN", libc::SIGTTIN),
    ("SIGTTOU", libc::SIGTTOU),
    ("SIGURG", libc::SIGURG),
    ("SIGXCPU", libc::SIGXCPU),
    ("SIGXFSZ", libc::SIGXFSZ),
    ("SIGVTALRM", libc::SIGVTALRM),
    ("SIGPROF", libc::SIGPROF),
    ("SIGWINCH", libc::SIGWINCH),
    ("SIGIO", libc::SIGIO),
    #[cfg(target_os = "linux")]
    ("SIGPOLL", libc::SIGPOLL),
    #[cfg(target_os = "linux")]
    ("SIGPWR", libc::SIGPWR),
    #[cfg(target_os = "macos")]
    ("SIGINFO", libc::SIGINFO),
    ("SIGSYS", libc::SIGSYS),
];

#[cfg(all(unix, any(target_os = "linux", target_os = "macos")))]
const ERRNO: Table = table![
    E2BIG,
    EACCES,
    EADDRINUSE,
    EADDRNOTAVAIL,
    EAFNOSUPPORT,
    EAGAIN,
    EALREADY,
    EBADF,
    EBADMSG,
    EBUSY,
    ECANCELED,
    ECHILD,
    ECONNABORTED,
    ECONNREFUSED,
    ECONNRESET,
    EDEADLK,
    EDESTADDRREQ,
    EDOM,
    EDQUOT,
    EEXIST,
    EFAULT,
    EFBIG,
    EHOSTUNREACH,
    EIDRM,
    EILSEQ,
    EINPROGRESS,
    EINTR,
    EINVAL,
    EIO,
    EISCONN,
    EISDIR,
    ELOOP,
    EMFILE,
    EMLINK,
    EMSGSIZE,
    EMULTIHOP,
    ENAMETOOLONG,
    ENETDOWN,
    ENETRESET,
    ENETUNREACH,
    ENFILE,
    ENOBUFS,
    ENODATA,
    ENODEV,
    ENOENT,
    ENOEXEC,
    ENOLCK,
    ENOLINK,
    ENOMEM,
    ENOMSG,
    ENOPROTOOPT,
    ENOSPC,
    ENOSR,
    ENOSTR,
    ENOSYS,
    ENOTCONN,
    ENOTDIR,
    ENOTEMPTY,
    ENOTSOCK,
    ENOTSUP,
    ENOTTY,
    ENXIO,
    EOPNOTSUPP,
    EOVERFLOW,
    EPERM,
    EPIPE,
    EPROTO,
    EPROTONOSUPPORT,
    EPROTOTYPE,
    ERANGE,
    EROFS,
    ESPIPE,
    ESRCH,
    ESTALE,
    ETIME,
    ETIMEDOUT,
    ETXTBSY,
    EWOULDBLOCK,
    EXDEV,
];

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
const ERRNO: Table = &[];

#[cfg(all(unix, target_os = "linux"))]
const DLOPEN: Table = table![RTLD_LAZY, RTLD_NOW, RTLD_GLOBAL, RTLD_LOCAL, RTLD_DEEPBIND];

#[cfg(all(unix, not(target_os = "linux")))]
const DLOPEN: Table = table![RTLD_LAZY, RTLD_NOW, RTLD_GLOBAL, RTLD_LOCAL];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_signals_match_node() {
        let names: Vec<&str> = WINDOWS_SIGNALS.iter().map(|(n, _)| *n).collect();
        assert_eq!(
            names,
            [
                "SIGHUP", "SIGINT", "SIGILL", "SIGABRT", "SIGFPE", "SIGKILL", "SIGSEGV", "SIGTERM",
                "SIGBREAK", "SIGWINCH"
            ]
        );
        assert!(WINDOWS_SIGNALS.contains(&("SIGABRT", 22)));
        assert!(WINDOWS_SIGNALS.contains(&("SIGBREAK", 21)));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_numbers_are_the_kernels() {
        let c = host_constants();
        assert!(c.signals.contains(&("SIGUSR1", 10)));
        assert!(c.signals.contains(&("SIGSTKFLT", 16)));
        assert!(c.errno.contains(&("EAGAIN", 11)));
        assert!(c.errno.contains(&("EADDRINUSE", 98)));
        assert_eq!(c.errno.len(), 79);
        assert!(c.dlopen.contains(&("RTLD_DEEPBIND", 8)));
    }
}
