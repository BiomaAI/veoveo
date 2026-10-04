//! Interval-set operations over checked bounds, preserving endpoint metadata.
use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use rangemap::RangeSet;

use crate::contract::{
    AuthorityBinding, EvaluateWindowsOutput, EvaluateWindowsRequest, TimeInstant, TimeWindow,
    WindowOperation,
};

pub(super) fn evaluate(
    authority: &AuthorityBinding,
    request: &EvaluateWindowsRequest,
) -> Result<EvaluateWindowsOutput> {
    let mut endpoints: BTreeMap<i128, &TimeInstant> = BTreeMap::new();
    for window in request.left.iter().chain(&request.right) {
        if window.authority() != authority {
            bail!("window operation references a non-active temporal authority");
        }
        for instant in [window.start(), window.end()] {
            endpoints
                .entry(instant.total_nanoseconds())
                .and_modify(|existing| {
                    if instant.uncertainty_nanoseconds > existing.uncertainty_nanoseconds {
                        *existing = instant;
                    }
                })
                .or_insert(instant);
        }
    }
    let left = window_set(&request.left);
    let right = window_set(&request.right);
    let ranges: RangeSet<i128> = match request.operation {
        WindowOperation::Union => left.union(&right).collect(),
        WindowOperation::Intersection => left.intersection(&right).collect(),
        WindowOperation::Difference => {
            let mut difference = left;
            for range in right.iter() {
                difference.remove(range.clone());
            }
            difference
        }
    };
    let endpoint = |coordinate: &i128| {
        endpoints
            .get(coordinate)
            .map(|instant| (*instant).clone())
            .context("interval algebra returned a coordinate absent from the input bounds")
    };
    Ok(EvaluateWindowsOutput {
        windows: ranges
            .iter()
            .map(|range| {
                Ok(TimeWindow::new(
                    endpoint(&range.start)?,
                    endpoint(&range.end)?,
                )?)
            })
            .collect::<Result<_>>()?,
    })
}

fn window_set(windows: &[TimeWindow]) -> RangeSet<i128> {
    windows
        .iter()
        .map(|window| window.start().total_nanoseconds()..window.end().total_nanoseconds())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AuthorityReleaseId;

    fn authority() -> AuthorityBinding {
        AuthorityBinding::new(
            AuthorityReleaseId::parse("time-release-window-tzdb").unwrap(),
            AuthorityReleaseId::parse("time-release-window-leaps").unwrap(),
        )
        .unwrap()
    }

    fn window(start: i128, end: i128, uncertainty: u64) -> TimeWindow {
        let instant = |coordinate| {
            TimeInstant::from_total_nanoseconds(coordinate, uncertainty, authority()).unwrap()
        };
        TimeWindow::new(instant(start), instant(end)).unwrap()
    }

    #[test]
    fn algebra_matches_half_open_membership_and_keeps_endpoint_uncertainty() {
        let mut cases = vec![vec![]];
        for start in -2..=1 {
            for end in (start + 1)..=2 {
                cases.push(vec![window(start, end, (end - start) as u64)]);
            }
        }
        cases.extend([
            vec![window(-2, 0, 7), window(0, 2, 11)],
            vec![window(-2, -1, 13), window(1, 2, 17)],
            vec![window(-2, 1, 19), window(-1, 2, 23)],
            vec![window(-2, 2, 29), window(-2, 2, 31)],
        ]);
        let contains = |windows: &[TimeWindow], point| {
            windows.iter().any(|window| {
                window.start().total_nanoseconds() <= point
                    && point < window.end().total_nanoseconds()
            })
        };
        for left in &cases {
            for right in &cases {
                for operation in [
                    WindowOperation::Union,
                    WindowOperation::Intersection,
                    WindowOperation::Difference,
                ] {
                    let output = evaluate(
                        &authority(),
                        &EvaluateWindowsRequest {
                            operation: operation.clone(),
                            left: left.clone(),
                            right: right.clone(),
                        },
                    )
                    .unwrap();
                    for point in -3..=3 {
                        let l = contains(left, point);
                        let r = contains(right, point);
                        let expected = match operation {
                            WindowOperation::Union => l || r,
                            WindowOperation::Intersection => l && r,
                            WindowOperation::Difference => l && !r,
                        };
                        assert_eq!(contains(&output.windows, point), expected);
                    }
                    for pair in output.windows.windows(2) {
                        assert!(
                            pair[0].end().total_nanoseconds() < pair[1].start().total_nanoseconds()
                        );
                    }
                    for bound in output.windows.iter().flat_map(|w| [w.start(), w.end()]) {
                        let expected_uncertainty = left
                            .iter()
                            .chain(right)
                            .flat_map(|w| [w.start(), w.end()])
                            .filter(|input| input.total_nanoseconds() == bound.total_nanoseconds())
                            .map(|input| input.uncertainty_nanoseconds)
                            .max()
                            .unwrap();
                        assert_eq!(bound.uncertainty_nanoseconds, expected_uncertainty);
                        assert_eq!(bound.authority, authority());
                    }
                }
            }
        }
    }

    #[test]
    fn difference_preserves_cut_bounds_across_the_full_seconds_range() {
        let minimum = i128::from(i64::MIN) * 1_000_000_000;
        let maximum = i128::from(i64::MAX) * 1_000_000_000 + 999_999_999;
        let whole = window(minimum, maximum, 7);
        let cut = window(-1, 1, 11);
        let output = evaluate(
            &authority(),
            &EvaluateWindowsRequest {
                operation: WindowOperation::Difference,
                left: vec![whole.clone()],
                right: vec![cut.clone()],
            },
        )
        .unwrap();
        assert_eq!(
            output.windows,
            vec![
                TimeWindow::new(whole.start().clone(), cut.start().clone()).unwrap(),
                TimeWindow::new(cut.end().clone(), whole.end().clone()).unwrap(),
            ]
        );
    }

    #[test]
    fn inactive_authority_is_rejected_on_either_side_even_without_overlap() {
        let mut start = window(10, 11, 0).start().clone();
        start.authority = AuthorityBinding::new(
            AuthorityReleaseId::parse("time-release-foreign-tzdb").unwrap(),
            AuthorityReleaseId::parse("time-release-foreign-leaps").unwrap(),
        )
        .unwrap();
        let mut end = start.clone();
        end.tai_seconds_since_1970 += 1;
        let foreign = TimeWindow::new(start, end).unwrap();
        for operation in [
            WindowOperation::Union,
            WindowOperation::Intersection,
            WindowOperation::Difference,
        ] {
            for (left, right) in [
                (vec![foreign.clone()], vec![]),
                (vec![window(-2, 2, 0)], vec![foreign.clone()]),
            ] {
                let error = evaluate(
                    &authority(),
                    &EvaluateWindowsRequest {
                        operation: operation.clone(),
                        left,
                        right,
                    },
                )
                .unwrap_err();
                assert_eq!(
                    error.to_string(),
                    "window operation references a non-active temporal authority"
                );
            }
        }
    }
}
