use super::{
    ComputersState, Fault,
    routes::{Operation, Route},
};
use axum::http::{HeaderValue, StatusCode};
use chrono::{TimeDelta, Utc};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};
use veoveo_mcp_contract::{self as contract, ServerSlug};
use veoveo_mcp_gateway::{
    AuthenticatedSubject, GatewayCatalog, PolicyRequest, merge_principal_audit_metadata,
};

// Deliberately no Debug: this contains an internal assertion.
pub(super) struct Authorized {
    pub catalog: Arc<GatewayCatalog>,
    pub manifest: contract::ServerManifest,
    pub authorization: HeaderValue,
    pub url: url::Url,
}

pub(super) async fn authorize(
    state: &ComputersState,
    route: &Route,
    operation: Operation,
    subject: AuthenticatedSubject,
) -> Result<Authorized, Fault> {
    tokio::time::timeout(
        Duration::from_secs(10),
        admitted(state, route, operation, subject),
    )
    .await
    .map_err(|_| Fault::unavailable())?
}
async fn admitted(
    state: &ComputersState,
    route: &Route,
    operation: Operation,
    subject: AuthenticatedSubject,
) -> Result<Authorized, Fault> {
    let started = Instant::now();
    let catalog = state.catalog.current();
    let server = ServerSlug::new("computers").expect("static server");
    let (_, _, manifest) = catalog
        .profile_server(&route.profile, &server)
        .ok_or_else(Fault::missing)?;
    let manifest = manifest.clone();
    let (target, actions) = operation.authorization();
    for &action in actions {
        let trace = contract::TraceId::new(uuid::Uuid::now_v7().to_string()).expect("UUID trace");
        let mut decision = catalog.decide(PolicyRequest {
            principal: &subject.principal,
            profile: &route.profile,
            action,
            target: &target,
            trace_id: &trace,
        });
        if operation.requires_contributor()
            && !subject
                .authority
                .membership
                .allows(contract::WorkContextMembershipLevel::Contributor)
        {
            decision.effect = contract::PolicyEffect::Deny;
            decision.reason = contract::PolicyReasonCode::MissingRole;
        }
        if operation.is_attachment() && subject.access_token.session_family.is_none() {
            decision.effect = contract::PolicyEffect::Deny;
            decision.reason = contract::PolicyReasonCode::MissingPrincipalAssurance;
        }
        state
            .gateway_state
            .record_audit_event(&contract::AuditEvent {
                event_id: trace.clone(),
                timestamp: decision.evaluated_at,
                trace_id: trace,
                profile: route.profile.clone(),
                method: contract::McpMethodName::new("computers/http").expect("static method"),
                action,
                target: target.clone(),
                decision: decision.clone(),
                principal: Some(subject.principal.id.clone()),
                principal_attributes: Some(contract::PrincipalAuditAttributes::from(
                    &subject.principal,
                )),
                tenant: subject.principal.tenant.clone(),
                token_issuer: Some(subject.access_token.issuer.clone()),
                latency_ms: u64::try_from(started.elapsed().as_millis()).ok(),
                metadata: merge_principal_audit_metadata(BTreeMap::new(), &subject.principal),
            })
            .await
            .map_err(|_| Fault::unavailable())?;
        if decision.effect != contract::PolicyEffect::Allow {
            return Err(Fault::denied());
        }
    }
    let context = subject.request_context();
    let expires = subject
        .access_token
        .expires_at
        .min(Utc::now() + TimeDelta::seconds(60));
    let token = state
        .issuer
        .issue(
            route.profile.clone(),
            server,
            subject.actor,
            subject.authority,
            Some(context),
            expires,
        )
        .map_err(|_| {
            Fault(
                StatusCode::UNAUTHORIZED,
                veoveo_computers_contract::ErrorCode::Forbidden,
            )
        })?;
    let mut authorization = HeaderValue::from_str(&format!("Bearer {}", token.bearer_token))
        .map_err(|_| Fault::unavailable())?;
    authorization.set_sensitive(true);
    let mut url =
        url::Url::parse(manifest.upstream.url.as_str()).map_err(|_| Fault::unavailable())?;
    let mount = manifest.mount_path.as_str().trim_end_matches('/');
    url.set_path(&format!("{mount}/admin/{}", operation.service_path()));
    url.set_query(None);
    url.set_fragment(None);
    Ok(Authorized {
        catalog,
        manifest,
        authorization,
        url,
    })
}
