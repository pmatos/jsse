use std::process::ExitCode;

#[cfg(all(feature = "jemalloc", not(any(windows, target_family = "wasm", miri))))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

/// Runs before the Rust runtime's first allocation: jemalloc's own bookkeeping
/// is faulted in by then and would take a zeroed huge page on a THP=always host.
#[cfg(all(feature = "jemalloc", target_os = "linux", not(miri)))]
#[used]
#[unsafe(link_section = ".init_array")]
static DISABLE_HUGE_PAGES_AT_STARTUP: extern "C" fn() = {
    extern "C" fn disable() {
        jsse::thp::disable_at_startup();
    }
    disable
};

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
    #[cfg(all(feature = "jemalloc", target_os = "linux", not(miri)))]
    jsse::thp::settle_deferral();
    jsse::run_cli()
}
