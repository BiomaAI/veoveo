use std::collections::BTreeSet;

use anyhow::Result;
use veoveo_mcp_contract::{
    GatewayAction, GatewayProfile, GatewayProfileId, McpMethodName, PolicyDecision, PolicyEffect,
    PolicyReasonCode, PolicyRule, PolicyRuleId, PolicyTarget, PolicyVersion, Principal,
    ResourceScheme, ScopeName, TraceId,
};

use crate::GatewayCatalog;

#[derive(Debug, Clone)]
pub struct PolicyRequest<'a> {
    pub principal: &'a Principal,
    pub profile: &'a GatewayProfileId,
    pub action: GatewayAction,
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

pub(crate) fn exposure_contains<T: PartialEq>(
    exposure: &veoveo_mcp_contract::Exposure<T>,
    item: &T,
) -> bool {
    match exposure {
        veoveo_mcp_contract::Exposure::All => true,
        veoveo_mcp_contract::Exposure::Listed(items) => items.iter().any(|allowed| allowed == item),
        veoveo_mcp_contract::Exposure::None => false,
    }
}

impl GatewayCatalog {
    pub fn decide(&self, request: PolicyRequest<'_>) -> PolicyDecision {
        let Some(profile) = self.profile(request.profile) else {
            return deny(
                &request,
                PolicyReasonCode::UnknownProfile,
                PolicyTarget::Gateway,
                None,
            );
        };

        let Some(policy) = self.policy(&profile.policy_version) else {
            return deny(
                &request,
                PolicyReasonCode::PolicyDeny,
                request.target.clone(),
                None,
            );
        };

        if let Err(reason) = self.profile_allows_target(profile, request.action, request.target) {
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

        let matching_denial = policy
            .rules
            .iter()
            .find(|rule| {
                rule.effect == PolicyEffect::Deny
                    && rule_match_detail(rule, profile, &request) == RuleMatchDetail::Match
            })
            .map(|rule| rule.id.clone());
        if let Some(rule_id) = matching_denial {
            return decision(
                &request,
                PolicyEffect::Deny,
                PolicyReasonCode::PolicyDeny,
                request.target.clone(),
                Some(policy.version.clone()),
                Some(rule_id),
            );
        }

        let mut strongest_missing_requirement: Option<(PolicyReasonCode, PolicyRuleId)> = None;
        for rule in &policy.rules {
            if rule.effect != PolicyEffect::Allow {
                continue;
            }
            match rule_match_detail(rule, profile, &request) {
                RuleMatchDetail::Match => {
                    return decision(
                        &request,
                        PolicyEffect::Allow,
                        PolicyReasonCode::PolicyAllow,
                        request.target.clone(),
                        Some(policy.version.clone()),
                        Some(rule.id.clone()),
                    );
                }
                RuleMatchDetail::MissingDataLabel => {
                    remember_strongest_missing_requirement(
                        &mut strongest_missing_requirement,
                        PolicyReasonCode::MissingDataLabel,
                        rule.id.clone(),
                    );
                }
                RuleMatchDetail::MissingRole => {
                    remember_strongest_missing_requirement(
                        &mut strongest_missing_requirement,
                        PolicyReasonCode::MissingRole,
                        rule.id.clone(),
                    );
                }
                RuleMatchDetail::MissingGroup => {
                    remember_strongest_missing_requirement(
                        &mut strongest_missing_requirement,
                        PolicyReasonCode::MissingGroup,
                        rule.id.clone(),
                    );
                }
                RuleMatchDetail::MissingTenant => {
                    remember_strongest_missing_requirement(
                        &mut strongest_missing_requirement,
                        PolicyReasonCode::MissingTenant,
                        rule.id.clone(),
                    );
                }
                RuleMatchDetail::MissingPrincipal => {
                    remember_strongest_missing_requirement(
                        &mut strongest_missing_requirement,
                        PolicyReasonCode::MissingPrincipal,
                        rule.id.clone(),
                    );
                }
                RuleMatchDetail::MissingScope => {
                    remember_strongest_missing_requirement(
                        &mut strongest_missing_requirement,
                        PolicyReasonCode::MissingScope,
                        rule.id.clone(),
                    );
                }
                RuleMatchDetail::NoMatch => {}
            }
        }
        if let Some((reason, rule_id)) = strongest_missing_requirement {
            return decision(
                &request,
                PolicyEffect::Deny,
                reason,
                request.target.clone(),
                Some(policy.version.clone()),
                Some(rule_id),
            );
        }

        deny(
            &request,
            PolicyReasonCode::PolicyDeny,
            request.target.clone(),
            Some(policy.version.clone()),
        )
    }

    fn profile_allows_target(
        &self,
        profile: &GatewayProfile,
        action: GatewayAction,
        target: &PolicyTarget,
    ) -> Result<(), PolicyReasonCode> {
        match target {
            PolicyTarget::Gateway => Ok(()),
            PolicyTarget::Server { server } => {
                if self.server(server).is_none() {
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
                let manifest = self.server(server).ok_or(PolicyReasonCode::UnknownServer)?;
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
                let manifest = self.server(server).ok_or(PolicyReasonCode::UnknownServer)?;
                let scheme =
                    resource_scheme(uri.as_str()).ok_or(PolicyReasonCode::UnknownResource)?;
                if scheme != manifest.uri_scheme {
                    return Err(PolicyReasonCode::UnknownResource);
                }
                let exposure = profile
                    .servers
                    .iter()
                    .find(|exposure| &exposure.server == server)
                    .ok_or(PolicyReasonCode::PolicyDeny)?;
                if matches!(&exposure.resources, veoveo_mcp_contract::Exposure::All)
                    || exposure.resources.iter().any(|selector| match selector {
                        veoveo_mcp_contract::ResourceSelector::Scheme { scheme: allowed } => {
                            allowed == &scheme
                        }
                        veoveo_mcp_contract::ResourceSelector::UriPrefix { prefix } => {
                            uri.as_str().starts_with(prefix.as_ref())
                        }
                        veoveo_mcp_contract::ResourceSelector::Template { uri_template } => {
                            uri_template.matches_uri(uri)
                        }
                    })
                {
                    Ok(())
                } else {
                    Err(PolicyReasonCode::PolicyDeny)
                }
            }
            PolicyTarget::Prompt { server, prompt } => {
                let manifest = self.server(server).ok_or(PolicyReasonCode::UnknownServer)?;
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
            PolicyTarget::TaskList { server } => {
                let _manifest = self.server(server).ok_or(PolicyReasonCode::UnknownServer)?;
                let exposure = profile
                    .servers
                    .iter()
                    .find(|exposure| &exposure.server == server)
                    .ok_or(PolicyReasonCode::PolicyDeny)?;
                if exposure.tasks == veoveo_mcp_contract::TaskExposure::Enabled {
                    Ok(())
                } else {
                    Err(PolicyReasonCode::PolicyDeny)
                }
            }
            PolicyTarget::Task {
                server,
                gateway_task_id: _,
            } => {
                let _manifest = self.server(server).ok_or(PolicyReasonCode::UnknownServer)?;
                let exposure = profile
                    .servers
                    .iter()
                    .find(|exposure| &exposure.server == server)
                    .ok_or(PolicyReasonCode::PolicyDeny)?;
                if exposure.tasks == veoveo_mcp_contract::TaskExposure::Enabled
                    || matches!(
                        action,
                        GatewayAction::ResourcesList | GatewayAction::ResourcesTemplatesList
                    )
                {
                    Ok(())
                } else {
                    Err(PolicyReasonCode::PolicyDeny)
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleMatchDetail {
    Match,
    MissingPrincipal,
    MissingTenant,
    MissingGroup,
    MissingRole,
    MissingScope,
    MissingDataLabel,
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
        action: request.action,
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
    request: &PolicyRequest<'_>,
) -> RuleMatchDetail {
    if !rule.actions.contains(&request.action) {
        return RuleMatchDetail::NoMatch;
    }
    if !rule.profiles.is_empty() && !rule.profiles.contains(&profile.id) {
        return RuleMatchDetail::NoMatch;
    }
    if !matches_target_filters(rule, request.target) {
        return RuleMatchDetail::NoMatch;
    }
    let mut strongest_missing_requirement = RuleMatchDetail::Match;
    if !rule.principal_ids.is_empty() && !rule.principal_ids.contains(&request.principal.id) {
        strongest_missing_requirement = strongest_missing_rule_detail(
            strongest_missing_requirement,
            RuleMatchDetail::MissingPrincipal,
        );
    }
    if !rule.tenant_ids.is_empty() {
        match &request.principal.tenant {
            Some(tenant) if rule.tenant_ids.contains(tenant) => {}
            _ => {
                strongest_missing_requirement = strongest_missing_rule_detail(
                    strongest_missing_requirement,
                    RuleMatchDetail::MissingTenant,
                );
            }
        }
    }
    if !rule.groups.is_empty() && !intersects(&rule.groups, &request.principal.groups) {
        strongest_missing_requirement = strongest_missing_rule_detail(
            strongest_missing_requirement,
            RuleMatchDetail::MissingGroup,
        );
    }
    if !rule.roles.is_empty() && !intersects(&rule.roles, &request.principal.roles) {
        strongest_missing_requirement = strongest_missing_rule_detail(
            strongest_missing_requirement,
            RuleMatchDetail::MissingRole,
        );
    }
    if !rule.required_scopes.is_empty()
        && !rule.required_scopes.is_subset(&request.principal.scopes)
    {
        strongest_missing_requirement = strongest_missing_rule_detail(
            strongest_missing_requirement,
            RuleMatchDetail::MissingScope,
        );
    }
    if !rule.required_data_labels.is_empty()
        && !rule
            .required_data_labels
            .is_subset(&request.principal.data_labels)
    {
        strongest_missing_requirement = strongest_missing_rule_detail(
            strongest_missing_requirement,
            RuleMatchDetail::MissingDataLabel,
        );
    }
    strongest_missing_requirement
}

fn strongest_missing_rule_detail(left: RuleMatchDetail, right: RuleMatchDetail) -> RuleMatchDetail {
    if rule_detail_rank(right) > rule_detail_rank(left) {
        right
    } else {
        left
    }
}

fn rule_detail_rank(detail: RuleMatchDetail) -> u8 {
    match detail {
        RuleMatchDetail::MissingDataLabel => 60,
        RuleMatchDetail::MissingRole => 50,
        RuleMatchDetail::MissingGroup => 40,
        RuleMatchDetail::MissingTenant => 30,
        RuleMatchDetail::MissingPrincipal => 20,
        RuleMatchDetail::MissingScope => 10,
        RuleMatchDetail::Match | RuleMatchDetail::NoMatch => 0,
    }
}

fn remember_strongest_missing_requirement(
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
        PolicyReasonCode::MissingDataLabel => 60,
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
        PolicyTarget::Prompt { server, prompt } => {
            filter_matches(&rule.servers, server) && filter_matches(&rule.prompts, prompt)
        }
        PolicyTarget::TaskList { server } => filter_matches(&rule.servers, server),
        PolicyTarget::Task {
            server,
            gateway_task_id: _,
        } => filter_matches(&rule.servers, server),
    }
}

fn has_required_scopes(
    principal_scopes: &BTreeSet<veoveo_mcp_contract::ScopeName>,
    required: &[ScopeName],
) -> bool {
    required
        .iter()
        .all(|scope| principal_scopes.contains(scope))
}

trait ExposureResourceIter {
    fn iter(&self) -> Box<dyn Iterator<Item = &veoveo_mcp_contract::ResourceSelector> + '_>;
}

impl ExposureResourceIter for veoveo_mcp_contract::Exposure<veoveo_mcp_contract::ResourceSelector> {
    fn iter(&self) -> Box<dyn Iterator<Item = &veoveo_mcp_contract::ResourceSelector> + '_> {
        match self {
            veoveo_mcp_contract::Exposure::All | veoveo_mcp_contract::Exposure::None => {
                Box::new([].iter())
            }
            veoveo_mcp_contract::Exposure::Listed(items) => Box::new(items.iter()),
        }
    }
}

fn filter_matches<T: Ord>(filter: &BTreeSet<T>, value: &T) -> bool {
    filter.is_empty() || filter.contains(value)
}

fn intersects<T: Ord>(left: &BTreeSet<T>, right: &BTreeSet<T>) -> bool {
    left.iter().any(|value| right.contains(value))
}
