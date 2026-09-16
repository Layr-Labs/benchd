//! The ROSTER-OF-EIGHT trusted tree, shared by the two gate suites that must stage one.
//!
//! `editable_surface_gate.rs` and `trusted_scope_freeze.rs` both need the trusted-scope freeze's
//! anti-vacuous check (every roster path must exist) to PASS, so that the gate each suite is
//! actually about is the only thing that can move the verdict. Both carried a byte-identical copy
//! of the roster and of the tree that satisfies it.

use crate::bed::Fixture;
use crate::common::write;
use std::path::Path;

/// The roster-of-eight, enumerated INDEPENDENTLY of the crate constant (a private module cannot be
/// reached from an integration test). The test tree must create exactly these. Kept in lockstep
/// with `trusted_scope::ROSTER_OF_EIGHT` by the crate's own `roster_is_exactly_the_ruled_eight`
/// unit test.
///
/// The eighth entry, `benchmark.json`, is the trusted manifest itself — each suite's `Fixture`
/// materializes it per case rather than [`populate_roster`], so an absent-manifest case can leave
/// it off (and a placeholder for it would not be valid JSON).
pub const ROSTER_OF_EIGHT: [&str; 8] = [
    "Package.swift",
    "Package.resolved",
    "Sources/MLXFastTrustedHarness",
    "Sources/MLXFastCLI",
    "Sources/MLXFastCore",
    ".github",
    "tools",
    "benchmark.json",
];

/// The trusted manifest itself — a roster entry, but written per-case by each suite's
/// `Fixture::set_manifest` (or deliberately absent), never by [`populate_roster`].
const ROSTER_MANIFEST: &str = "benchmark.json";

/// Roster entries that are FILES (the rest are directories). `benchmark.json` is a file too, but is
/// materialized by `set_manifest`, so it is excluded from [`populate_roster`]'s file handling.
const ROSTER_FILES: [&str; 2] = ["Package.swift", "Package.resolved"];

/// Populate the roster-of-eight under a workspace, so the freeze's anti-vacuous check passes and
/// the arm under test is the one that binds. `benchmark.json` (the eighth entry) is SKIPPED here —
/// it is the manifest, materialized per case; a placeholder for it would not be valid JSON and
/// would defeat the absent-manifest case.
pub fn populate_roster(ws: &Path) {
    for entry in ROSTER_OF_EIGHT {
        if entry == ROSTER_MANIFEST {
            continue;
        }
        if ROSTER_FILES.contains(&entry) {
            write(&ws.join(entry), "// trusted manifest placeholder\n");
        } else {
            write(&ws.join(entry).join(".keep"), "placeholder\n");
        }
    }
}

/// The ROSTER-POPULATED half of [`crate::bed::Fixture`]: the constructor the two gate suites use
/// and the writer for the roster's eighth entry. It lives with the roster rather than with the bed
/// because only the suites that stage a roster compile this module.
impl Fixture {
    /// The bed WITH the roster staged on both legs.
    ///
    /// WIRE-1 item 1b — the candidate is a submission checkout, i.e. the baseline PLUS edits
    /// confined to editablePaths, so its non-editable surface MIRRORS the baseline. Populating the
    /// same roster on both legs keeps the write-outside-editablePaths gate a no-op for the passing
    /// case (nothing diverges outside editablePaths) while leaving the trusted-scope drift cases —
    /// which die on the baseline manifest before that gate runs — unchanged.
    pub fn new(suite: &str, tag: &str) -> Fixture {
        let fixture = Fixture::unrostered(suite, tag);
        populate_roster(&fixture.baseline);
        populate_roster(&fixture.candidate);
        fixture
    }

    /// Write `benchmark.json` (the editable-surface manifest) to BOTH legs — a submission ships the
    /// same contract it is judged against, so the contract file itself never diverges.
    pub fn set_manifest(&self, manifest: &serde_json::Value) {
        let body = serde_json::to_string_pretty(manifest).unwrap();
        write(&self.baseline.join("benchmark.json"), &body);
        write(&self.candidate.join("benchmark.json"), &body);
    }
}
