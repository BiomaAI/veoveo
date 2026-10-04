use super::*;
use veoveo_gateway_contract::ProtectedResourceId;
use veoveo_mcp_contract::{GatewayRequestContext, OAuthClientId};

/// Client handler that surfaces every server-initiated notification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TaskCapability {
    Disabled,
    Enabled,
}

#[derive(Clone)]
pub(super) struct CliHandler {
    task_capability: TaskCapability,
}

impl CliHandler {
    fn capabilities(&self) -> ClientCapabilities {
        let builder = ClientCapabilities::builder();
        match self.task_capability {
            TaskCapability::Disabled => builder.build(),
            TaskCapability::Enabled => builder.enable_tasks().build(),
        }
    }
}

impl ClientHandler for CliHandler {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(
            self.capabilities(),
            Implementation::new("veoveo-conformance", env!("CARGO_PKG_VERSION")),
        )
    }

    async fn on_progress(
        &self,
        params: ProgressNotificationParam,
        _context: NotificationContext<rmcp::RoleClient>,
    ) {
        eprintln!(
            "  [progress] {:.0}%{}",
            params.progress * 100.0 / params.total.unwrap_or(1.0),
            params
                .message
                .map(|m| format!(" — {m}"))
                .unwrap_or_default()
        );
    }

    async fn on_task_status(
        &self,
        params: TaskStatusNotificationParams,
        _context: NotificationContext<rmcp::RoleClient>,
    ) {
        eprintln!(
            "  [task {}] {:?}: {}",
            params.task.task.task_id,
            params.task.status(),
            params.task.task.status_message.as_deref().unwrap_or("")
        );
    }

    async fn on_resource_updated(
        &self,
        params: ResourceUpdatedNotificationParam,
        _context: NotificationContext<rmcp::RoleClient>,
    ) {
        eprintln!("  [resource updated] {}", params.uri);
    }

    async fn on_resource_list_changed(&self, _context: NotificationContext<rmcp::RoleClient>) {
        eprintln!("  [resource list changed]");
    }
}

pub(super) type Client = rmcp::service::RunningService<rmcp::RoleClient, CliHandler>;

pub(super) async fn connect(args: &Args, task_capability: TaskCapability) -> Result<Client> {
    let mut config = StreamableHttpClientTransportConfig::with_uri(args.url.clone());
    if let Some(token) = bearer_token_from_args(args)? {
        config = config.auth_header(token);
    }
    let transport = StreamableHttpClientTransport::from_config(config);
    Ok(CliHandler { task_capability }
        .serve_with_lifecycle(
            transport,
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await?)
}

pub(super) fn bearer_token_from_args(args: &Args) -> Result<Option<String>> {
    if let Some(token) = &args.bearer_token {
        Ok(Some(token.clone()))
    } else if let Some(private_key_der_b64) = &args.internal_signing_key_der_b64 {
        Ok(Some(issue_internal_conformance_token(
            args,
            private_key_der_b64,
        )?))
    } else {
        Ok(None)
    }
}

fn issue_internal_conformance_token(args: &Args, private_key_der_b64: &str) -> Result<String> {
    let private_key_der = BASE64_STANDARD.decode(private_key_der_b64.trim())?;
    let issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::parse(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        GatewayInternalSigningKey::new(args.internal_signing_key_id.clone(), private_key_der)?,
    );
    let principal_issuer = TokenIssuer::parse("https://conformance.veoveo.local")?;
    let principal_subject = TokenSubject::parse(args.internal_principal_subject.clone())?;
    let principal = Principal {
        id: PrincipalId::parse(format!("{principal_issuer}#{principal_subject}"))?,
        kind: PrincipalKind::Service,
        issuer: principal_issuer,
        subject: principal_subject,
        tenant: Some(TenantId::parse(args.internal_tenant.clone())?),
        groups: Default::default(),
        group_roles: Default::default(),
        roles: Default::default(),
        scopes: args
            .internal_scopes
            .iter()
            .map(|scope| ScopeName::parse(scope.clone()))
            .collect::<Result<_, _>>()?,
        data_labels: Default::default(),
        assurances: Default::default(),
        authenticated_at: Some(Utc::now()),
    };
    let authority = InvocationAuthority {
        work_context: WorkContextId::parse(args.internal_work_context.clone())?,
        tenant: TenantId::parse(args.internal_tenant.clone())?,
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: PolicyVersion::parse("r1")?,
        output_policy: WorkContextOutputPolicy {
            owner: AccessSubject::Principal(principal.id.clone()),
            initial_grants: Vec::new(),
            classification: None,
            data_labels: Default::default(),
        },
        provenance: InvocationProvenance::Automated,
    };
    let now = Utc::now();
    let expires_at = now + TimeDelta::minutes(30);
    let request_context = GatewayRequestContext {
        format: veoveo_mcp_contract::GatewayRequestContextFormat::V2,
        audit: veoveo_mcp_contract::audit::AuditRequest::background(),
        access_token: AccessTokenSubject {
            managed_execution: None,
            issuer: principal.issuer.clone(),
            subject: principal.subject.clone(),
            oauth_client_id: OAuthClientId::parse(principal.subject.as_str())?,
            session_family: None,
            audience: ProtectedResourceId::parse("https://conformance.veoveo.local")?,
            work_context: authority.work_context.clone(),
            invocation_mode: authority.provenance.mode(),
            initiator: None,
            delegation_id: None,
            scopes: principal.scopes.clone(),
            jwt_id: None,
            issued_at: now,
            not_before: Some(now),
            expires_at,
        },
        principal: principal.clone(),
    };
    let token = issuer.issue(
        GatewayProfileId::parse(args.internal_profile.clone())?,
        ServerSlug::parse(args.internal_server.clone())?,
        principal,
        authority,
        Some(request_context),
        expires_at,
    )?;
    Ok(token.bearer_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_certification_signs_consistent_automated_request_context() {
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
        let args = Args {
            url: "http://localhost:8804/computers/mcp".into(),
            scheme: "computer".into(),
            bearer_token: None,
            internal_signing_key_der_b64: None,
            internal_signing_key_id: "certification-key".into(),
            internal_server: "computers".into(),
            internal_profile: "test-profile".into(),
            internal_work_context: "test-context".into(),
            internal_principal_subject: "test-client".into(),
            internal_tenant: "test-tenant".into(),
            internal_scopes: vec!["operator:use".into(), "extension:read".into()],
            cmd: Cmd::Certify {
                profile: "unused.json".into(),
                report: "unused-report.json".into(),
            },
        };
        let trust = GatewayInternalTrustBundle::from_json(
            &json!({"keys": [{
                "kty": "OKP", "crv": "Ed25519", "alg": "EdDSA", "use": "sig",
                "kid": args.internal_signing_key_id,
                "x": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key.public_key_raw()),
            }]})
            .to_string(),
        )
        .unwrap();
        let token =
            issue_internal_conformance_token(&args, &BASE64_STANDARD.encode(key.serialize_der()))
                .unwrap();
        let identity = GatewayInternalTokenVerifier::new(
            TokenIssuer::parse(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap(),
            ServerSlug::parse("computers").unwrap(),
            trust,
        )
        .verify(&token)
        .unwrap();
        let context = identity.request_context.as_ref().unwrap();
        context
            .validate_for(&identity.actor, &identity.authority)
            .unwrap();
        assert_eq!(context.access_token.oauth_client_id.as_str(), "test-client");
        assert_eq!(
            identity.actor.tenant.as_ref().unwrap().as_str(),
            "test-tenant"
        );
        assert_eq!(context.access_token.work_context.as_str(), "test-context");
        assert_eq!(context.access_token.scopes.len(), 2);
        assert_eq!(identity.profile.as_str(), "test-profile");
        assert!(context.access_token.session_family.is_none());
        assert!(context.access_token.initiator.is_none());
        assert!(context.access_token.delegation_id.is_none());
        assert!(identity.expires_at <= context.access_token.expires_at);
        assert_eq!(
            context.access_token.expires_at - context.access_token.issued_at,
            TimeDelta::minutes(30)
        );
    }

    #[test]
    fn direct_client_does_not_advertise_tasks() {
        let handler = CliHandler {
            task_capability: TaskCapability::Disabled,
        };
        assert!(!handler.capabilities().supports_tasks());
    }

    #[test]
    fn task_client_advertises_tasks() {
        let handler = CliHandler {
            task_capability: TaskCapability::Enabled,
        };
        assert!(handler.capabilities().supports_tasks());
    }
}
