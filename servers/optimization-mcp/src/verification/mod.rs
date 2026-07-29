//! Independent feasibility checks applied before cuOpt output is published.

mod mathematical;
mod routing;

pub use mathematical::{CandidateVerification, verify_convex_candidate, verify_milp_candidate};
pub use routing::{RoutingVerification, verify_routing_solution};

use crate::domain::{NonNegativeF64, VerificationFinding, VerificationReport};

pub const DEFAULT_ABSOLUTE_TOLERANCE: f64 = 1e-6;
pub const DEFAULT_RELATIVE_TOLERANCE: f64 = 1e-6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VerificationTolerance {
    pub absolute: f64,
    pub relative: f64,
}

impl VerificationTolerance {
    pub fn new(absolute: f64, relative: f64) -> Self {
        Self { absolute, relative }
    }

    pub fn allowed(self, scale: f64) -> f64 {
        self.absolute + self.relative * scale.abs().max(1.0)
    }
}

impl Default for VerificationTolerance {
    fn default() -> Self {
        Self {
            absolute: DEFAULT_ABSOLUTE_TOLERANCE,
            relative: DEFAULT_RELATIVE_TOLERANCE,
        }
    }
}

pub(crate) fn report(
    findings: Vec<VerificationFinding>,
    tolerance: VerificationTolerance,
    maximum_constraint_violation: Option<f64>,
    maximum_integrality_violation: Option<f64>,
    maximum_bound_violation: Option<f64>,
) -> VerificationReport {
    let verified = !findings
        .iter()
        .any(|finding| finding.severity == crate::domain::VerificationSeverity::Error);
    VerificationReport {
        verification_id: crate::domain::VerificationId::new(),
        verified,
        findings,
        absolute_tolerance: non_negative(tolerance.absolute),
        relative_tolerance: non_negative(tolerance.relative),
        maximum_constraint_violation: maximum_constraint_violation.map(non_negative),
        maximum_integrality_violation: maximum_integrality_violation.map(non_negative),
        maximum_bound_violation: maximum_bound_violation.map(non_negative),
        verified_at: chrono::Utc::now(),
    }
}

pub(crate) fn non_negative(value: f64) -> NonNegativeF64 {
    NonNegativeF64::new(value.max(0.0)).expect("verification metrics are finite and non-negative")
}
