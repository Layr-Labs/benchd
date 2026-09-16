//! ONE fresh temp directory for a unit test.
//!
//! Four test modules — `weights_preflight`, `file_digest`, `editable_divergence`, `byte_budget` —
//! each carried a create-clean-return helper of its own, in two shapes: two suffixed the directory
//! with the thread id and two with the wall-clock nanos. Same job, two answers to "what makes this
//! name unique", four places to fix when one of them is not unique enough.

/// A fresh, EMPTY directory under the OS temp dir, named for `tag`.
///
/// Unique by PROCESS ID AND WALL-CLOCK NANOS: the pid separates concurrent `cargo test` runs, and
/// the nanos separate two calls within one process — including two calls from the SAME thread, the
/// case a thread-id suffix does not cover. Removed first, so a leftover directory from an earlier
/// run can never make a test read bytes it did not write.
pub fn tmp(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "benchd-unit-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}
