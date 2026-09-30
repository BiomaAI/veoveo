use super::{Fault, Route, SpeechState};
use axum::http::{HeaderValue, StatusCode};
use chrono::{TimeDelta, Utc};
use std::{sync::Arc, time::Duration};
use veoveo_mcp_contract::audit::AuditDetail;
use veoveo_mcp_contract::{self as contract, ServerSlug};
use veoveo_mcp_gateway::{AuthenticatedSubject, GatewayCatalog, PolicyRequest};

// Deliberately no Debug: this contains an internal assertion.
pub(super) struct Authorized {
    pub catalog: Arc<GatewayCatalog>,
    pub manifest: contract::ServerManifest,
    pub authorization: HeaderValue,
    pub url: url::Url,
}

pub(super) async fn authorize(
    state: &SpeechState,
    route: &Route,
    subject: AuthenticatedSubject,
) -> Result<Authorized, Fault> {
    tokio::time::timeout(Duration::from_secs(10), admitted(state, route, subject))
        .await
        .map_err(|_| Fault::unavailable())?
}
async fn admitted(
    state: &SpeechState,
    route: &Route,
    subject: AuthenticatedSubject,
) -> Result<Authorized, Fault> {
    let catalog = state.catalog.current();
    let server = ServerSlug::new("speech").expect("static server");
    let (_, _, manifest) = catalog
        .profile_server(&route.profile, &server)
        .ok_or_else(Fault::missing)?;
    let manifest = manifest.clone();
    let (target, actions) = route.authorization();
    for &action in actions {
        let trace = contract::TraceId::new(subject.audit.trace_id.to_string()).expect("UUID trace");
        let mut decision = catalog.decide(PolicyRequest {
            principal: &subject.principal,
            profile: &route.profile,
            action,
            target: &target,
            trace_id: &trace,
        });
        if !subject
            .authority
            .membership
            .allows(veoveo_types::WorkContextMembershipLevel::Contributor)
            || subject.actor.kind != contract::PrincipalKind::User
            || subject.access_token.session_family.is_none()
        {
            decision.effect = contract::PolicyEffect::Deny;
            decision.reason = contract::PolicyReasonCode::MissingPrincipalAssurance;
        }
        if decision.effect != contract::PolicyEffect::Allow {
            state
                .gateway_state
                .record_policy_admission(
                    &subject,
                    &route.profile,
                    &target,
                    AuditDetail::DictationDenial,
                    &decision,
                )
                .await
                .map_err(|_| Fault::unavailable())?;
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
        .map_err(|_| Fault(StatusCode::UNAUTHORIZED))?;
    let mut authorization = HeaderValue::from_str(&format!("Bearer {}", token.bearer_token))
        .map_err(|_| Fault::unavailable())?;
    authorization.set_sensitive(true);
    let mut url =
        url::Url::parse(manifest.upstream.url.as_str()).map_err(|_| Fault::unavailable())?;
    let mount = manifest.mount_path.as_str().trim_end_matches('/');
    url.set_path(&format!("{mount}{}", route.path));
    url.set_query(None);
    url.set_fragment(None);
    Ok(Authorized {
        catalog,
        manifest,
        authorization,
        url,
    })
}
