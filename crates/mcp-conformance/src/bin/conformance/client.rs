use super::*;

/// Client handler that surfaces every server-initiated notification.
#[derive(Clone, Default)]
pub(super) struct CliHandler;

impl ClientHandler for CliHandler {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new("veoveo-conformance", env!("CARGO_PKG_VERSION")),
        )
    }

    async fn on_progress(
        &self,
        params: ProgressNotificationParam,
        _context: NotificationContext<rmcp::RoleClient>,
    ) {
        println!(
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
        params: TaskStatusNotificationParam,
        _context: NotificationContext<rmcp::RoleClient>,
    ) {
        println!(
            "  [task {}] {:?}: {}",
            params.task.task_id,
            params.task.status,
            params.task.status_message.as_deref().unwrap_or("")
        );
    }

    async fn on_resource_updated(
        &self,
        params: ResourceUpdatedNotificationParam,
        _context: NotificationContext<rmcp::RoleClient>,
    ) {
        println!("  [resource updated] {}", params.uri);
    }

    async fn on_resource_list_changed(&self, _context: NotificationContext<rmcp::RoleClient>) {
        println!("  [resource list changed]");
    }
}

pub(super) type Client = rmcp::service::RunningService<rmcp::RoleClient, CliHandler>;

pub(super) async fn connect(args: &Args) -> Result<Client> {
    let mut config = StreamableHttpClientTransportConfig::with_uri(args.url.clone());
    if let Some(token) = &args.bearer_token {
        config = config.auth_header(token.clone());
    } else if let Some(secret) = &args.internal_token_secret {
        config = config.auth_header(issue_internal_conformance_token(args, secret)?);
    }
    let transport = StreamableHttpClientTransport::from_config(config);
    Ok(CliHandler.serve(transport).await?)
}

fn issue_internal_conformance_token(args: &Args, secret: &str) -> Result<String> {
    let issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        InternalTokenSecret::new(secret.to_string())?,
    );
    let principal_issuer = TokenIssuer::new("https://conformance.veoveo.local")?;
    let principal_subject = TokenSubject::new("conformance")?;
    let principal = Principal {
        id: PrincipalId::new(format!("{principal_issuer}#{principal_subject}"))?,
        kind: PrincipalKind::Service,
        issuer: principal_issuer,
        subject: principal_subject,
        tenant: Some(TenantId::new("local")?),
        groups: Default::default(),
        roles: Default::default(),
        scopes: [ScopeName::new("operator:use")?].into_iter().collect(),
        data_labels: Default::default(),
        assurances: Default::default(),
        authenticated_at: Some(Utc::now()),
    };
    let token = issuer.issue(
        GatewayProfileId::new(args.internal_profile.clone())?,
        ServerSlug::new(args.internal_server.clone())?,
        principal,
        Utc::now() + TimeDelta::minutes(30),
    )?;
    Ok(token.bearer_token)
}
