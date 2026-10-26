//! System queries behind the `os` binding (issue #1222). `/proc` parsing is
//! pure and compiled everywhere so it is unit tested on any host; the libc calls
//! are unix-only and every query fails with an errno on other targets.

#[cfg(unix)]
use std::ffi::CStr;

pub(super) struct Passwd {
    pub uid: u32,
    pub gid: u32,
    pub name: Vec<u8>,
    pub dir: Vec<u8>,
    pub shell: Option<Vec<u8>>,
}

pub(super) struct IfAddr {
    pub name: String,
    pub address: String,
    pub netmask: String,
    pub family: &'static str,
    pub mac: String,
    pub internal: bool,
    pub scope_id: i64,
}

pub(super) struct CpuInfo {
    pub model: String,
    pub speed: f64,
    pub times: [f64; 5],
}

/// Host errno values: libuv reports the platform's own numbers, which differ
/// between Linux, the BSDs and macOS.
#[cfg(unix)]
pub(super) mod errno {
    pub use libc::{EACCES, EINVAL, ENAMETOOLONG, ENOBUFS, ENOENT, ENOMEM, ENOSYS, EPERM, ESRCH};
}

#[cfg(not(unix))]
pub(super) mod errno {
    pub const EPERM: i32 = 1;
    pub const ENOENT: i32 = 2;
    pub const ESRCH: i32 = 3;
    pub const ENOMEM: i32 = 12;
    pub const EACCES: i32 = 13;
    pub const EINVAL: i32 = 22;
    pub const ENAMETOOLONG: i32 = 36;
    pub const ENOSYS: i32 = 38;
    pub const ENOBUFS: i32 = 105;
}

use errno::ENOSYS;

pub(super) fn node_platform(os: &str) -> String {
    match os {
        "macos" => "darwin",
        "windows" => "win32",
        "illumos" | "solaris" => "sunos",
        other => other,
    }
    .to_string()
}

pub(super) fn node_arch(arch: &str) -> String {
    match arch {
        "x86_64" => "x64",
        "x86" => "ia32",
        "aarch64" => "arm64",
        "powerpc64" => "ppc64",
        "loongarch64" => "loong64",
        other => other,
    }
    .to_string()
}

#[cfg(not(unix))]
pub(super) fn os_type_name(os: &str) -> &'static str {
    match os {
        "windows" => "Windows_NT",
        "macos" => "Darwin",
        _ => "Linux",
    }
}

pub(super) fn parse_meminfo(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let rest = line.strip_prefix(key)?.strip_prefix(':')?;
        rest.split_whitespace().next()?.parse::<u64>().ok()
    })
}

pub(super) fn parse_uptime(text: &str) -> Option<f64> {
    text.split_whitespace().next()?.parse().ok()
}

/// `(user, nice, sys, idle, irq)` in milliseconds for each `cpuN` line.
pub(super) fn parse_cpu_stat(text: &str, ms_per_tick: f64) -> Vec<[f64; 5]> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let label = fields.next()?.strip_prefix("cpu")?;
            if label.is_empty() || !label.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            let n: Vec<f64> = fields.map(|f| f.parse().unwrap_or(0.0)).collect();
            let at = |i: usize| n.get(i).copied().unwrap_or(0.0) * ms_per_tick;
            Some([at(0), at(1), at(2), at(3), at(5)])
        })
        .collect()
}

/// `(model name, cpu MHz)` for each `processor` block, in file order.
pub(super) fn parse_cpuinfo(text: &str) -> Vec<(String, Option<f64>)> {
    let mut out: Vec<(String, Option<f64>)> = Vec::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "processor" => out.push((String::new(), None)),
            "model name" => {
                if let Some(last) = out.last_mut() {
                    last.0 = value.to_string();
                }
            }
            "cpu MHz" => {
                if let Some(last) = out.last_mut() {
                    last.1 = value.parse().ok();
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(unix)]
pub(super) fn last_errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

#[cfg(unix)]
fn clear_errno() {
    #[cfg(any(target_os = "linux", target_os = "dragonfly"))]
    unsafe {
        *libc::__errno_location() = 0;
    }
    #[cfg(any(target_os = "android", target_os = "netbsd", target_os = "openbsd"))]
    unsafe {
        *libc::__errno() = 0;
    }
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
    unsafe {
        *libc::__error() = 0;
    }
    #[cfg(any(target_os = "illumos", target_os = "solaris"))]
    unsafe {
        *libc::___errno() = 0;
    }
}

#[cfg(unix)]
fn c_bytes(p: *const libc::c_char) -> Vec<u8> {
    unsafe { CStr::from_ptr(p) }.to_bytes().to_vec()
}

#[cfg(unix)]
fn c_chars(p: &[libc::c_char]) -> String {
    unsafe { CStr::from_ptr(p.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

/// `[sysname, version, release, machine]` from `uname(2)`.
#[cfg(unix)]
pub(super) fn uname() -> Result<[String; 4], i32> {
    let mut u: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut u) } != 0 {
        return Err(last_errno());
    }
    Ok([
        c_chars(&u.sysname),
        c_chars(&u.version),
        c_chars(&u.release),
        c_chars(&u.machine),
    ])
}

#[cfg(not(unix))]
pub(super) fn uname() -> Result<[String; 4], i32> {
    let os = std::env::consts::OS;
    Ok([
        os_type_name(os).to_string(),
        String::new(),
        String::new(),
        std::env::consts::ARCH.to_string(),
    ])
}

#[cfg(unix)]
pub(super) fn hostname() -> Result<String, i32> {
    let mut buf = [0 as libc::c_char; 1024];
    if unsafe { libc::gethostname(buf.as_mut_ptr(), buf.len() - 1) } != 0 {
        return Err(last_errno());
    }
    Ok(c_chars(&buf))
}

#[cfg(not(unix))]
pub(super) fn hostname() -> Result<String, i32> {
    std::env::var("COMPUTERNAME").map_err(|_| ENOSYS)
}

#[cfg(unix)]
pub(super) fn passwd() -> Result<Passwd, i32> {
    let uid = unsafe { libc::geteuid() };
    let mut size = 1024usize;
    loop {
        let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
        let mut buf = vec![0 as libc::c_char; size];
        let mut found: *mut libc::passwd = std::ptr::null_mut();
        let rc = unsafe { libc::getpwuid_r(uid, &mut pwd, buf.as_mut_ptr(), size, &mut found) };
        if rc == libc::ERANGE && size < (1 << 20) {
            size *= 2;
            continue;
        }
        if rc != 0 {
            return Err(rc);
        }
        if found.is_null() {
            return Err(libc::ENOENT);
        }
        return Ok(Passwd {
            uid: pwd.pw_uid,
            gid: pwd.pw_gid,
            name: c_bytes(pwd.pw_name),
            dir: c_bytes(pwd.pw_dir),
            shell: (!pwd.pw_shell.is_null()).then(|| c_bytes(pwd.pw_shell)),
        });
    }
}

#[cfg(not(unix))]
pub(super) fn passwd() -> Result<Passwd, i32> {
    Err(ENOSYS)
}

#[cfg(unix)]
pub(super) fn get_priority(pid: i32) -> Result<i32, i32> {
    clear_errno();
    let r = unsafe { libc::getpriority(libc::PRIO_PROCESS as _, pid as _) };
    match (r, last_errno()) {
        (-1, e) if e != 0 => Err(e),
        _ => Ok(r),
    }
}

#[cfg(unix)]
pub(super) fn set_priority(pid: i32, priority: i32) -> Result<(), i32> {
    match unsafe { libc::setpriority(libc::PRIO_PROCESS as _, pid as _, priority) } {
        0 => Ok(()),
        _ => Err(last_errno()),
    }
}

#[cfg(not(unix))]
pub(super) fn get_priority(_pid: i32) -> Result<i32, i32> {
    Err(ENOSYS)
}

#[cfg(not(unix))]
pub(super) fn set_priority(_pid: i32, _priority: i32) -> Result<(), i32> {
    Err(ENOSYS)
}

#[cfg(unix)]
pub(super) fn loadavg() -> [f64; 3] {
    let mut avg = [0f64; 3];
    if unsafe { libc::getloadavg(avg.as_mut_ptr(), 3) } < 3 {
        return [0.0; 3];
    }
    avg
}

#[cfg(not(unix))]
pub(super) fn loadavg() -> [f64; 3] {
    [0.0; 3]
}

#[cfg(target_os = "linux")]
fn proc_text(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

#[cfg(target_os = "linux")]
pub(super) fn total_mem() -> f64 {
    proc_text("/proc/meminfo")
        .and_then(|t| parse_meminfo(&t, "MemTotal"))
        .map_or(0.0, |kb| (kb * 1024) as f64)
}

#[cfg(target_os = "linux")]
pub(super) fn free_mem() -> f64 {
    proc_text("/proc/meminfo")
        .and_then(|t| parse_meminfo(&t, "MemAvailable").or_else(|| parse_meminfo(&t, "MemFree")))
        .map_or(0.0, |kb| (kb * 1024) as f64)
}

#[cfg(target_os = "linux")]
pub(super) fn uptime() -> Result<f64, i32> {
    proc_text("/proc/uptime")
        .and_then(|t| parse_uptime(&t))
        .ok_or(ENOSYS)
}

#[cfg(target_os = "linux")]
pub(super) fn cpus() -> Vec<CpuInfo> {
    let tick = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    let ms_per_tick = (1000 / tick.max(1)) as f64;
    let stat = proc_text("/proc/stat").unwrap_or_default();
    let info = proc_text("/proc/cpuinfo")
        .map(|t| parse_cpuinfo(&t))
        .unwrap_or_default();
    parse_cpu_stat(&stat, ms_per_tick)
        .into_iter()
        .enumerate()
        .map(|(i, times)| {
            let (model, mhz) = info.get(i).cloned().unwrap_or_default();
            let sysfs = proc_text(&format!(
                "/sys/devices/system/cpu/cpu{i}/cpufreq/scaling_cur_freq"
            ))
            .and_then(|t| t.trim().parse::<f64>().ok())
            .map(|khz| (khz / 1000.0).floor());
            CpuInfo {
                model: if model.is_empty() {
                    "unknown".to_string()
                } else {
                    model
                },
                speed: sysfs.or(mhz.map(f64::floor)).unwrap_or(0.0),
                times,
            }
        })
        .collect()
}

#[cfg(not(target_os = "linux"))]
pub(super) fn total_mem() -> f64 {
    0.0
}

#[cfg(not(target_os = "linux"))]
pub(super) fn free_mem() -> f64 {
    0.0
}

#[cfg(not(target_os = "linux"))]
pub(super) fn uptime() -> Result<f64, i32> {
    Err(ENOSYS)
}

#[cfg(not(target_os = "linux"))]
pub(super) fn cpus() -> Vec<CpuInfo> {
    Vec::new()
}

pub(super) fn available_parallelism() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

#[cfg(unix)]
const ZERO_MAC: &str = "00:00:00:00:00:00";

#[cfg(unix)]
fn c_name(ent: &libc::ifaddrs) -> String {
    unsafe { CStr::from_ptr(ent.ifa_name) }
        .to_string_lossy()
        .into_owned()
}

/// Walks a `getifaddrs` list. The caller keeps the list alive (until
/// `freeifaddrs`) for as long as the returned references are used.
#[cfg(unix)]
fn ifaddr_entries<'a>(head: *const libc::ifaddrs) -> impl Iterator<Item = &'a libc::ifaddrs> {
    std::iter::successors(unsafe { head.as_ref() }, |e| unsafe { e.ifa_next.as_ref() })
}

#[cfg(target_os = "linux")]
fn mac_table(head: *const libc::ifaddrs) -> std::collections::HashMap<String, String> {
    let mut macs = std::collections::HashMap::new();
    for ent in ifaddr_entries(head) {
        if ent.ifa_addr.is_null() || unsafe { (*ent.ifa_addr).sa_family } as i32 != libc::AF_PACKET
        {
            continue;
        }
        let ll = unsafe { &*(ent.ifa_addr as *const libc::sockaddr_ll) };
        let mac = ll.sll_addr[..6]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(":");
        macs.entry(c_name(ent)).or_insert(mac);
    }
    macs
}

#[cfg(all(unix, not(target_os = "linux")))]
fn mac_table(_head: *const libc::ifaddrs) -> std::collections::HashMap<String, String> {
    std::collections::HashMap::new()
}

#[cfg(unix)]
pub(super) fn interface_addresses() -> Result<Vec<IfAddr>, i32> {
    let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return Err(last_errno());
    }
    let macs = mac_table(head);
    let mut out = Vec::new();
    for ent in ifaddr_entries(head) {
        let flags = ent.ifa_flags as u64;
        let up = (libc::IFF_UP | libc::IFF_RUNNING) as u64;
        if flags & up != up || ent.ifa_addr.is_null() {
            continue;
        }
        let family = unsafe { (*ent.ifa_addr).sa_family } as i32;
        let (address, netmask, family_name, scope_id) = if family == libc::AF_INET {
            let a = unsafe { &*(ent.ifa_addr as *const libc::sockaddr_in) };
            let m = (!ent.ifa_netmask.is_null())
                .then(|| unsafe { &*(ent.ifa_netmask as *const libc::sockaddr_in) });
            let v4 = |s: u32| std::net::Ipv4Addr::from(u32::from_be(s)).to_string();
            (
                v4(a.sin_addr.s_addr),
                v4(m.map_or(0, |m| m.sin_addr.s_addr)),
                "IPv4",
                -1,
            )
        } else if family == libc::AF_INET6 {
            let a = unsafe { &*(ent.ifa_addr as *const libc::sockaddr_in6) };
            let m = (!ent.ifa_netmask.is_null())
                .then(|| unsafe { &*(ent.ifa_netmask as *const libc::sockaddr_in6) });
            let v6 = |b: [u8; 16]| std::net::Ipv6Addr::from(b).to_string();
            (
                v6(a.sin6_addr.s6_addr),
                v6(m.map_or([0; 16], |m| m.sin6_addr.s6_addr)),
                "IPv6",
                i64::from(a.sin6_scope_id),
            )
        } else {
            continue;
        };
        let name = c_name(ent);
        let mac = macs.get(&name).map_or(ZERO_MAC, String::as_str).to_string();
        out.push(IfAddr {
            name,
            address,
            netmask,
            family: family_name,
            mac,
            internal: flags & libc::IFF_LOOPBACK as u64 != 0,
            scope_id,
        });
    }
    unsafe { libc::freeifaddrs(head) };
    Ok(out)
}

#[cfg(not(unix))]
pub(super) fn interface_addresses() -> Result<Vec<IfAddr>, i32> {
    Err(ENOSYS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_names_follow_node() {
        for (os, want) in [
            ("linux", "linux"),
            ("macos", "darwin"),
            ("windows", "win32"),
            ("freebsd", "freebsd"),
            ("illumos", "sunos"),
            ("solaris", "sunos"),
            ("aix", "aix"),
            ("android", "android"),
        ] {
            assert_eq!(node_platform(os), want, "{os}");
        }
    }

    #[test]
    fn arch_names_follow_node() {
        for (arch, want) in [
            ("x86_64", "x64"),
            ("x86", "ia32"),
            ("aarch64", "arm64"),
            ("arm", "arm"),
            ("riscv64", "riscv64"),
            ("powerpc64", "ppc64"),
            ("s390x", "s390x"),
            ("loongarch64", "loong64"),
        ] {
            assert_eq!(node_arch(arch), want, "{arch}");
        }
    }

    #[cfg(not(unix))]
    #[test]
    fn type_names_follow_node() {
        assert_eq!(os_type_name("windows"), "Windows_NT");
        assert_eq!(os_type_name("macos"), "Darwin");
    }

    #[test]
    fn meminfo_values_are_kilobytes_by_key() {
        let text = "MemTotal:       16384 kB\nMemFree:          100 kB\nMemAvailable:    9000 kB\nSwapTotal: 0 kB\n";
        assert_eq!(parse_meminfo(text, "MemTotal"), Some(16384));
        assert_eq!(parse_meminfo(text, "MemAvailable"), Some(9000));
        assert_eq!(parse_meminfo(text, "Mem"), None);
        assert_eq!(parse_meminfo(text, "Buffers"), None);
    }

    #[test]
    fn uptime_is_the_first_field() {
        assert_eq!(parse_uptime("375047.23 8123.45\n"), Some(375047.23));
        assert_eq!(parse_uptime(""), None);
    }

    #[test]
    fn cpu_stat_skips_the_aggregate_line_and_scales_ticks() {
        let text = "cpu  10 20 30 40 50 60 70 0 0 0\ncpu0 1 2 3 4 5 6 7 0 0 0\ncpu1 8 9 10 11 12 13 14\nintr 5\n";
        assert_eq!(
            parse_cpu_stat(text, 10.0),
            vec![
                [10.0, 20.0, 30.0, 40.0, 60.0],
                [80.0, 90.0, 100.0, 110.0, 130.0]
            ]
        );
    }

    #[test]
    fn cpuinfo_pairs_models_and_speeds_per_processor() {
        let text = "processor\t: 0\nmodel name\t: Some CPU @ 3GHz\ncpu MHz\t\t: 2999.998\n\nprocessor\t: 1\nmodel name\t: Some CPU @ 3GHz\n";
        assert_eq!(
            parse_cpuinfo(text),
            vec![
                ("Some CPU @ 3GHz".to_string(), Some(2999.998)),
                ("Some CPU @ 3GHz".to_string(), None)
            ]
        );
        assert_eq!(
            parse_cpuinfo("processor : 0\n"),
            vec![(String::new(), None)]
        );
    }
}
