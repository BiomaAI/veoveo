use super::*;

/// Client handler that surfaces every server-initiated notification.
#[derive(Clone, Default)]
pub(super) struct CliHandler;

impl ClientHandler for CliHandler {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::new(
            ClientCapabilities::builder().enable_tasks().build(),
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

pub(super) async fn connect(args: &Args) -> Result<Client> {
    let mut config = StreamableHttpClientTransportConfig::with_uri(args.url.clone());
    if let Some(token) = bearer_token_from_args(args)? {
        config = config.auth_header(token);
    }
    let transport = StreamableHttpClientTransport::from_config(config);
    Ok(CliHandler
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
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        GatewayInternalSigningKey::new(args.internal_signing_key_id.clone(), private_key_der)?,
    );
    let principal_issuer = TokenIssuer::new("https://conformance.veoveo.local")?;
    let principal_subject = TokenSubject::new(args.internal_principal_subject.clone())?;
    let principal = Principal {
        id: PrincipalId::new(format!("{principal_issuer}#{principal_subject}"))?,
        kind: PrincipalKind::Service,
        issuer: principal_issuer,
        subject: principal_subject,
        tenant: Some(TenantId::new(args.internal_tenant.clone())?),
        groups: Default::default(),
        group_roles: Default::default(),
        roles: Default::default(),
        scopes: args
            .internal_scopes
            .iter()
            .map(|scope| ScopeName::new(scope.clone()))
            .collect::<Result<_, _>>()?,
        data_labels: Default::default(),
        assurances: Default::default(),
        authenticated_at: Some(Utc::now()),
    };
    let authority = InvocationAuthority {
        work_context: WorkContextId::new(args.internal_work_context.clone())?,
        tenant: TenantId::new(args.internal_tenant.clone())?,
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: PolicyVersion::new("r1")?,
        output_policy: WorkContextOutputPolicy {
            owner: AccessSubject::Principal(principal.id.clone()),
            initial_grants: Vec::new(),
            classification: None,
            data_labels: Default::default(),
        },
        provenance: InvocationProvenance::Automated,
    };
    let token = issuer.issue(
        GatewayProfileId::new(args.internal_profile.clone())?,
        ServerSlug::new(args.internal_server.clone())?,
        principal,
        authority,
        Utc::now() + TimeDelta::minutes(30),
    )?;
    Ok(token.bearer_token)
}
