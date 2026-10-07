//! Shared domain-neutral native SDK client; bearer issuance belongs to runtime callers.
use anyhow::Result;
use rmcp::{
    ClientHandler, ClientLifecycleMode, ClientServiceExt,
    model::{
        ClientCapabilities, ClientConfig, Implementation, ProgressNotificationParam,
        ResourceUpdatedNotificationParam, TaskStatusNotificationParams,
    },
    service::NotificationContext,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
pub struct ConnectionOptions {
    pub url: String,
    pub bearer_token: Option<String>,
}

/// Client handler that surfaces every server-initiated notification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskCapability {
    Disabled,
    Enabled,
}

#[derive(Clone)]
pub struct CliHandler {
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

pub type Client = rmcp::service::RunningService<rmcp::RoleClient, CliHandler>;

pub async fn connect(args: &ConnectionOptions, task_capability: TaskCapability) -> Result<Client> {
    let mut config = StreamableHttpClientTransportConfig::with_uri(args.url.clone());
    if let Some(token) = bearer_token_from_args(args)? {
        config = config.auth_header(token);
    }
    // Private denial probes require a response from the selected endpoint.
    // Redirects cannot substitute another endpoint's authorization outcome.
    let policy = if std::env::var_os("VEOVEO_SMOKE_FAILURE_REPORT").is_some() {
        reqwest::redirect::Policy::none()
    } else {
        reqwest::redirect::Policy::default()
    };
    let http = reqwest::Client::builder().redirect(policy).build()?;
    let transport = StreamableHttpClientTransport::with_client(http, config);
    Ok(CliHandler { task_capability }
        .serve_with_lifecycle(
            transport,
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await?)
}

fn bearer_token_from_args(args: &ConnectionOptions) -> Result<Option<String>> {
    Ok(args.bearer_token.clone())
}

pub mod failure;
