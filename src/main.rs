use std::process::ExitCode;

#[cfg(all(feature = "jemalloc", not(any(windows, target_family = "wasm", miri))))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

/// Transparent huge pages make every fresh 2 MiB extent a zeroed huge page on
/// hosts with THP=always: ~17% more wall time over a test262 run of short
/// processes, and no benefit for the long benchmarks (#1047).
#[cfg(all(feature = "jemalloc", target_os = "linux", not(miri)))]
#[unsafe(export_name = "_rjem_malloc_conf")]
pub static MALLOC_CONF: Option<&'static u8> = Some(&b"thp:never\0"[0]);

/// jemalloc serves every Rust allocation, but glibc still backs libc's own
/// (per-thread TLS blocks) and gives each thread an arena that reserves 64 MiB
/// of address space. A multi-agent Atomics test then exhausts the test
/// runner's RLIMIT_AS and an allocation fails; one shared arena avoids that.
#[cfg(all(
    feature = "jemalloc",
    target_os = "linux",
    target_env = "gnu",
    not(miri)
))]
fn limit_glibc_arenas() {
    const M_ARENA_MAX: i32 = -8;
    unsafe extern "C" {
        fn mallopt(param: i32, value: i32) -> i32;
    }
    if std::env::var_os("MALLOC_ARENA_MAX").is_none() {
        unsafe {
            mallopt(M_ARENA_MAX, 1);
        }
    }
}

#[cfg(not(all(
    feature = "jemalloc",
    target_os = "linux",
    target_env = "gnu",
    not(miri)
)))]
fn limit_glibc_arenas() {}

fn main() -> ExitCode {
    limit_glibc_arenas();
    jsse::run_cli()
}
