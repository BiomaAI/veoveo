//! Requirement coverage is separate from owner declarations and runtime check outcomes.
use crate::{CheckResult, CheckStatus};
use veoveo_mcp_contract::docs::{CATALOG_REVISION, RequirementId};

/// Mapping changes evolve independently of hosted and requirement catalog revisions.
pub const COVERAGE_REVISION: u32 = 3;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementCoverage {
    pub coverage_revision: u32,
    pub catalog_revision: u32,
    pub requirements: Vec<RequirementVerification>,
}
pub fn requirement_coverage() -> RequirementCoverage {
    RequirementCoverage {
        coverage_revision: COVERAGE_REVISION,
        catalog_revision: CATALOG_REVISION,
        requirements: RequirementId::ALL
            .iter()
            .copied()
            .map(requirement_verification)
            .collect(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum VerificationMode {
    Runtime,
    Review,
    Mixed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum RequirementOutcome {
    Passed,
    Failed,
    Incomplete,
    ReviewRequired,
}

#[derive(serde::Serialize)]
pub struct RequirementVerification {
    pub id: RequirementId,
    pub mode: VerificationMode,
    pub checks: &'static [&'static str],
}
impl RequirementVerification {
    /// A declaration and a skipped/missing runtime check cannot establish success.
    pub fn assess(&self, results: &[CheckResult]) -> RequirementOutcome {
        if self.checks.iter().any(|id| {
            results
                .iter()
                .any(|result| result.requirement_id == *id && result.status == CheckStatus::Failed)
        }) {
            return RequirementOutcome::Failed;
        }
        if self.checks.iter().any(|id| {
            !results
                .iter()
                .any(|result| result.requirement_id == *id && result.status == CheckStatus::Passed)
        }) {
            return RequirementOutcome::Incomplete;
        }
        match self.mode {
            VerificationMode::Runtime => RequirementOutcome::Passed,
            VerificationMode::Review | VerificationMode::Mixed => {
                RequirementOutcome::ReviewRequired
            }
        }
    }
}

/// Existing VV-MCP and K check identities are not catalog RequirementIds.
pub fn requirement_verification(id: RequirementId) -> RequirementVerification {
    use RequirementId::*;
    use VerificationMode::*;
    let (mode, checks): (VerificationMode, &'static [&'static str]) = match id {
        C01 => (Review, &[]),
        C02 => (Mixed, &["VV-MCP-TOOLS-002", "VV-MCP-TOOLS-003"]),
        C03 => (Mixed, &["VV-MCP-TASKS-001"]),
        C04 => (Mixed, &["VV-MCP-RESOURCES-001", "VV-MCP-TEMPLATES-001"]),
        C05 | C06 => (Review, &[]),
        C07 => (Runtime, &["VV-MCP-TOOLS-003"]),
        C08 | C09 | C10 => (Review, &[]),
        C11 => (Mixed, &["VV-MCP-HTTP-001"]),
        C12 => (
            Mixed,
            &[
                "VV-MCP-DOCS-HTTP-000",
                "VV-MCP-DOCS-HTTP-001",
                "VV-MCP-DOCS-HTTP-002",
            ],
        ),
        C13 | C14 | C15 | C16 => (Review, &[]),
        C17 => (Mixed, &["VV-MCP-CONTRACT-001"]),
        C18 => (Runtime, &["VV-MCP-DOCS-001", "VV-MCP-DOCS-002"]),
        C19 => (Runtime, &["VV-MCP-CONTRACT-001", "VV-MCP-CONTRACT-002"]),
        C20 => (Runtime, &["VV-MCP-DOCS-HTTP-001", "VV-MCP-DOCS-HTTP-002"]),
        C21 => (Mixed, &["VV-MCP-DOCS-002"]),
        C22 | C23 | C24 => (Review, &[]),
        C25 | C26 => (
            Runtime,
            &[
                "VV-MCP-TRANSPORT-001",
                "VV-MCP-HTTP-001",
                "VV-MCP-HTTP-002",
                "VV-MCP-HTTP-003",
                "VV-MCP-HTTP-004",
            ],
        ),
        C27 => (Mixed, &["VV-MCP-SUBSCRIPTIONS-001"]),
        C28 => (Review, &[]),
        C29 => (Mixed, &["VV-MCP-TRANSPORT-001"]),
        C30 => (Review, &[]),
        C31 => (
            Mixed,
            &[
                "VV-MCP-IDENTITY-001",
                "VV-MCP-TOOLS-001",
                "VV-MCP-RESOURCES-001",
                "VV-MCP-TEMPLATES-001",
                "VV-MCP-PROMPTS-001",
            ],
        ),
        C33 => (Mixed, &["VV-MCP-NAMING-001"]),
        C32 => (
            Mixed,
            &[
                "VV-MCP-CONTRACT-003",
                "K01",
                "K02",
                "K03",
                "K04",
                "K05",
                "K06",
                "K07",
                "K08",
            ],
        ),
    };
    RequirementVerification { id, mode, checks }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn skipped_checks_and_review_declarations_do_not_create_success() {
        let mapping = requirement_verification(RequirementId::C07);
        let check = |status| CheckResult {
            requirement_id: "VV-MCP-TOOLS-003".into(),
            status,
            summary: "fixture".into(),
            evidence: None,
        };
        assert_eq!(mapping.assess(&[]), RequirementOutcome::Incomplete);
        assert_eq!(
            mapping.assess(&[check(CheckStatus::Skipped)]),
            RequirementOutcome::Incomplete
        );
        assert_eq!(
            mapping.assess(&[check(CheckStatus::Failed)]),
            RequirementOutcome::Failed
        );
        assert_eq!(
            mapping.assess(&[check(CheckStatus::Passed)]),
            RequirementOutcome::Passed
        );
        assert_eq!(
            requirement_verification(RequirementId::C09).assess(&[]),
            RequirementOutcome::ReviewRequired
        );
    }
    #[test]
    fn security_and_readiness_runtime_checks_still_require_policy_review() {
        for id in [RequirementId::C12, RequirementId::C31, RequirementId::C33] {
            let mapping = requirement_verification(id);
            assert_eq!(mapping.mode, VerificationMode::Mixed);
            let results: Vec<_> = mapping
                .checks
                .iter()
                .map(|id| CheckResult {
                    requirement_id: (*id).into(),
                    status: CheckStatus::Passed,
                    summary: "runtime probe passed".into(),
                    evidence: None,
                })
                .collect();
            assert_eq!(mapping.assess(&results), RequirementOutcome::ReviewRequired);
        }
    }

    #[test]
    fn knowledge_runtime_qualification_requires_owner_review_and_complete_probes() {
        let mapping = requirement_verification(RequirementId::C32);
        assert_eq!(mapping.mode, VerificationMode::Mixed);
        let check = |id: &str, status| CheckResult {
            requirement_id: id.into(),
            status,
            summary: "knowledge qualification probe".into(),
            evidence: None,
        };
        let passed: Vec<_> = mapping
            .checks
            .iter()
            .map(|id| check(id, CheckStatus::Passed))
            .collect();
        assert_eq!(mapping.assess(&passed), RequirementOutcome::ReviewRequired);

        for index in 0..passed.len() {
            let mut missing = passed.clone();
            missing.remove(index);
            assert_eq!(mapping.assess(&missing), RequirementOutcome::Incomplete);
            missing.extend([
                check("K09", CheckStatus::Passed),
                check("K10", CheckStatus::Passed),
            ]);
            assert_eq!(mapping.assess(&missing), RequirementOutcome::Incomplete);

            let mut skipped = passed.clone();
            skipped[index].status = CheckStatus::Skipped;
            assert_eq!(mapping.assess(&skipped), RequirementOutcome::Incomplete);

            let mut failed = passed.clone();
            failed[index].status = CheckStatus::Failed;
            assert_eq!(mapping.assess(&failed), RequirementOutcome::Failed);
        }
        let mut purported_review = passed;
        purported_review.extend([
            check("K09", CheckStatus::Passed),
            check("K10", CheckStatus::Passed),
        ]);
        assert_eq!(
            mapping.assess(&purported_review),
            RequirementOutcome::ReviewRequired
        );
    }
}
