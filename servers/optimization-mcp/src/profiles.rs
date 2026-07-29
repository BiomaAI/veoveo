use std::{num::NonZeroU32, sync::LazyLock};

use crate::{
    domain::{
        ConvexProblemKind, NonNegativeF64, OptimizationContractError, OptimizationProfileUri,
        ProblemFamily, SolverIntent, SolverPolicyRef, SolverProfile, SolverProfileDefaults,
        SolverProfileId, UnitInterval,
    },
    executor::{
        ConvexMethod, ConvexSolverSettings, ExecutorProfile, MilpSolverSettings,
        RoutingSolverSettings,
    },
};

pub const INTERACTIVE_PROFILE_URI: &str = "optimization://profile/interactive";
pub const BALANCED_PROFILE_URI: &str = "optimization://profile/balanced";
pub const THOROUGH_PROFILE_URI: &str = "optimization://profile/thorough";

static PROFILES: LazyLock<Vec<SolverProfile>> = LazyLock::new(|| {
    vec![
        profile(
            "interactive",
            "Interactive",
            "Low-latency exploration with a short solve budget and a five-percent MILP gap.",
            SolverIntent::Interactive,
            30,
            (5, 5, 10),
            1e-4,
            0.05,
            false,
        ),
        profile(
            "balanced",
            "Balanced",
            "Production default balancing solution quality, runtime, and incumbent retention.",
            SolverIntent::Balanced,
            300,
            (30, 30, 60),
            1e-6,
            0.01,
            true,
        ),
        profile(
            "thorough",
            "Thorough",
            "Long-running search for tighter mathematical bounds and higher-quality routes.",
            SolverIntent::Thorough,
            3_600,
            (300, 300, 300),
            1e-8,
            0.001,
            true,
        ),
    ]
});

pub fn profiles() -> &'static [SolverProfile] {
    &PROFILES
}

pub fn profile_by_uri(uri: &OptimizationProfileUri) -> Option<&'static SolverProfile> {
    PROFILES.iter().find(|profile| profile.profile_uri == *uri)
}

pub fn executor_profile(
    policy: &SolverPolicyRef,
    family: ProblemFamily,
    retain_incumbents: bool,
) -> Result<ExecutorProfile, OptimizationContractError> {
    let selected = profile_by_uri(&policy.profile_uri).ok_or_else(|| {
        OptimizationContractError::InvalidProblem(format!(
            "unknown solver profile {}",
            policy.profile_uri
        ))
    })?;
    if !selected.supported_families.contains(&family) {
        return Err(OptimizationContractError::InvalidProblem(format!(
            "solver profile {} does not support {family:?}",
            policy.profile_uri
        )));
    }
    if policy.quality_target.is_some() && family != ProblemFamily::Milp {
        return Err(OptimizationContractError::InvalidProblem(
            "quality_target is only valid for MILP solves".to_owned(),
        ));
    }
    let requested_deadline = policy.deadline_seconds.unwrap_or(match family {
        ProblemFamily::Routing | ProblemFamily::RouteScenarios => {
            selected.defaults.routing_deadline_seconds
        }
        ProblemFamily::Convex => selected.defaults.convex_deadline_seconds,
        ProblemFamily::Milp => selected.defaults.milp_deadline_seconds,
    });
    if requested_deadline > selected.maximum_deadline_seconds {
        return Err(OptimizationContractError::InvalidProblem(format!(
            "deadline {} exceeds profile maximum {} seconds",
            requested_deadline, selected.maximum_deadline_seconds
        )));
    }
    let quality = policy.quality_target.as_ref();
    let relative_gap = quality
        .and_then(|target| target.relative_gap)
        .unwrap_or(selected.defaults.milp_relative_gap);
    let absolute_gap = quality
        .and_then(|target| target.absolute_gap)
        .unwrap_or(selected.defaults.milp_absolute_gap);
    let deadline = NonNegativeF64::new(f64::from(requested_deadline.get()))
        .expect("positive deadline is non-negative");

    Ok(ExecutorProfile {
        name: selected.profile_id.to_string(),
        routing: RoutingSolverSettings {
            time_limit_seconds: deadline,
            verbose: false,
        },
        convex: ConvexSolverSettings {
            time_limit_seconds: deadline,
            method: ConvexMethod::Pdlp,
            optimality_tolerance: selected.defaults.convex_optimality_tolerance,
            presolve: true,
        },
        milp: MilpSolverSettings {
            time_limit_seconds: deadline,
            relative_gap: NonNegativeF64::new(relative_gap.get())
                .expect("unit interval is non-negative"),
            absolute_gap,
            integrality_tolerance: NonNegativeF64::new(1e-5)
                .expect("integrality tolerance is non-negative"),
            presolve: true,
            retain_incumbents: retain_incumbents || selected.defaults.retain_milp_incumbents,
        },
    })
}

pub fn convex_executor_profile(
    policy: &SolverPolicyRef,
    kind: ConvexProblemKind,
) -> Result<ExecutorProfile, OptimizationContractError> {
    let mut profile = executor_profile(policy, ProblemFamily::Convex, false)?;
    profile.convex.method = match kind {
        ConvexProblemKind::LinearProgram => ConvexMethod::Pdlp,
        ConvexProblemKind::QuadraticProgram
        | ConvexProblemKind::QuadraticallyConstrainedProgram
        | ConvexProblemKind::SecondOrderConeProgram => ConvexMethod::Barrier,
    };
    Ok(profile)
}

#[allow(clippy::too_many_arguments)]
fn profile(
    id: &str,
    title: &str,
    description: &str,
    intent: SolverIntent,
    maximum_deadline_seconds: u32,
    deadlines: (u32, u32, u32),
    convex_tolerance: f64,
    milp_relative_gap: f64,
    retain_milp_incumbents: bool,
) -> SolverProfile {
    let profile_id = SolverProfileId::new(id).expect("static profile id is valid");
    SolverProfile {
        profile_uri: OptimizationProfileUri::parse(format!("optimization://profile/{profile_id}"))
            .expect("static profile URI is valid"),
        profile_id,
        title: title.to_owned(),
        description: description.to_owned(),
        intent,
        supported_families: vec![
            ProblemFamily::Routing,
            ProblemFamily::RouteScenarios,
            ProblemFamily::Convex,
            ProblemFamily::Milp,
        ],
        maximum_deadline_seconds: NonZeroU32::new(maximum_deadline_seconds)
            .expect("static maximum deadline is positive"),
        defaults: SolverProfileDefaults {
            routing_deadline_seconds: NonZeroU32::new(deadlines.0)
                .expect("static routing deadline is positive"),
            convex_deadline_seconds: NonZeroU32::new(deadlines.1)
                .expect("static convex deadline is positive"),
            milp_deadline_seconds: NonZeroU32::new(deadlines.2)
                .expect("static MILP deadline is positive"),
            convex_optimality_tolerance: NonNegativeF64::new(convex_tolerance)
                .expect("static convex tolerance is non-negative"),
            milp_relative_gap: UnitInterval::new(milp_relative_gap)
                .expect("static relative gap is a unit interval"),
            milp_absolute_gap: NonNegativeF64::new(0.0).expect("zero absolute gap is non-negative"),
            retain_milp_incumbents,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_has_a_unique_uri() {
        let mut uris = profiles()
            .iter()
            .map(|profile| profile.profile_uri.as_str())
            .collect::<Vec<_>>();
        uris.sort_unstable();
        uris.dedup();
        assert_eq!(uris.len(), profiles().len());
    }

    #[test]
    fn policy_deadline_cannot_escape_the_profile_bound() {
        let policy = SolverPolicyRef {
            profile_uri: OptimizationProfileUri::parse(INTERACTIVE_PROFILE_URI).unwrap(),
            deadline_seconds: NonZeroU32::new(31),
            quality_target: None,
        };
        assert!(executor_profile(&policy, ProblemFamily::Routing, false).is_err());
    }

    #[test]
    fn quadratic_convex_forms_select_the_required_barrier_method() {
        let policy = SolverPolicyRef {
            profile_uri: OptimizationProfileUri::parse(BALANCED_PROFILE_URI).unwrap(),
            deadline_seconds: None,
            quality_target: None,
        };
        assert_eq!(
            convex_executor_profile(&policy, ConvexProblemKind::LinearProgram)
                .unwrap()
                .convex
                .method,
            ConvexMethod::Pdlp
        );
        for kind in [
            ConvexProblemKind::QuadraticProgram,
            ConvexProblemKind::QuadraticallyConstrainedProgram,
            ConvexProblemKind::SecondOrderConeProgram,
        ] {
            assert_eq!(
                convex_executor_profile(&policy, kind)
                    .unwrap()
                    .convex
                    .method,
                ConvexMethod::Barrier
            );
        }
    }
}
