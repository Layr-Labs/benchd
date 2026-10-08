//! Property tests for the score / floor / acceptance-band pure functions.
//!
//! Ports the invariants the Swift example-based tests demonstrate
//! (`Tests/MLXFastTests/ScoreTests.swift`, `Tests/MLXFastTests/AcceptanceBandTests.swift`)
//! into generalized `proptest` cases against the production code in
//! `bench-core::score` (ported from `Sources/MLXFastCore/Score.swift` +
//! `Sources/MLXFastCore/AcceptanceBand.swift`). Closes debt item #45
//! (score monotonicity + acceptance-band symmetry).
//!
//! Constants come from `bench_core::constants` (the same values the production
//! score path uses) rather than being hardcoded, so these tests track the code.

use bench_core::constants::{
    SCORE_DECODE_SPEEDUP_FLOOR, SCORE_DECODE_WEIGHT, SCORE_PREFILL_SPEEDUP_FLOOR,
    SCORE_PREFILL_WEIGHT,
};
use bench_core::score::{
    check_fast_side, passes_speedup_floors, score, score_default_weights, speedup,
};
use proptest::prelude::*;

/// Positive, finite, well-conditioned seconds-per-token / speedup magnitudes.
/// Bounded away from 0 and from overflow so `powf` stays numerically clean.
fn pos() -> impl Strategy<Value = f64> {
    1e-4f64..1e4f64
}

proptest! {
    // ---- speedup (Score.swift:4-16) ----

    /// For finite positive inputs, speedup is exactly baseline/candidate and positive.
    #[test]
    fn speedup_is_ratio_and_positive(baseline in pos(), candidate in pos()) {
        let s = speedup(baseline, candidate);
        prop_assert!(s.is_finite() && s > 0.0);
        prop_assert_eq!(s, baseline / candidate);
    }

    /// Monotone in the candidate: a faster candidate (smaller seconds-per-token)
    /// never yields a smaller speedup. (Score.swift:4-16.)
    #[test]
    fn speedup_monotonic_in_candidate(baseline in pos(), c1 in pos(), c2 in pos()) {
        let (fast, slow) = if c1 <= c2 { (c1, c2) } else { (c2, c1) };
        prop_assert!(speedup(baseline, fast) >= speedup(baseline, slow));
    }

    /// Guard: any non-finite or non-positive operand collapses to 0.
    /// (Score.swift:9-14 `guard ... else { return 0 }`.)
    #[test]
    fn speedup_guards_return_zero(x in pos()) {
        for bad in [0.0f64, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            prop_assert_eq!(speedup(bad, x), 0.0);
            prop_assert_eq!(speedup(x, bad), 0.0);
        }
    }

    // ---- score monotonicity / shape (Score.swift:18-48; issue #45) ----

    /// #45 monotonicity: holding prefill and both baselines fixed, a faster decode
    /// (smaller decode seconds-per-token => larger decode speedup) never lowers the
    /// score. (Score.swift:44-47 — score is increasing in each component speedup.)
    #[test]
    fn score_monotonic_in_decode(
        baseline_decode in pos(),
        baseline_prefill in pos(),
        prefill_spt in pos(),
        d1 in pos(),
        d2 in pos(),
    ) {
        let (fast, slow) = if d1 <= d2 { (d1, d2) } else { (d2, d1) };
        let s_fast = score_default_weights(fast, prefill_spt, baseline_decode, baseline_prefill);
        let s_slow = score_default_weights(slow, prefill_spt, baseline_decode, baseline_prefill);
        // Relative slack absorbs benign float noise; the direction is what matters.
        prop_assert!(s_fast >= s_slow * (1.0 - 1e-12));
    }

    /// #45 monotonicity, prefill axis: symmetric statement for the prefill component.
    #[test]
    fn score_monotonic_in_prefill(
        baseline_decode in pos(),
        baseline_prefill in pos(),
        decode_spt in pos(),
        p1 in pos(),
        p2 in pos(),
    ) {
        let (fast, slow) = if p1 <= p2 { (p1, p2) } else { (p2, p1) };
        let s_fast = score_default_weights(decode_spt, fast, baseline_decode, baseline_prefill);
        let s_slow = score_default_weights(decode_spt, slow, baseline_decode, baseline_prefill);
        prop_assert!(s_fast >= s_slow * (1.0 - 1e-12));
    }

    /// Weighted-geometric-mean shape: the score always lies between the two component
    /// speedups. (Score.swift:44-47.)
    #[test]
    fn score_between_component_speedups(
        decode_spt in pos(),
        prefill_spt in pos(),
        baseline_decode in pos(),
        baseline_prefill in pos(),
    ) {
        let ds = speedup(baseline_decode, decode_spt);
        let ps = speedup(baseline_prefill, prefill_spt);
        let s = score_default_weights(decode_spt, prefill_spt, baseline_decode, baseline_prefill);
        let lo = ds.min(ps);
        let hi = ds.max(ps);
        prop_assert!(s >= lo * (1.0 - 1e-9) && s <= hi * (1.0 + 1e-9));
    }

    /// Equal component speedups collapse the weighted geomean to that speedup.
    /// (Mirrors ScoreTests.swift:5-27; Score.swift:44-47.)
    #[test]
    fn score_equal_speedups_equals_that_speedup(
        decode_spt in pos(),
        prefill_spt in pos(),
        k in 0.1f64..10.0f64,
    ) {
        // baseline = k * candidate on both axes => both speedups == k.
        let s = score_default_weights(decode_spt, prefill_spt, k * decode_spt, k * prefill_spt);
        prop_assert!((s - k).abs() <= k * 1e-9);
    }

    /// Exponent-weighting shape: when prefill is neutral (speedup 1), the score is the
    /// decode speedup raised to the decode weight share. With the default 0.75/0.25
    /// weights this is `decode_speedup^0.75`. (Mirrors ScoreTests.swift:5-27 `2^0.75`;
    /// Score.swift:44-47.)
    #[test]
    fn score_prefill_neutral_is_decode_speedup_pow_weight(
        decode_spt in pos(),
        baseline_decode in pos(),
        prefill_spt in pos(),
    ) {
        // baseline_prefill == prefill_spt => prefill speedup exactly 1.
        let s = score_default_weights(decode_spt, prefill_spt, baseline_decode, prefill_spt);
        let ds = speedup(baseline_decode, decode_spt);
        let total = SCORE_DECODE_WEIGHT + SCORE_PREFILL_WEIGHT;
        let expected = ds.powf(SCORE_DECODE_WEIGHT / total);
        prop_assert!((s - expected).abs() <= expected * 1e-9);
    }

    /// Guard: a non-positive or non-finite timing / baseline yields NaN.
    /// (Score.swift:36-42 `guard ... else { return .nan }`; ScoreTests.swift:63-70.)
    #[test]
    fn score_rejects_nonpositive_and_nonfinite_timings(
        decode_spt in pos(),
        prefill_spt in pos(),
        baseline_decode in pos(),
        baseline_prefill in pos(),
    ) {
        for bad in [0.0f64, -1.0, f64::NAN, f64::INFINITY] {
            prop_assert!(
                score_default_weights(bad, prefill_spt, baseline_decode, baseline_prefill).is_nan()
            );
            prop_assert!(
                score_default_weights(decode_spt, bad, baseline_decode, baseline_prefill).is_nan()
            );
            // A non-positive baseline zeroes the speedup, which the score rejects as NaN.
            prop_assert!(
                score_default_weights(decode_spt, prefill_spt, bad, baseline_prefill).is_nan()
            );
        }
    }

    /// Guard: a negative or non-finite weight yields NaN. (Score.swift:36-42.)
    #[test]
    fn score_rejects_bad_weights(
        decode_spt in pos(),
        prefill_spt in pos(),
        baseline_decode in pos(),
        baseline_prefill in pos(),
    ) {
        for bad in [-0.1f64, f64::NAN, f64::INFINITY] {
            prop_assert!(
                score(decode_spt, prefill_spt, baseline_decode, baseline_prefill, bad, 0.25).is_nan()
            );
        }
        // Weights summing to zero is also rejected.
        prop_assert!(
            score(decode_spt, prefill_spt, baseline_decode, baseline_prefill, 0.0, 0.0).is_nan()
        );
    }

    // ---- speedup floors (Score.swift:50-64; ScoreTests.swift:29-34) ----

    /// Floors pass iff BOTH speedups clear their (finite) floor. (Score.swift:63.)
    #[test]
    fn floors_pass_iff_both_meet(ds in pos(), ps in pos()) {
        let expected = ds >= SCORE_DECODE_SPEEDUP_FLOOR && ps >= SCORE_PREFILL_SPEEDUP_FLOOR;
        prop_assert_eq!(
            passes_speedup_floors(ds, ps, SCORE_DECODE_SPEEDUP_FLOOR, SCORE_PREFILL_SPEEDUP_FLOOR),
            expected
        );
    }

    /// Boundary: exactly at the floor passes (>=, inclusive). (Score.swift:63.)
    #[test]
    fn floors_boundary_inclusive(headroom in 0.0f64..1.0f64) {
        let d = SCORE_DECODE_SPEEDUP_FLOOR + headroom;
        let p = SCORE_PREFILL_SPEEDUP_FLOOR + headroom;
        prop_assert!(passes_speedup_floors(
            d, p, SCORE_DECODE_SPEEDUP_FLOOR, SCORE_PREFILL_SPEEDUP_FLOOR
        ));
        prop_assert!(passes_speedup_floors(
            SCORE_DECODE_SPEEDUP_FLOOR, SCORE_PREFILL_SPEEDUP_FLOOR,
            SCORE_DECODE_SPEEDUP_FLOOR, SCORE_PREFILL_SPEEDUP_FLOOR
        ));
    }

    /// A non-finite speedup can never pass the floors. (Score.swift:55-60.)
    #[test]
    fn floors_nonfinite_never_pass(ok in pos()) {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            prop_assert!(!passes_speedup_floors(
                bad, ok, SCORE_DECODE_SPEEDUP_FLOOR, SCORE_PREFILL_SPEEDUP_FLOOR
            ));
            prop_assert!(!passes_speedup_floors(
                ok, bad, SCORE_DECODE_SPEEDUP_FLOOR, SCORE_PREFILL_SPEEDUP_FLOOR
            ));
        }
    }

    // ---- the candidate's fast-side check (the lower side of AcceptanceBand.swift:35-67) ----
    // The slow side is gone (David 2026-10-07): the speedup floor is the candidate's one slowness
    // gate, so the check refuses only a value below `B*(1-down)` while the lower bound is enabled.

    /// A finite positive measurement passes iff it is at or above `B*(1-down)`, inclusive, and any
    /// value passes when the lower bound is disabled. Recomputes the bound with the same float ops
    /// the production code uses, so the equivalence is exact.
    #[test]
    fn fast_side_passes_iff_at_or_above_the_lower_edge(
        reference in pos(),
        value in pos(),
        down in 0.0f64..0.9f64,
        enforce in proptest::bool::ANY,
    ) {
        let lo = reference * (1.0 - down);
        let expected = !enforce || value >= lo;
        prop_assert_eq!(check_fast_side(value, reference, down, enforce, "x").passed, expected);
    }

    /// No slow value fails: any multiple of the reference above 1 passes, on either setting.
    #[test]
    fn fast_side_never_refuses_a_slow_value(
        reference in pos(),
        factor in 1.0f64..1e6f64,
        down in 0.0f64..0.9f64,
        enforce in proptest::bool::ANY,
    ) {
        prop_assert!(check_fast_side(reference * factor, reference, down, enforce, "x").passed);
    }

    /// Strictly below the lower edge, the enabled check fails as too large a gain.
    #[test]
    fn fast_side_outside_fails(reference in pos(), tol in 1e-4f64..0.5f64, frac in 1.05f64..3.0f64) {
        let delta = frac * tol;
        if delta < 1.0 {
            let fast = check_fast_side(reference * (1.0 - delta), reference, tol, true, "x");
            prop_assert!(!fast.passed);
            prop_assert!(fast.reason.contains("chunk"));
        }
    }

    /// Non-finite / non-positive value or reference is always rejected.
    /// (AcceptanceBand.swift:43-48; AcceptanceBandTests.swift:100-105.)
    #[test]
    fn fast_side_rejects_nonfinite_or_nonpositive(ok in pos(), down in 0.0f64..0.5f64) {
        for bad in [0.0f64, -1.0, f64::NAN, f64::INFINITY] {
            for enforce in [true, false] {
                prop_assert!(!check_fast_side(bad, ok, down, enforce, "x").passed);
                prop_assert!(!check_fast_side(ok, bad, down, enforce, "x").passed);
            }
        }
    }
}
