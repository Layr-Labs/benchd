//! The preflight bed the three CLI suites share.
//!
//! `editable_surface_gate.rs`, `trusted_scope_freeze.rs` and `measure_job_tape_golden.rs` each
//! carried a byte-identical copy of this struct and its constructor: the same temp root, the same
//! two workspaces, the same weights dir. That is one fixture shape in three places to drift.
//!
//! The ROSTER-populated half of the bed — the two gate suites' `Fixture::new` and its
//! `benchmark.json` writer — lives in [`crate::roster`], which only those two suites compile:
//! `measure_job_tape_golden.rs` stages no roster, so it declares no `roster` module and builds the
//! bed through [`Fixture::unrostered`].
//!
//! It is a module of its own rather than part of [`crate::common`] because `common` holds the
//! PRIMITIVES every suite uses to build whatever bed it needs — `write`, `workspace`, `tape_json`,
//! the cache root — and this is the assembled bed those primitives make.
//!
//! What stays in the suites is what each one is ABOUT: its cases, its manifests, its mutations.

use std::path::PathBuf;

use crate::common::{workspace, write};

/// A preflight bed: stub engines, weights, and the mirrored baseline + candidate workspaces. The
/// roster (where a suite stages one), the `benchmark.json` and the editable trees are written on
/// top of it, per case.
pub struct Fixture {
    pub root: PathBuf,
    pub candidate: PathBuf,
    pub baseline: PathBuf,
    pub weights: PathBuf,
}

impl Fixture {
    /// The bed WITHOUT the roster, for a suite that stages none: `measure_job_tape_golden.rs`
    /// drives preflight over a tape and a contract, and a roster it never reads would be bytes on
    /// disk no case is about.
    ///
    /// The root is named for `suite` and `tag`; the pid + nanos suffix is what keeps parallel cases
    /// off each other's directories.
    pub fn unrostered(suite: &str, tag: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!(
            "benchd-{suite}-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let candidate = workspace(&root, "candidate-ws");
        let baseline = workspace(&root, "baseline-ws");
        let weights = root.join("weights");
        write(&weights.join("config.json"), "{}");
        Fixture {
            root,
            candidate,
            baseline,
            weights,
        }
    }
}
