//! The `<module source>` host specifier must not be capturable by a real
//! filesystem entry of the same name (jsse#222 review finding).
//!
//! Its module-registry key is a bare relative path, so any `canonicalize()` on
//! it succeeds whenever jsse runs in a directory holding an entry called
//! `<module source>` — rebinding the specifier to that file's registry slot.
//! This lives in `tests/` rather than `test262-extra/` because the condition is
//! the *process working directory*, which only a spawned child can control.

use std::fs;
use std::process::Command;

fn run_in(dir: &std::path::Path, entry: &str) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .current_dir(dir)
        .args(["--module", entry])
        .output()
        .expect("failed to run jsse");
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.success(), combined)
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("jsse-module-source-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("failed to create scratch dir");
    dir
}

/// A named re-export of a binding the host module does not have is a link-time
/// SyntaxError, and stays one even when a file called `<module source>` sits in
/// the working directory offering that very export.
#[test]
fn a_real_file_does_not_capture_the_host_specifier() {
    let reexport = "export { x } from '<module source>';\n";

    let clean = scratch("clean");
    fs::write(clean.join("reexport.mjs"), reexport).unwrap();
    let (clean_ok, clean_out) = run_in(&clean, "reexport.mjs");

    let shadowed = scratch("shadowed");
    fs::write(shadowed.join("reexport.mjs"), reexport).unwrap();
    fs::write(
        shadowed.join("<module source>"),
        "export const x = 'from the file on disk';\n",
    )
    .unwrap();
    let (shadowed_ok, shadowed_out) = run_in(&shadowed, "reexport.mjs");

    assert!(
        !clean_ok,
        "re-exporting an absent binding should fail, got: {clean_out}"
    );
    assert!(
        clean_out.contains("has no export named 'x'"),
        "expected a link error naming the missing export, got: {clean_out}"
    );
    assert_eq!(
        (clean_ok, clean_out.clone()),
        (shadowed_ok, shadowed_out.clone()),
        "a file named `<module source>` in the working directory changed the \
         outcome: clean={clean_out:?} shadowed={shadowed_out:?}"
    );
}

/// The synthetic record still backs every phase when the file is present: one
/// namespace with no bindings, and a source-phase binding that is not the file.
#[test]
fn the_host_record_wins_over_a_real_file_in_every_phase() {
    let dir = scratch("phases");
    fs::write(
        dir.join("<module source>"),
        "export const x = 1;\nexport const secret = 'leaked';\n",
    )
    .unwrap();
    fs::write(
        dir.join("main.mjs"),
        r#"
import source S from '<module source>';
import * as ns from '<module source>';
const dyn = await import('<module source>');
if (typeof S !== 'object' || S === null) throw new Error('no Module Source');
if (Object.keys(ns).length !== 0) throw new Error('namespace leaked: ' + Object.keys(ns));
if (Object.keys(dyn).length !== 0) throw new Error('dynamic namespace leaked');
if (ns !== dyn) throw new Error('phases disagree on the record');
"#,
    )
    .unwrap();

    let (ok, out) = run_in(&dir, "main.mjs");
    assert!(ok, "host record did not win over the on-disk file: {out}");
}
