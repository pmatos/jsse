#![no_main]

use jsse_fuzz::{
    Verdict, classify, jsse_release_binary, node_available, repo_root, run_engine_subprocess,
    run_node_subprocess,
};
use libfuzzer_sys::fuzz_target;
use std::path::Path;
use std::time::Duration;

/// Per-engine wall-clock budget. Comfortably under libFuzzer's own
/// `-timeout` (set to 30s when running this target, see the workflow/skill)
/// so a genuinely hung pair reports as two clean per-engine kills rather
/// than one misattributed libFuzzer-level timeout.
const PER_ENGINE_TIMEOUT: Duration = Duration::from_secs(10);

/// Deletes the file it wraps on drop, however the fuzz iteration exits
/// (including via `panic!` on a Tier 1/2 finding).
struct TempSourceFile(std::path::PathBuf);

impl Drop for TempSourceFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Delivers the fuzzed bytes by file path under `$TMPDIR` (never a
/// hardcoded `/tmp/...` path), not `-e <src>`/`node -e <src>`: fuzzed bytes
/// can contain interior NULs (which make `Command`'s argument encoding fail
/// outright) or exceed a single-argument length limit in pathological
/// cases. A file sidesteps both.
fn write_temp_source(src: &[u8]) -> std::io::Result<TempSourceFile> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "jsse-fuzz-differential-{}-{nanos}.js",
        std::process::id()
    ));
    std::fs::write(&path, src)?;
    Ok(TempSourceFile(path))
}

fuzz_target!(|data: &[u8]| {
    let Ok(src) = std::str::from_utf8(data) else {
        return;
    };

    static NODE_AVAILABLE: std::sync::LazyLock<bool> = std::sync::LazyLock::new(node_available);

    let jsse_bin = jsse_release_binary();
    if !Path::new(&jsse_bin).exists() || !*NODE_AVAILABLE {
        return;
    }

    let Ok(src_file) = write_temp_source(src.as_bytes()) else {
        return;
    };

    let jsse_outcome = run_engine_subprocess(&jsse_bin, &src_file.0, PER_ENGINE_TIMEOUT);
    let node_outcome = run_node_subprocess(&repo_root(), &src_file.0, PER_ENGINE_TIMEOUT);

    match classify(&jsse_outcome, &node_outcome) {
        Verdict::Tier1(reason) => panic!("Tier 1 divergence: {reason}"),
        Verdict::Tier2(reason) => panic!("Tier 2 divergence: {reason}"),
        Verdict::Recorded => {}
    }
});
