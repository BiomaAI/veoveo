//! Recording authorization over a current admitted catalog; no HTTP or runtime.
use crate::{RecordingAction, RecordingIngestResource, RecordingProducerRegistration};
#[cfg(test)]
use std::collections::BTreeSet;
use veoveo_mcp_contract::{
    PolicyEffect, PolicyReasonCode, PolicyRule, PolicyRuleId, Principal, TraceId,
};
use veoveo_policy::{
    PolicyCatalogView, RuleMatchDetail, assemble_rule_outcome, principal_rule_conditions,
};
use veoveo_types::PolicyVersion;
#[cfg(test)]
use veoveo_types::ScopeName;

#[derive(Debug, Clone)]
pub struct RecordingIngestPolicyRequest<'a> {
    pub principal: &'a Principal,
    pub resource: &'a RecordingIngestResource,
    pub producer: &'a RecordingProducerRegistration,
    pub action: RecordingAction,
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

pub fn decide_recording_ingest(
    catalog: &impl PolicyCatalogView,
    request: RecordingIngestPolicyRequest<'_>,
) -> RecordingIngestPolicyDecision {
    let deny = |reason, policy_version, rule_id| RecordingIngestPolicyDecision {
        effect: PolicyEffect::Deny,
        reason,
        policy_version,
        rule_id,
        trace_id: request.trace_id.clone(),
    };
    if request.action == RecordingAction::LayerPublish
        || catalog
            .registry()
            .action_key::<RecordingAction>()
            .and_then(|key| key.action(request.action))
            .and_then(|handle| catalog.registry().check_action(&handle))
            .is_err()
    {
        return deny(PolicyReasonCode::PolicyDeny, None, None);
    }
    let Some(policy) = catalog.policy(&request.resource.policy_version) else {
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
    let outcome = assemble_rule_outcome(policy, |rule| recording_rule_match(rule, &request));
    RecordingIngestPolicyDecision {
        effect: outcome.effect,
        reason: outcome.reason,
        policy_version: Some(policy.version.clone()),
        rule_id: outcome.rule_id,
        trace_id: request.trace_id.clone(),
    }
}

fn recording_rule_match(
    rule: &PolicyRule,
    request: &RecordingIngestPolicyRequest<'_>,
) -> RuleMatchDetail {
    if !rule
        .actions
        .iter()
        .any(|action| action.as_str() == request.action.as_str())
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
    principal_rule_conditions(rule, request.principal, &request.producer.labels)
}

#[cfg(test)]
mod recording_ingest_tests {
    use crate::{
        RecordingApplicationId, RecordingDatasetName, RecordingProducerBlueprintPolicy,
        RecordingProducerId, RecordingProducerQuotas, RecordingRetentionPolicy,
    };
    use serde_json::Value;
    use veoveo_gateway_contract::{
        AuthorizationServerId, HttpUpstreamEndpoint, ProtectedResourceId, ProtectedResourceName,
        UpstreamTransportSecurity, UpstreamUrl,
    };
    use veoveo_mcp_contract::PolicyEffect;
    use veoveo_types::DataLabelId;
    use veoveo_types::OAuthClientId;

    use super::*;

    fn fixture() -> (
        Principal,
        RecordingIngestResource,
        RecordingProducerRegistration,
        PolicyRule,
        TraceId,
    ) {
        let protected_resource =
            ProtectedResourceId::parse("https://veoveo.example/ingest/recordings").unwrap();
        let tenant = veoveo_types::TenantId::parse("tenant-a").unwrap();
        let scope = ScopeName::parse("recording:ingest").unwrap();
        let label = DataLabelId::parse("cui").unwrap();
        let principal = Principal {
            id: veoveo_types::PrincipalId::parse("https://veoveo.example/oauth#sensor-a").unwrap(),
            kind: veoveo_mcp_contract::PrincipalKind::Service,
            issuer: veoveo_mcp_contract::TokenIssuer::parse("https://veoveo.example/oauth")
                .unwrap(),
            subject: veoveo_mcp_contract::TokenSubject::parse("sensor-a").unwrap(),
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
            id: RecordingProducerId::parse("sensor-a").unwrap(),
            oauth_client: OAuthClientId::parse("sensor-a").unwrap(),
            tenant: tenant.clone(),
            dataset: RecordingDatasetName::parse("factory-floor").unwrap(),
            allowed_application_ids: BTreeSet::from([RecordingApplicationId::parse(
                "inspection-camera",
            )
            .unwrap()]),
            single_recording_application_ids: BTreeSet::new(),
            classification: "internal".to_owned(),
            labels: BTreeSet::from([label.clone()]),
            quotas: RecordingProducerQuotas {
                maximum_concurrent_streams: 4,
                maximum_batches_per_minute: 60,
                maximum_bytes_per_day: 1_000_000,
                maximum_stream_bytes: 500_000,
            },
            blueprints: RecordingProducerBlueprintPolicy {
                enabled: true,
                maximum_bytes: 2 * 1024 * 1024,
                maximum_messages: 10_000,
                maximum_revisions: 32,
            },
            retention: RecordingRetentionPolicy {
                open_stream_days: 7,
            },
            enabled: true,
            metadata: Value::Null,
        };
        let resource = RecordingIngestResource {
            id: ProtectedResourceName::parse("recording-ingest").unwrap(),
            protected_resource: protected_resource.clone(),
            authorization_server: AuthorizationServerId::parse("veoveo").unwrap(),
            policy_version: PolicyVersion::parse("2026-07-16").unwrap(),
            upstream: HttpUpstreamEndpoint {
                url: UpstreamUrl::parse("http://recording-hub:9878").unwrap(),
                health_url: UpstreamUrl::parse("http://recording-hub:9878/healthz").unwrap(),
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
            id: PolicyRuleId::parse("allow-sensor-recording-ingest").unwrap(),
            effect: PolicyEffect::Allow,
            actions: BTreeSet::from([veoveo_types::ActionName::parse(
                RecordingAction::BatchAppend.as_str(),
            )
            .unwrap()]),
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
            TraceId::parse("trace-recording-ingest").unwrap(),
        )
    }

    #[test]
    fn recording_policy_matches_resource_producer_and_scope() {
        let (principal, resource, producer, rule, trace_id) = fixture();
        let request = RecordingIngestPolicyRequest {
            principal: &principal,
            resource: &resource,
            producer: &producer,
            action: RecordingAction::BatchAppend,
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
            action: RecordingAction::BatchAppend,
            trace_id: &trace_id,
        };

        assert_eq!(
            recording_rule_match(&rule, &request),
            RuleMatchDetail::MissingDataLabel
        );
    }
    #[test]
    fn registered_ingest_entrypoint_preserves_shared_deny_and_principal_matching() {
        let (principal, resource, producer, rule, trace) = fixture();
        let mut builder = veoveo_gateway_contract::CatalogRegistryBuilder::new(
            [],
            ["gateway", "server", "resource"]
                .into_iter()
                .map(|kind| veoveo_types::ExtensionName::parse(kind).unwrap()),
        );
        crate::register_catalog(&mut builder).unwrap();
        let registry = builder.build().unwrap();
        struct Catalog {
            registry: veoveo_gateway_contract::CatalogRegistry,
            sections: veoveo_gateway_contract::AdmittedCatalogSections,
            policy: veoveo_mcp_contract::PolicySet,
        }
        impl PolicyCatalogView for Catalog {
            fn registry(&self) -> &veoveo_gateway_contract::CatalogRegistry {
                &self.registry
            }
            fn sections(&self) -> &veoveo_gateway_contract::AdmittedCatalogSections {
                &self.sections
            }
            fn policy(&self, _: &PolicyVersion) -> Option<&veoveo_mcp_contract::PolicySet> {
                Some(&self.policy)
            }
            fn profile(
                &self,
                _: &veoveo_mcp_contract::GatewayProfileId,
            ) -> Option<&veoveo_mcp_contract::GatewayProfile> {
                None
            }
            fn data_label(
                &self,
                _: &veoveo_types::DataLabelId,
            ) -> Option<&veoveo_mcp_contract::DataLabelDefinition> {
                None
            }
            fn tenant(
                &self,
                _: &veoveo_types::TenantId,
            ) -> Option<&veoveo_mcp_contract::TenantDefinition> {
                None
            }
            fn server(
                &self,
                _: &veoveo_types::ServerSlug,
            ) -> Option<&veoveo_mcp_contract::ServerManifest> {
                None
            }
        }
        let mut catalog = Catalog {
            sections: registry
                .admit_sections(&Default::default(), &Default::default())
                .unwrap(),
            registry,
            policy: veoveo_mcp_contract::PolicySet {
                version: resource.policy_version.clone(),
                rules: vec![rule.clone()],
                metadata: Value::Null,
            },
        };
        let request = |action| RecordingIngestPolicyRequest {
            principal: &principal,
            resource: &resource,
            producer: &producer,
            action,
            trace_id: &trace,
        };
        assert_eq!(
            decide_recording_ingest(&catalog, request(RecordingAction::BatchAppend)).effect,
            PolicyEffect::Allow
        );
        assert_eq!(
            decide_recording_ingest(&catalog, request(RecordingAction::LayerPublish)).effect,
            PolicyEffect::Deny
        );
        let mut deny = rule;
        deny.effect = PolicyEffect::Deny;
        catalog.policy.rules.push(deny);
        assert_eq!(
            decide_recording_ingest(&catalog, request(RecordingAction::BatchAppend)).reason,
            PolicyReasonCode::PolicyDeny
        );
        catalog.registry = veoveo_gateway_contract::CatalogRegistryBuilder::new([], [])
            .build()
            .unwrap();
        assert_eq!(
            decide_recording_ingest(&catalog, request(RecordingAction::BatchAppend)).effect,
            PolicyEffect::Deny
        );
    }
}
