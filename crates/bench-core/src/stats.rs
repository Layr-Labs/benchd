//! The ONE implementation of every aggregate benchd takes over measured figures.
//!
//! * [`mean`] — the arithmetic mean: the `mean` pair rule, the calibration means, the measure-job
//!   per-prompt and pooled means.
//! * [`lower_median_index`] / [`lower_median`] — the `lower_median` pair rule and the measure-job
//!   per-pair diagnostic: the order statistic at `(n - 1) / 2`, ties in input order.
//! * [`even_n_median`] — the median of the timed prefill passes and the decode-only track's
//!   published median of the per-prompt ratios.
//!
//! None of them makes a paired / candidate-vs-baseline assumption.

/// The EVEN-N median of a slice: for an odd count the middle order statistic, for an even count the
/// mean of the two central order statistics. Returns `NaN` for an empty slice (the caller guards
/// non-empty). NaN samples sort last; a materialised copy is sorted, so the input is untouched.
///
/// This is the `even_n_mean_of_two_central_order_statistics` rule (NOT the lower-median rule a p50
/// diagnostic uses).
pub fn even_n_median(samples: &[f64]) -> f64 {
    let n = samples.len();
    if n == 0 {
        return f64::NAN;
    }
    let mut sorted = samples.to_vec();
    // Total order over f64 for the order statistics; a positive NaN sorts last.
    sorted.sort_by(|a, b| a.total_cmp(b));
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

/// The ARITHMETIC MEAN of a slice: the sum in input order over the count. Returns `NaN` for an
/// empty slice (the caller guards non-empty).
pub fn mean(samples: &[f64]) -> f64 {
    samples.iter().sum::<f64>() / samples.len() as f64
}

/// The INDEX of the LOWER MEDIAN of a slice: the order statistic at `(n - 1) / 2` of the values in
/// [`f64::total_cmp`] order, which on an even count is the lower of the two central values, never
/// their mean. The sort is stable, so equal values keep input order and the earlier one is chosen.
/// `None` for an empty slice.
pub fn lower_median_index(samples: &[f64]) -> Option<usize> {
    if samples.is_empty() {
        return None;
    }
    let mut order: Vec<usize> = (0..samples.len()).collect();
    order.sort_by(|&a, &b| samples[a].total_cmp(&samples[b]));
    Some(order[(samples.len() - 1) / 2])
}

/// The LOWER MEDIAN of a slice ([`lower_median_index`]). Returns `NaN` for an empty slice.
pub fn lower_median(samples: &[f64]) -> f64 {
    lower_median_index(samples).map_or(f64::NAN, |i| samples[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn even_n_median_odd_is_middle_even_is_mean_of_two_central() {
        // Odd n → middle order statistic.
        assert_eq!(even_n_median(&[0.9, 1.1, 1.0]), 1.0);
        // Even n → mean of the two central order statistics (NOT lower-median).
        assert_eq!(even_n_median(&[1.0, 2.0, 3.0, 4.0]), 2.5);
        // Single sample → that one value.
        assert_eq!(even_n_median(&[1.234]), 1.234);
        // Unsorted input is ordered first.
        assert_eq!(even_n_median(&[4.0, 1.0, 3.0, 2.0]), 2.5);
    }

    #[test]
    fn lower_median_is_the_lower_central_value_and_keeps_input_order_on_ties() {
        assert_eq!(lower_median(&[1.3, 1.1, 1.2]), 1.2);
        assert_eq!(lower_median(&[1.4, 1.1, 1.3, 1.2]), 1.2);
        assert_eq!(lower_median_index(&[1.2, 1.2]), Some(0));
        assert_eq!(lower_median_index(&[]), None);
        assert!(lower_median(&[]).is_nan());
    }

    #[test]
    fn mean_is_the_sum_over_the_count() {
        assert_eq!(mean(&[1.0, 2.0, 4.5]), 2.5);
        assert!(mean(&[]).is_nan());
    }

    #[test]
    fn even_n_median_empty_is_nan() {
        assert!(even_n_median(&[]).is_nan());
    }
}
