use std::collections::BTreeSet;
use veoveo_gateway_contract::{GatewayAction, PolicyAction};

use anyhow::Result;
use veoveo_mcp_contract::{
    GatewayProfile, GatewayProfileId, McpMethodName, PolicyDecision, PolicyEffect,
    PolicyReasonCode, PolicyRule, PolicyRuleId, PolicyTarget, Principal, TraceId,
};
use veoveo_types::{PolicyVersion, ResourceScheme, ScopeName};

use crate::{
    PolicyCatalogView,
    resource_policy::{ResourcePolicyReference, profile_allows_resource},
};

#[derive(Debug, Clone)]
pub struct PolicyRequest<'a> {
    pub principal: &'a Principal,
    pub profile: &'a GatewayProfileId,
    pub action: PolicyAction,
    pub target: &'a PolicyTarget,
    pub trace_id: &'a TraceId,
}

pub fn mcp_method_name(action: GatewayAction) -> Result<McpMethodName> {
    let Some(method) = action.mcp_method() else {
        anyhow::bail!("gateway action {action:?} does not map to one MCP method")
    };
    Ok(McpMethodName::new(method)?)
}

pub fn resource_scheme_from_uri(uri: &str) -> Option<ResourceScheme> {
    resource_scheme(uri)
}

pub(crate) fn resource_scheme(uri: &str) -> Option<ResourceScheme> {
    let (scheme, _) = uri.split_once("://")?;
    ResourceScheme::new(scheme).ok()
}

pub fn exposure_contains<T: PartialEq>(
    exposure: &veoveo_mcp_contract::Exposure<T>,
    item: &T,
) -> bool {
    match exposure {
        veoveo_mcp_contract::Exposure::All => true,
        veoveo_mcp_contract::Exposure::Listed(items) => items.iter().any(|allowed| allowed == item),
        veoveo_mcp_contract::Exposure::None => false,
    }
}

pub fn decide(catalog: &impl PolicyCatalogView, request: PolicyRequest<'_>) -> PolicyDecision {
    if validate_runtime_action(catalog, &request.action, request.target).is_err() {
        return deny(
            &request,
            PolicyReasonCode::PolicyDeny,
            request.target.clone(),
            None,
        );
    }
    let Some(profile) = catalog.profile(request.profile) else {
        return deny(
            &request,
            PolicyReasonCode::UnknownProfile,
            PolicyTarget::Gateway,
            None,
        );
    };

    let Some(policy) = catalog.policy(&profile.policy_version) else {
        return deny(
            &request,
            PolicyReasonCode::PolicyDeny,
            request.target.clone(),
            None,
        );
    };

    if let Err(reason) = validate_principal(catalog, request.principal) {
        return deny(
            &request,
            reason,
            request.target.clone(),
            Some(policy.version.clone()),
        );
    }

    if let Err(reason) = profile_allows_target(catalog, profile, &request.action, request.target) {
        return deny(
            &request,
            reason,
            request.target.clone(),
            Some(policy.version.clone()),
        );
    }

    if !has_required_scopes(&request.principal.scopes, &profile.required_scopes) {
        return deny(
            &request,
            PolicyReasonCode::MissingScope,
            request.target.clone(),
            Some(policy.version.clone()),
        );
    }

    let outcome = evaluate_rules(
        profile,
        policy,
        request.principal,
        &request.action.name(),
        |rule| matches_target_filters(rule, request.target),
    );
    decision(
        &request,
        outcome.effect,
        outcome.reason,
        request.target.clone(),
        Some(policy.version.clone()),
        outcome.rule_id,
    )
}

pub(crate) fn validate_principal(
    catalog: &impl PolicyCatalogView,
    principal: &Principal,
) -> Result<(), PolicyReasonCode> {
    if principal
        .data_labels
        .iter()
        .any(|label| catalog.data_label(label).is_none())
    {
        return Err(PolicyReasonCode::UnknownDataLabel);
    }
    if principal
        .tenant
        .as_ref()
        .is_some_and(|tenant| catalog.tenant(tenant).is_none())
    {
        return Err(PolicyReasonCode::UnknownTenant);
    }
    Ok(())
}

pub struct RuleOutcome {
    pub effect: PolicyEffect,
    pub reason: PolicyReasonCode,
    pub rule_id: Option<PolicyRuleId>,
}

/// One rule evaluator for concrete requests and resource-family admission.
/// The latter defers only lexical profile selection to the database.
pub(crate) fn evaluate_rules(
    profile: &GatewayProfile,
    policy: &veoveo_mcp_contract::PolicySet,
    principal: &Principal,
    action: &veoveo_types::ActionName,
    target_matches: impl Fn(&PolicyRule) -> bool,
) -> RuleOutcome {
    let detail = |rule: &PolicyRule| {
        rule_match_detail(rule, profile, principal, action, target_matches(rule))
    };
    assemble_rule_outcome(policy, detail)
}

/// Deny precedence and missing-condition diagnostics shared by every owner adapter.
pub fn assemble_rule_outcome(
    policy: &veoveo_mcp_contract::PolicySet,
    detail: impl Fn(&PolicyRule) -> RuleMatchDetail,
) -> RuleOutcome {
    if let Some(rule) = policy
        .rules
        .iter()
        .find(|rule| rule.effect == PolicyEffect::Deny && detail(rule) == RuleMatchDetail::Match)
    {
        return RuleOutcome {
            effect: PolicyEffect::Deny,
            reason: PolicyReasonCode::PolicyDeny,
            rule_id: Some(rule.id.clone()),
        };
    }
    let mut missing = None;
    for rule in &policy.rules {
        if rule.effect != PolicyEffect::Allow {
            continue;
        }
        let reason = match detail(rule) {
            RuleMatchDetail::Match => {
                return RuleOutcome {
                    effect: PolicyEffect::Allow,
                    reason: PolicyReasonCode::PolicyAllow,
                    rule_id: Some(rule.id.clone()),
                };
            }
            RuleMatchDetail::MissingDataLabel => PolicyReasonCode::MissingDataLabel,
            RuleMatchDetail::MissingPrincipalAssurance => {
                PolicyReasonCode::MissingPrincipalAssurance
            }
            RuleMatchDetail::MissingRole => PolicyReasonCode::MissingRole,
            RuleMatchDetail::MissingGroup => PolicyReasonCode::MissingGroup,
            RuleMatchDetail::MissingTenant => PolicyReasonCode::MissingTenant,
            RuleMatchDetail::MissingPrincipal => PolicyReasonCode::MissingPrincipal,
            RuleMatchDetail::MissingScope => PolicyReasonCode::MissingScope,
            RuleMatchDetail::NoMatch => continue,
        };
        remember_strongest_missing_requirement(&mut missing, reason, rule.id.clone());
    }
    let (reason, rule_id) = match missing {
        Some((reason, id)) => (reason, Some(id)),
        None => (PolicyReasonCode::PolicyDeny, None),
    };
    RuleOutcome {
        effect: PolicyEffect::Deny,
        reason,
        rule_id,
    }
}

fn profile_allows_target(
    catalog: &impl PolicyCatalogView,
    profile: &GatewayProfile,
    action: &PolicyAction,
    target: &PolicyTarget,
) -> Result<(), PolicyReasonCode> {
    match target {
        PolicyTarget::Gateway => Ok(()),
        PolicyTarget::Server { server } => {
            if catalog.server(server).is_none() {
                return Err(PolicyReasonCode::UnknownServer);
            }
            profile
                .servers
                .iter()
                .any(|exposure| &exposure.server == server)
                .then_some(())
                .ok_or(PolicyReasonCode::PolicyDeny)
        }
        PolicyTarget::Tool { server, tool } => {
            let manifest = catalog
                .server(server)
                .ok_or(PolicyReasonCode::UnknownServer)?;
            if !manifest.tools.is_empty() && !manifest.tools.iter().any(|known| known == tool) {
                return Err(PolicyReasonCode::UnknownTool);
            }
            let exposure = profile
                .servers
                .iter()
                .find(|exposure| &exposure.server == server)
                .ok_or(PolicyReasonCode::PolicyDeny)?;
            if exposure_contains(&exposure.tools, tool) {
                Ok(())
            } else {
                Err(PolicyReasonCode::PolicyDeny)
            }
        }
        PolicyTarget::Resource { server, uri }
        | PolicyTarget::Artifact {
            server,
            artifact_uri: uri,
        }
        | PolicyTarget::Usage {
            server,
            usage_uri: uri,
        } => {
            if matches!(
                action.kernel(),
                Some(GatewayAction::CompletionComplete | GatewayAction::ResourcesTemplatesList)
            ) {
                return Err(PolicyReasonCode::PolicyDeny);
            }
            profile_allows_resource(
                catalog,
                profile,
                server,
                ResourcePolicyReference::Concrete(uri),
            )
        }
        PolicyTarget::ResourceTemplate { server, uri } => {
            if !matches!(
                action.kernel(),
                Some(GatewayAction::CompletionComplete | GatewayAction::ResourcesTemplatesList)
            ) {
                return Err(PolicyReasonCode::PolicyDeny);
            }
            profile_allows_resource(
                catalog,
                profile,
                server,
                ResourcePolicyReference::Template(uri),
            )
        }
        PolicyTarget::Prompt { server, prompt } => {
            let manifest = catalog
                .server(server)
                .ok_or(PolicyReasonCode::UnknownServer)?;
            if !manifest.prompts.is_empty() && !manifest.prompts.iter().any(|p| p == prompt) {
                return Err(PolicyReasonCode::UnknownPrompt);
            }
            let exposure = profile
                .servers
                .iter()
                .find(|exposure| &exposure.server == server)
                .ok_or(PolicyReasonCode::PolicyDeny)?;
            if exposure_contains(&exposure.prompts, prompt) {
                Ok(())
            } else {
                Err(PolicyReasonCode::PolicyDeny)
            }
        }
        PolicyTarget::Task { server, .. } | PolicyTarget::PlatformTask { server, .. } => {
            let _manifest = catalog
                .server(server)
                .ok_or(PolicyReasonCode::UnknownServer)?;
            let exposure = profile
                .servers
                .iter()
                .find(|exposure| &exposure.server == server)
                .ok_or(PolicyReasonCode::PolicyDeny)?;
            if exposure.tasks == veoveo_mcp_contract::TaskExposure::Enabled
                || matches!(
                    action.kernel(),
                    Some(GatewayAction::ResourcesList | GatewayAction::ResourcesTemplatesList)
                )
            {
                Ok(())
            } else {
                Err(PolicyReasonCode::PolicyDeny)
            }
        }
        PolicyTarget::Owner(_) | PolicyTarget::Unadmitted(_) => Err(PolicyReasonCode::PolicyDeny),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleMatchDetail {
    Match,
    MissingPrincipal,
    MissingTenant,
    MissingGroup,
    MissingRole,
    MissingScope,
    MissingDataLabel,
    MissingPrincipalAssurance,
    NoMatch,
}

fn deny(
    request: &PolicyRequest<'_>,
    reason: PolicyReasonCode,
    target: PolicyTarget,
    policy_version: Option<PolicyVersion>,
) -> PolicyDecision {
    decision(
        request,
        PolicyEffect::Deny,
        reason,
        target,
        policy_version,
        None,
    )
}

fn decision(
    request: &PolicyRequest<'_>,
    effect: PolicyEffect,
    reason: PolicyReasonCode,
    target: PolicyTarget,
    policy_version: Option<PolicyVersion>,
    rule_id: Option<veoveo_mcp_contract::PolicyRuleId>,
) -> PolicyDecision {
    PolicyDecision {
        effect,
        reason,
        evaluated_at: chrono::Utc::now(),
        profile: request.profile.clone(),
        action: request.action.name(),
        target,
        principal: Some(request.principal.id.clone()),
        tenant: request.principal.tenant.clone(),
        policy_version,
        rule_id,
        trace_id: request.trace_id.clone(),
    }
}

fn rule_match_detail(
    rule: &PolicyRule,
    profile: &GatewayProfile,
    principal: &Principal,
    action: &veoveo_types::ActionName,
    target_matches: bool,
) -> RuleMatchDetail {
    if !rule.actions.contains(action) {
        return RuleMatchDetail::NoMatch;
    }
    if !rule.profiles.is_empty() && !rule.profiles.contains(&profile.id) {
        return RuleMatchDetail::NoMatch;
    }
    if !target_matches {
        return RuleMatchDetail::NoMatch;
    }
    principal_rule_conditions(rule, principal, &principal.data_labels)
}

/// Labels are supplied by the owning authority (principal labels or producer labels).
pub fn principal_rule_conditions(
    rule: &PolicyRule,
    principal: &Principal,
    labels: &BTreeSet<veoveo_types::DataLabelId>,
) -> RuleMatchDetail {
    let mut strongest_missing_requirement = RuleMatchDetail::Match;
    if !rule.principal_ids.is_empty() && !rule.principal_ids.contains(&principal.id) {
        strongest_missing_requirement = strongest_missing_rule_detail(
            strongest_missing_requirement,
            RuleMatchDetail::MissingPrincipal,
        );
    }
    if !rule.tenant_ids.is_empty() {
        match &principal.tenant {
            Some(tenant) if rule.tenant_ids.contains(tenant) => {}
            _ => {
                strongest_missing_requirement = strongest_missing_rule_detail(
                    strongest_missing_requirement,
                    RuleMatchDetail::MissingTenant,
                );
            }
        }
    }
    if !rule.groups.is_empty() && !intersects(&rule.groups, &principal.groups) {
        strongest_missing_requirement = strongest_missing_rule_detail(
            strongest_missing_requirement,
            RuleMatchDetail::MissingGroup,
        );
    }
    if !rule.roles.is_empty() && !intersects(&rule.roles, &principal.roles) {
        strongest_missing_requirement = strongest_missing_rule_detail(
            strongest_missing_requirement,
            RuleMatchDetail::MissingRole,
        );
    }
    if !rule.required_scopes.is_empty() && !rule.required_scopes.is_subset(&principal.scopes) {
        strongest_missing_requirement = strongest_missing_rule_detail(
            strongest_missing_requirement,
            RuleMatchDetail::MissingScope,
        );
    }
    if !rule.required_data_labels.is_empty() && !rule.required_data_labels.is_subset(labels) {
        strongest_missing_requirement = strongest_missing_rule_detail(
            strongest_missing_requirement,
            RuleMatchDetail::MissingDataLabel,
        );
    }
    if !rule.required_assurances.is_empty()
        && !rule.required_assurances.is_subset(&principal.assurances)
    {
        strongest_missing_requirement = strongest_missing_rule_detail(
            strongest_missing_requirement,
            RuleMatchDetail::MissingPrincipalAssurance,
        );
    }
    strongest_missing_requirement
}

pub fn strongest_missing_rule_detail(
    left: RuleMatchDetail,
    right: RuleMatchDetail,
) -> RuleMatchDetail {
    if rule_detail_rank(right) > rule_detail_rank(left) {
        right
    } else {
        left
    }
}

fn rule_detail_rank(detail: RuleMatchDetail) -> u8 {
    match detail {
        RuleMatchDetail::MissingDataLabel => 70,
        RuleMatchDetail::MissingPrincipalAssurance => 60,
        RuleMatchDetail::MissingRole => 50,
        RuleMatchDetail::MissingGroup => 40,
        RuleMatchDetail::MissingTenant => 30,
        RuleMatchDetail::MissingPrincipal => 20,
        RuleMatchDetail::MissingScope => 10,
        RuleMatchDetail::Match | RuleMatchDetail::NoMatch => 0,
    }
}

pub fn remember_strongest_missing_requirement(
    current: &mut Option<(PolicyReasonCode, PolicyRuleId)>,
    reason: PolicyReasonCode,
    rule_id: PolicyRuleId,
) {
    let replace = current
        .as_ref()
        .map(|(current_reason, _)| {
            missing_requirement_rank(reason) > missing_requirement_rank(*current_reason)
        })
        .unwrap_or(true);
    if replace {
        *current = Some((reason, rule_id));
    }
}

fn missing_requirement_rank(reason: PolicyReasonCode) -> u8 {
    match reason {
        PolicyReasonCode::MissingDataLabel => 70,
        PolicyReasonCode::MissingPrincipalAssurance => 60,
        PolicyReasonCode::MissingRole => 50,
        PolicyReasonCode::MissingGroup => 40,
        PolicyReasonCode::MissingTenant => 30,
        PolicyReasonCode::MissingPrincipal => 20,
        PolicyReasonCode::MissingScope => 10,
        _ => 0,
    }
}

fn matches_target_filters(rule: &PolicyRule, target: &PolicyTarget) -> bool {
    match target {
        PolicyTarget::Gateway => {
            rule.servers.is_empty()
                && rule.tools.is_empty()
                && rule.resource_schemes.is_empty()
                && rule.prompts.is_empty()
        }
        PolicyTarget::Server { server } => {
            filter_matches(&rule.servers, server)
                && rule.tools.is_empty()
                && rule.resource_schemes.is_empty()
                && rule.prompts.is_empty()
        }
        PolicyTarget::Tool { server, tool } => {
            filter_matches(&rule.servers, server) && filter_matches(&rule.tools, tool)
        }
        PolicyTarget::Resource { server, uri }
        | PolicyTarget::Artifact {
            server,
            artifact_uri: uri,
        }
        | PolicyTarget::Usage {
            server,
            usage_uri: uri,
        } => {
            let Some(scheme) = resource_scheme(uri.as_str()) else {
                return false;
            };
            filter_matches(&rule.servers, server) && filter_matches(&rule.resource_schemes, &scheme)
        }
        PolicyTarget::ResourceTemplate { server, uri } => {
            let Some(scheme) = resource_scheme(uri.as_str()) else {
                return false;
            };
            filter_matches(&rule.servers, server) && filter_matches(&rule.resource_schemes, &scheme)
        }
        PolicyTarget::Prompt { server, prompt } => {
            filter_matches(&rule.servers, server) && filter_matches(&rule.prompts, prompt)
        }
        PolicyTarget::Task { server, .. } | PolicyTarget::PlatformTask { server, .. } => {
            filter_matches(&rule.servers, server)
        }
        PolicyTarget::Owner(_) | PolicyTarget::Unadmitted(_) => false,
    }
}

pub(crate) fn has_required_scopes(
    principal_scopes: &BTreeSet<veoveo_types::ScopeName>,
    required: &[ScopeName],
) -> bool {
    required
        .iter()
        .all(|scope| principal_scopes.contains(scope))
}

pub(crate) fn filter_matches<T: Ord>(filter: &BTreeSet<T>, value: &T) -> bool {
    filter.is_empty() || filter.contains(value)
}

pub fn intersects<T: Ord>(left: &BTreeSet<T>, right: &BTreeSet<T>) -> bool {
    left.iter().any(|value| right.contains(value))
}

fn validate_runtime_action(
    catalog: &impl PolicyCatalogView,
    action: &PolicyAction,
    target: &PolicyTarget,
) -> Result<(), veoveo_types::ExtensionError> {
    match action {
        PolicyAction::Kernel(action) => {
            catalog
                .registry()
                .action_key::<GatewayAction>()?
                .action(*action)?;
        }
        PolicyAction::Registered(handle) => {
            catalog.registry().check_action(handle)?;
            let descriptor = catalog
                .registry()
                .descriptor(handle.name())
                .ok_or_else(|| {
                    veoveo_types::ExtensionError::new("owner action descriptor is unbound")
                })?;
            if !descriptor
                .target_kinds
                .iter()
                .any(|kind| kind.as_str() == target.kind())
            {
                return Err(veoveo_types::ExtensionError::new(
                    "action does not support this target",
                ));
            }
            if let Some(requirement) = &descriptor.server {
                let server = target
                    .server()
                    .and_then(|server| catalog.server(server))
                    .ok_or_else(|| {
                        veoveo_types::ExtensionError::new("action requires a known server target")
                    })?;
                if requirement
                    .slug
                    .as_ref()
                    .is_some_and(|slug| slug != &server.slug)
                    || (requirement.resources && !server.capabilities.resources)
                {
                    return Err(veoveo_types::ExtensionError::new(
                        "action target does not meet server requirements",
                    ));
                }
            }
        }
    }
    match target {
        PolicyTarget::Owner(target) => catalog.registry().check_target(target),
        PolicyTarget::Unadmitted(_) => Err(veoveo_types::ExtensionError::new(
            "owner target is unadmitted",
        )),
        _ => Ok(()),
    }
}
