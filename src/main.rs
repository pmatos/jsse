use std::process::ExitCode;

#[cfg(all(feature = "jemalloc", not(any(windows, target_family = "wasm", miri))))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

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
    unsafe {
        mallopt(M_ARENA_MAX, 1);
    }
}

fn main() -> ExitCode {
    #[cfg(all(
        feature = "jemalloc",
        target_os = "linux",
        target_env = "gnu",
        not(miri)
    ))]
    limit_glibc_arenas();
    jsse::run_cli()
}
