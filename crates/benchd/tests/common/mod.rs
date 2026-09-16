//! The fixture scaffold the CLI-level integration tests share.
//!
//! `editable_surface_gate.rs`, `trusted_scope_freeze.rs` and `measure_job_tape_golden.rs` all drive
//! the REAL `benchd measure-job --preflight-only` binary over a synthesized on-disk bed, so they
//! need the same bed: a tape the loader accepts, a workspace whose engine resolves, a place to put
//! bytes, and a cache root that keeps the spawned binary out of the developer's real one. Each file
//! used to carry its own byte-identical copy of all four, which is three places for one fixture
//! shape to drift.
//!
//! What stays in the test files is what each one is ABOUT: its `Fixture`, its manifests, its cases.

use std::path::{Path, PathBuf};

/// The pre-GPU prerequisite/integrity exit code (die-8) every refusal below asserts.
pub const DIE_PREREQ: i32 = 8;

/// PROTOCOL-v1.1's RULED free-run window (`BENCHMARK_DECODE_STEPS`), which is the default candidate
/// regime's fixed N — so every synthesized tape carries at least this many rows and the window is
/// satisfiable from the tape alone (no `--tokens`).
pub const DECODE_STEPS: usize = bench_core::constants::BENCHMARK_DECODE_STEPS;

/// A synthesized timed-prompt tape: 8 seed tokens, `rows` reference chain, both optional keys
/// present (as every live pinned object carries them). `marker` varies the bytes ⇒ distinct sha.
///
/// SYNTHESIZED to the schema, never copied: no organizer bytes enter this repository.
pub fn tape_json(marker: i64, rows: usize) -> String {
    let chain: Vec<i64> = (0..rows as i64).map(|i| 7_000 + i).collect();
    let row_objs: Vec<serde_json::Value> = chain
        .iter()
        .map(|t| {
            serde_json::json!({
                "sequential_argmax": t,
                "top1_logit": 19.5,
                "top2_logits": [19.5, 18.375],
                "top2_tokens": [t, 321],
            })
        })
        .collect();
    serde_json::to_string(&serde_json::json!({
        "emitted_tokens": chain,
        "reference_seed_token": 4_625,
        "reference_self_consistent": true,
        "rows": row_objs,
        "seed_tokens": vec![marker; 8],
    }))
    .unwrap()
}

/// Write `body` to `path`, creating parents. Generic over `AsRef<[u8]>` so a caller hands it a
/// `&str`, a `&String` or a byte literal — the two shapes the three suites already used, unchanged
/// at every call site — without a second copy of this function existing for the other one.
pub fn write(path: &Path, body: &(impl AsRef<[u8]> + ?Sized)) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body.as_ref()).unwrap();
}

/// A workspace whose `.build/release/mlxfast-runtime-worker` exists and is executable — engine
/// resolution is a real pre-GPU check, and the worker is never SPAWNED on the preflight path.
///
/// A pinned release ships the worker binary and its `mlx.metallib` sibling TOGETHER; the #42
/// pre-GPU adjacency guard refuses at preflight when the sibling is absent. It is staged here so
/// every passing bed models a real release; the metallib-guard case removes it deliberately.
pub fn workspace(root: &Path, name: &str) -> PathBuf {
    let ws = root.join(name);
    let engine = ws.join(".build/release/mlxfast-runtime-worker");
    write(&engine, "#!/bin/sh\nexit 0\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&engine, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    write(&ws.join(".build/release/mlx.metallib"), "");
    ws
}

/// A per-test-process `XDG_CACHE_HOME`, so a spawned `benchd` writes its weights-digest sidecar
/// under the OS temp dir instead of the developer's real user cache directory.
pub fn test_cache_root() -> PathBuf {
    std::env::temp_dir().join(format!("benchd-test-cache-{}", std::process::id()))
}
