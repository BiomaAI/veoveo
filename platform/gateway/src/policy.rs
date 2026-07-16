use std::collections::BTreeSet;

use anyhow::Result;
use veoveo_mcp_contract::{
    GatewayAction, GatewayProfile, GatewayProfileId, McpMethodName, PolicyDecision, PolicyEffect,
    PolicyReasonCode, PolicyRule, PolicyRuleId, PolicyTarget, PolicyVersion, Principal,
    RecordingIngestResource, RecordingProducerRegistration, ResourceProjectionMode, ResourceScheme,
    ScopeName, ServerManifest, TraceId,
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

#[derive(Debug, Clone)]
pub struct RecordingIngestPolicyRequest<'a> {
    pub principal: &'a Principal,
    pub resource: &'a RecordingIngestResource,
    pub producer: &'a RecordingProducerRegistration,
    pub action: GatewayAction,
    pub trace_id: &'a TraceId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingIngestPolicyDecision {
    pub effect: PolicyEffect,
    pub reason: PolicyReasonCode,
    pub policy_version: Option<PolicyVersion>,
    pub rule_id: Option<PolicyRuleId>,
    pub trace_id: TraceId,
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
    pub fn decide_recording_ingest(
        &self,
        request: RecordingIngestPolicyRequest<'_>,
    ) -> RecordingIngestPolicyDecision {
        let deny = |reason, policy_version, rule_id| RecordingIngestPolicyDecision {
            effect: PolicyEffect::Deny,
            reason,
            policy_version,
            rule_id,
            trace_id: request.trace_id.clone(),
        };
        let Some(policy) = self.policy(&request.resource.policy_version) else {
            return deny(PolicyReasonCode::PolicyDeny, None, None);
        };
        if !request.producer.enabled
            || request.principal.tenant.as_ref() != Some(&request.producer.tenant)
        {
            return deny(
                PolicyReasonCode::UnknownTenant,
                Some(policy.version.clone()),
                None,
            );
        }
        if !request
            .resource
            .required_scopes
            .is_subset(&request.principal.scopes)
        {
            return deny(
                PolicyReasonCode::MissingScope,
                Some(policy.version.clone()),
                None,
            );
        }
        if let Some(rule) = policy.rules.iter().find(|rule| {
            rule.effect == PolicyEffect::Deny
                && recording_rule_match(rule, &request) == RuleMatchDetail::Match
        }) {
            return deny(
                PolicyReasonCode::PolicyDeny,
                Some(policy.version.clone()),
                Some(rule.id.clone()),
            );
        }
        let mut strongest_missing: Option<(PolicyReasonCode, PolicyRuleId)> = None;
        for rule in policy
            .rules
            .iter()
            .filter(|rule| rule.effect == PolicyEffect::Allow)
        {
            match recording_rule_match(rule, &request) {
                RuleMatchDetail::Match => {
                    return RecordingIngestPolicyDecision {
                        effect: PolicyEffect::Allow,
                        reason: PolicyReasonCode::PolicyAllow,
                        policy_version: Some(policy.version.clone()),
                        rule_id: Some(rule.id.clone()),
                        trace_id: request.trace_id.clone(),
                    };
                }
                RuleMatchDetail::MissingDataLabel => remember_strongest_missing_requirement(
                    &mut strongest_missing,
                    PolicyReasonCode::MissingDataLabel,
                    rule.id.clone(),
                ),
                RuleMatchDetail::MissingPrincipalAssurance => {
                    remember_strongest_missing_requirement(
                        &mut strongest_missing,
                        PolicyReasonCode::MissingPrincipalAssurance,
                        rule.id.clone(),
                    )
                }
                RuleMatchDetail::MissingRole => remember_strongest_missing_requirement(
                    &mut strongest_missing,
                    PolicyReasonCode::MissingRole,
                    rule.id.clone(),
                ),
                RuleMatchDetail::MissingGroup => remember_strongest_missing_requirement(
                    &mut strongest_missing,
                    PolicyReasonCode::MissingGroup,
                    rule.id.clone(),
                ),
                RuleMatchDetail::MissingTenant => remember_strongest_missing_requirement(
                    &mut strongest_missing,
                    PolicyReasonCode::MissingTenant,
                    rule.id.clone(),
                ),
                RuleMatchDetail::MissingPrincipal => remember_strongest_missing_requirement(
                    &mut strongest_missing,
                    PolicyReasonCode::MissingPrincipal,
                    rule.id.clone(),
                ),
                RuleMatchDetail::MissingScope => remember_strongest_missing_requirement(
                    &mut strongest_missing,
                    PolicyReasonCode::MissingScope,
                    rule.id.clone(),
                ),
                RuleMatchDetail::NoMatch => {}
            }
        }
        match strongest_missing {
            Some((reason, rule_id)) => deny(reason, Some(policy.version.clone()), Some(rule_id)),
            None => deny(
                PolicyReasonCode::PolicyDeny,
                Some(policy.version.clone()),
                None,
            ),
        }
    }

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

        if request
            .principal
            .data_labels
            .iter()
            .any(|label| self.data_label(label).is_none())
        {
            return deny(
                &request,
                PolicyReasonCode::UnknownDataLabel,
                request.target.clone(),
                Some(policy.version.clone()),
            );
        }

        if let Some(tenant) = &request.principal.tenant
            && self.tenant(tenant).is_none()
        {
            return deny(
                &request,
                PolicyReasonCode::UnknownTenant,
                request.target.clone(),
                Some(policy.version.clone()),
            );
        }

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
                RuleMatchDetail::MissingPrincipalAssurance => {
                    remember_strongest_missing_requirement(
                        &mut strongest_missing_requirement,
                        PolicyReasonCode::MissingPrincipalAssurance,
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
                if !manifest_owns_gateway_resource_uri(manifest, uri.as_str(), &scheme) {
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
            PolicyTarget::Task { server, task_id: _ } => {
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
            PolicyTarget::RecordingProducer { .. } | PolicyTarget::RecordingStream { .. } => {
                Err(PolicyReasonCode::PolicyDeny)
            }
        }
    }
}

fn recording_rule_match(
    rule: &PolicyRule,
    request: &RecordingIngestPolicyRequest<'_>,
) -> RuleMatchDetail {
    if !rule.actions.contains(&request.action)
        || (!rule.protected_resources.is_empty()
            && !rule
                .protected_resources
                .contains(&request.resource.protected_resource))
        || !rule.profiles.is_empty()
        || !rule.servers.is_empty()
        || !rule.tools.is_empty()
        || !rule.resource_schemes.is_empty()
        || !rule.prompts.is_empty()
    {
        return RuleMatchDetail::NoMatch;
    }
    let mut strongest = RuleMatchDetail::Match;
    if !rule.principal_ids.is_empty() && !rule.principal_ids.contains(&request.principal.id) {
        strongest = strongest_missing_rule_detail(strongest, RuleMatchDetail::MissingPrincipal);
    }
    if !rule.tenant_ids.is_empty()
        && request
            .principal
            .tenant
            .as_ref()
            .is_none_or(|tenant| !rule.tenant_ids.contains(tenant))
    {
        strongest = strongest_missing_rule_detail(strongest, RuleMatchDetail::MissingTenant);
    }
    if !rule.groups.is_empty() && !intersects(&rule.groups, &request.principal.groups) {
        strongest = strongest_missing_rule_detail(strongest, RuleMatchDetail::MissingGroup);
    }
    if !rule.roles.is_empty() && !intersects(&rule.roles, &request.principal.roles) {
        strongest = strongest_missing_rule_detail(strongest, RuleMatchDetail::MissingRole);
    }
    if !rule.required_scopes.is_subset(&request.principal.scopes) {
        strongest = strongest_missing_rule_detail(strongest, RuleMatchDetail::MissingScope);
    }
    if !rule
        .required_data_labels
        .is_subset(&request.producer.labels)
    {
        strongest = strongest_missing_rule_detail(strongest, RuleMatchDetail::MissingDataLabel);
    }
    if !rule
        .required_assurances
        .is_subset(&request.principal.assurances)
    {
        strongest =
            strongest_missing_rule_detail(strongest, RuleMatchDetail::MissingPrincipalAssurance);
    }
    strongest
}

fn manifest_owns_gateway_resource_uri(
    manifest: &ServerManifest,
    uri: &str,
    scheme: &ResourceScheme,
) -> bool {
    manifest.uri_scheme == *scheme
        || (manifest.resource_projection == ResourceProjectionMode::ServerOwned
            && scheme.as_str() == "ui"
            && uri.starts_with(&format!("ui://{}/", manifest.slug.as_str())))
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
    if !rule.required_assurances.is_empty()
        && !rule
            .required_assurances
            .is_subset(&request.principal.assurances)
    {
        strongest_missing_requirement = strongest_missing_rule_detail(
            strongest_missing_requirement,
            RuleMatchDetail::MissingPrincipalAssurance,
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
        PolicyTarget::Prompt { server, prompt } => {
            filter_matches(&rule.servers, server) && filter_matches(&rule.prompts, prompt)
        }
        PolicyTarget::Task { server, task_id: _ } => filter_matches(&rule.servers, server),
        PolicyTarget::RecordingProducer { .. } | PolicyTarget::RecordingStream { .. } => false,
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

#[cfg(test)]
mod recording_ingest_tests {
    use serde_json::Value;
    use veoveo_mcp_contract::{
        AuthorizationServerId, DataLabelId, OAuthClientId, PolicyEffect, ProtectedResourceId,
        ProtectedResourceName, RecordingApplicationId, RecordingDatasetName, RecordingProducerId,
        RecordingProducerQuotas, RecordingRetentionPolicy, UpstreamEndpoint, UpstreamTransport,
        UpstreamTransportSecurity, UpstreamUrl,
    };

    use super::*;

    fn fixture() -> (
        Principal,
        RecordingIngestResource,
        RecordingProducerRegistration,
        PolicyRule,
        TraceId,
    ) {
        let protected_resource =
            ProtectedResourceId::new("https://veoveo.example/ingest/recordings").unwrap();
        let tenant = veoveo_mcp_contract::TenantId::new("tenant-a").unwrap();
        let scope = ScopeName::new("recording:ingest").unwrap();
        let label = DataLabelId::new("cui").unwrap();
        let principal = Principal {
            id: veoveo_mcp_contract::PrincipalId::new("https://veoveo.example/oauth#sensor-a")
                .unwrap(),
            kind: veoveo_mcp_contract::PrincipalKind::Service,
            issuer: veoveo_mcp_contract::TokenIssuer::new("https://veoveo.example/oauth").unwrap(),
            subject: veoveo_mcp_contract::TokenSubject::new("sensor-a").unwrap(),
            tenant: Some(tenant.clone()),
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::from([scope.clone()]),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: None,
        };
        let producer = RecordingProducerRegistration {
            id: RecordingProducerId::new("sensor-a").unwrap(),
            oauth_client: OAuthClientId::new("sensor-a").unwrap(),
            tenant: tenant.clone(),
            dataset: RecordingDatasetName::new("factory-floor").unwrap(),
            allowed_application_ids: BTreeSet::from([RecordingApplicationId::new(
                "inspection-camera",
            )
            .unwrap()]),
            classification: "internal".to_owned(),
            labels: BTreeSet::from([label.clone()]),
            quotas: RecordingProducerQuotas {
                maximum_concurrent_streams: 4,
                maximum_batches_per_minute: 60,
                maximum_bytes_per_day: 1_000_000,
                maximum_stream_bytes: 500_000,
            },
            retention: RecordingRetentionPolicy {
                journal_grace_seconds: 3_600,
                open_stream_days: 7,
            },
            enabled: true,
            metadata: Value::Null,
        };
        let resource = RecordingIngestResource {
            id: ProtectedResourceName::new("recording-ingest").unwrap(),
            protected_resource: protected_resource.clone(),
            authorization_server: AuthorizationServerId::new("veoveo").unwrap(),
            policy_version: PolicyVersion::new("2026-07-16").unwrap(),
            upstream: UpstreamEndpoint {
                transport: UpstreamTransport::StreamableHttp,
                url: UpstreamUrl::new("http://recording-hub:9878").unwrap(),
                security: UpstreamTransportSecurity::ClusterInternalHttp,
                trusted_certificate_authorities: Vec::new(),
                client_certificate: None,
                client_private_key: None,
            },
            maximum_batch_bytes: 8_388_608,
            required_scopes: BTreeSet::from([scope.clone()]),
            producers: vec![producer.clone()],
            metadata: Value::Null,
        };
        let rule = PolicyRule {
            id: PolicyRuleId::new("allow-sensor-recording-ingest").unwrap(),
            effect: PolicyEffect::Allow,
            actions: BTreeSet::from([GatewayAction::RecordingBatchAppend]),
            profiles: BTreeSet::new(),
            protected_resources: BTreeSet::from([protected_resource]),
            servers: BTreeSet::new(),
            tools: BTreeSet::new(),
            resource_schemes: BTreeSet::new(),
            prompts: BTreeSet::new(),
            principal_ids: BTreeSet::from([principal.id.clone()]),
            tenant_ids: BTreeSet::from([tenant]),
            groups: BTreeSet::new(),
            roles: BTreeSet::new(),
            required_scopes: BTreeSet::from([scope]),
            required_data_labels: BTreeSet::from([label]),
            required_assurances: BTreeSet::new(),
            metadata: Value::Null,
        };
        (
            principal,
            resource,
            producer,
            rule,
            TraceId::new("trace-recording-ingest").unwrap(),
        )
    }

    #[test]
    fn recording_policy_matches_resource_producer_and_scope() {
        let (principal, resource, producer, rule, trace_id) = fixture();
        let request = RecordingIngestPolicyRequest {
            principal: &principal,
            resource: &resource,
            producer: &producer,
            action: GatewayAction::RecordingBatchAppend,
            trace_id: &trace_id,
        };

        assert_eq!(
            recording_rule_match(&rule, &request),
            RuleMatchDetail::Match
        );
    }

    #[test]
    fn recording_policy_reports_missing_producer_label() {
        let (principal, resource, mut producer, rule, trace_id) = fixture();
        producer.labels.clear();
        let request = RecordingIngestPolicyRequest {
            principal: &principal,
            resource: &resource,
            producer: &producer,
            action: GatewayAction::RecordingBatchAppend,
            trace_id: &trace_id,
        };

        assert_eq!(
            recording_rule_match(&rule, &request),
            RuleMatchDetail::MissingDataLabel
        );
    }
}
