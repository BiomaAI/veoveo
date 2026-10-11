//! Real Time/Gateway/Store/kernel qualification with a deterministic model.
use super::*;
use std::collections::BTreeSet;
use veoveo_gateway_contract::ProtectedResourceId;
use veoveo_mcp_contract::{
    AuthMode, Exposure, GatewayControlPlane, GatewayProfileId, LocalToolName, OAuthClientId,
};
use veoveo_time_mcp::contract::{TimeResource, TimeScope};
use veoveo_types::ResourceAddress;

const PROFILE: &str = "agent-time-events";
const CLIENT: &str = "agent-time-events";

/// Compose a qualification-local grant. The reference installation's policy,
/// clients and templates are never published by this scenario.
fn qualification_catalog(
    base: &Path,
    reference: &Path,
    time_base: &url::Url,
) -> Result<GatewayControlPlane> {
    let mut catalog: GatewayControlPlane = serde_json::from_slice(&fs::read(base)?)?;
    let reference: GatewayControlPlane = serde_json::from_slice(&fs::read(reference)?)?;
    let mut time = reference
        .servers
        .iter()
        .find(|s| s.slug.as_str() == "time")
        .context("reference lacks Time owner")?
        .clone();
    let base_upstream = catalog
        .servers
        .first()
        .context("base lacks loopback upstream profile")?
        .upstream
        .clone();
    time.upstream = base_upstream;
    let mut endpoint = time_base.clone();
    endpoint.set_path("/time/mcp");
    time.upstream.url = veoveo_gateway_contract::UpstreamUrl::parse(endpoint.as_str())?;
    endpoint.set_path("/time/readyz");
    time.upstream.health_url = veoveo_gateway_contract::UpstreamUrl::parse(endpoint.as_str())?;
    time.knowledge.clear();
    time.app_resource_dependencies.clear();
    time.app_tool_dependencies.clear();
    let old_profile = GatewayProfileId::parse("operator")?;
    let old_client = OAuthClientId::parse("operator-service")?;
    let profile = GatewayProfileId::parse(PROFILE)?;
    let client = OAuthClientId::parse(CLIENT)?;
    let mut selected = catalog
        .profiles
        .iter()
        .find(|p| p.id == old_profile)
        .context("base lacks operator profile")?
        .clone();
    let old_resource = selected.protected_resource.clone();
    let mut resource = url::Url::parse(old_resource.as_str())?;
    resource.set_path(&format!("/mcp/{PROFILE}"));
    selected.id = profile.clone();
    selected.auth_modes = BTreeSet::from([AuthMode::OAuthClientCredentials]);
    selected.protected_resource = ProtectedResourceId::parse(resource.as_str())?;
    selected
        .required_scopes
        .extend([TimeScope::Read.into(), TimeScope::EventWrite.into()]);
    let mut exposure = reference
        .profiles
        .iter()
        .find(|p| p.id == old_profile)
        .and_then(|p| p.servers.iter().find(|s| s.server == time.slug))
        .context("reference lacks operator Time exposure")?
        .clone();
    exposure.tools = Exposure::Listed(
        ["create_temporal_event", "cancel_temporal_event"]
            .into_iter()
            .map(LocalToolName::parse)
            .collect::<std::result::Result<_, _>>()?,
    );
    selected.servers = vec![exposure];
    let mut registration = catalog
        .oauth_clients
        .iter()
        .find(|c| c.id == old_client)
        .context("base lacks machine client")?
        .clone();
    let tenant = registration
        .tenant
        .as_ref()
        .context("qualification machine client requires an explicit tenant")?
        .clone();
    registration.id = client.clone();
    registration.allowed_resources = BTreeSet::from([selected.protected_resource.clone()]);
    registration.allowed_scopes = selected.required_scopes.iter().cloned().collect();
    catalog.work_contexts.retain(|context| {
        context.id == registration.default_work_context && context.tenant == tenant
    });
    for context in &mut catalog.work_contexts {
        context.memberships.retain_mut(|membership| {
            if !membership.oauth_clients.contains(&old_client) {
                return false;
            }
            membership.oauth_clients = BTreeSet::from([client.clone()]);
            membership.principals.clear();
            membership.groups.clear();
            membership.roles.clear();
            true
        });
    }
    catalog.tenants.retain(|definition| definition.id == tenant);
    let mut policy = catalog
        .policies
        .iter()
        .find(|p| p.version == selected.policy_version)
        .context("base lacks selected policy")?
        .clone();
    policy.rules = reference
        .policies
        .iter()
        .flat_map(|p| &p.rules)
        .filter(|rule| {
            matches!(
                rule.id.as_str(),
                "allow_time_surface_read" | "allow_time_event_tools"
            )
        })
        .cloned()
        .map(|mut rule| {
            rule.profiles = BTreeSet::from([profile.clone()]);
            rule
        })
        .collect();
    anyhow::ensure!(
        policy.rules.len() == 2,
        "reference Time policy pair missing or ambiguous"
    );
    catalog.servers = vec![time];
    catalog.profiles = vec![selected];
    catalog.oauth_clients = vec![registration];
    catalog.oidc_clients.clear();
    // No Recording ingest, managed Agent or Computer declarations belong to this
    // single Time caller. Their owner sections cannot keep references to removed clients.
    catalog.extensions.clear();
    catalog.policies = vec![policy];
    catalog.secrets.retain(|secret| match &secret.owner {
        veoveo_gateway_contract::SecretOwner::Gateway => true,
        veoveo_gateway_contract::SecretOwner::Profile { profile } => catalog
            .profiles
            .iter()
            .any(|selected| &selected.id == profile),
        veoveo_gateway_contract::SecretOwner::Server { server } => catalog
            .servers
            .iter()
            .any(|selected| &selected.slug == server),
        veoveo_gateway_contract::SecretOwner::Tenant { tenant } => catalog
            .tenants
            .iter()
            .any(|selected| &selected.id == tenant),
    });
    // The stable root is admitted by the owner builder, not a guessed scheme.
    let _ = TimeResource::Events { cursor: None }.to_uri()?;
    let admission = veoveo_mcp_gateway::GatewayCatalogAdmission::unbound()
        .bind(veoveo_gateway_catalog::registry()?)?;
    veoveo_mcp_gateway::GatewayCatalog::from_control_plane(catalog.clone(), admission)
        .context("admit qualification Time catalog before preparing Store")?;
    Ok(catalog)
}

const MODEL_KEY_ENV: &str = "SMOKE_LLM_API_KEY";
const SIGNING_KEY_ENV: &str = "SMOKE_AGENT_PRIVATE_KEY_DER_B64";

fn qualification_manifest(
    catalog: &GatewayControlPlane,
    gateway_origin: &url::Url,
    model_origin: &url::Url,
    workspace: &Path,
) -> Result<veoveo_agent_kernel::manifest::AgentManifest> {
    use veoveo_agent_kernel::manifest::*;
    let selected = catalog
        .profiles
        .first()
        .context("qualification profile missing")?;
    let registration = catalog
        .oauth_clients
        .first()
        .context("qualification client missing")?;
    let tenant = registration
        .tenant
        .as_ref()
        .context("qualification client requires tenant")?;
    let migrations = workspace.join("migrations");
    fs::create_dir(&migrations)?;
    fs::write(
        migrations.join("0001_effects.sql"),
        "CREATE TABLE time_effects(event_id TEXT PRIMARY KEY, state TEXT NOT NULL, record_version TEXT NOT NULL);\n",
    )?;
    let manifest = AgentManifest {
        agent: AgentIdentity {
            tenant: tenant.to_string(),
            id: format!("time-events-{}", uuid::Uuid::now_v7()),
            display_name: "Time event qualification".into(),
        },
        model: ModelConfig {
            base_url: model_origin.join("v1")?.to_string(),
            api_key_env: MODEL_KEY_ENV.into(),
            model: "fake/time-events".into(),
            temperature: None,
            top_p: None,
            top_k: None,
            max_output_tokens: None,
        },
        gateway: GatewayAccess {
            url: PUBLIC_BASE_URL.into(),
            transport_url: gateway_origin.to_string(),
            profile: selected.id.to_string(),
            client_id: registration.id.to_string(),
            work_context: registration.default_work_context.to_string(),
            audience: url::Url::parse(PUBLIC_BASE_URL)?.join("oauth/token")?.to_string(),
            resource: selected.protected_resource.to_string(),
            scopes: selected.required_scopes.iter().map(ToString::to_string).collect(),
            private_key_env: SIGNING_KEY_ENV.into(),
            private_key_kid: "test-key".into(),
            token_refresh_fraction: 0.6,
        },
        episode: EpisodeConfig {
            max_turns: 24,
            request_timeout_s: 300,
            task_deadline_s: 60,
        },
        memory: MemoryConfig {
            memory_write_tables: vec!["time_effects".into()],
            ..Default::default()
        },
        context: Default::default(),
        budgets: Default::default(),
        schedule: ScheduleConfig {
            heartbeat_interval_s: 3600,
            wake_coalesce_window_ms: 100,
            ..Default::default()
        },
        resource_subscriptions: vec![ResourceSubscription {
            uri: TimeResource::Events { cursor: None }.to_uri()?.to_string(),
        }],
        migrations_dir: Some(migrations),
        preamble: "Read current Time event state. Only Due events authorize one retained semantic effect per event identity. This deterministic fixture does not qualify live model reasoning.".into(),
    };
    manifest
        .validate_with_environment_presence(|key| [MODEL_KEY_ENV, SIGNING_KEY_ENV].contains(&key))
        .context("admit qualification kernel manifest before preparing Store")?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qualification_catalog_binds_one_local_agent_and_explicit_time_grants() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let catalog = qualification_catalog(
            &root.join("configs/gateway.smoke.json"),
            &root.join("examples/bioma/gateway.json"),
            &url::Url::parse("http://127.0.0.1:18870").unwrap(),
        )
        .unwrap();
        assert_eq!(catalog.servers.len(), 1);
        assert_eq!(catalog.profiles.len(), 1);
        assert_eq!(catalog.oauth_clients.len(), 1);
        assert!(catalog.extensions.is_empty());
        assert_eq!(
            catalog.profiles[0].auth_modes,
            BTreeSet::from([AuthMode::OAuthClientCredentials])
        );
        // Restoring the base's unrelated ingest section recreates the real
        // dangling producer-client defect and must fail production admission.
        let base: GatewayControlPlane =
            serde_json::from_slice(&fs::read(root.join("configs/gateway.smoke.json")).unwrap())
                .unwrap();
        let mut unrelated = catalog.clone();
        unrelated.extensions = base.extensions;
        let admission = veoveo_mcp_gateway::GatewayCatalogAdmission::unbound()
            .bind(veoveo_gateway_catalog::registry().unwrap())
            .unwrap();
        assert!(
            veoveo_mcp_gateway::GatewayCatalog::from_control_plane(unrelated, admission).is_err()
        );
        let profile = &catalog.profiles[0];
        let client = &catalog.oauth_clients[0];
        assert_eq!(profile.id.as_str(), PROFILE);
        assert_eq!(client.id.as_str(), CLIENT);
        assert_eq!(
            client.allowed_resources,
            BTreeSet::from([profile.protected_resource.clone()])
        );
        assert_eq!(
            client.allowed_scopes,
            BTreeSet::from([
                "operator:use".parse().unwrap(),
                TimeScope::Read.into(),
                TimeScope::EventWrite.into()
            ])
        );
        assert!(catalog.work_contexts.iter().any(|context| {
            context
                .memberships
                .iter()
                .any(|membership| membership.oauth_clients.contains(&client.id))
        }));
        assert!(
            catalog.policies[0]
                .rules
                .iter()
                .all(|rule| rule.profiles == BTreeSet::from([profile.id.clone()]))
        );
        assert_eq!(
            catalog.servers[0].upstream.url.as_str(),
            "http://127.0.0.1:18870/time/mcp"
        );
    }
    #[test]
    fn qualification_manifest_admits_origin_and_refuses_path_or_missing_environment() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let catalog = qualification_catalog(
            &root.join("configs/gateway.smoke.json"),
            &root.join("examples/bioma/gateway.json"),
            &url::Url::parse("http://127.0.0.1:18870").unwrap(),
        )
        .unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let manifest = qualification_manifest(
            &catalog,
            &url::Url::parse("http://127.0.0.1:18871").unwrap(),
            &url::Url::parse("http://127.0.0.1:18872").unwrap(),
            workspace.path(),
        )
        .unwrap();
        let decoded: veoveo_agent_kernel::manifest::AgentManifest =
            serde_json::from_slice(&serde_json::to_vec(&manifest).unwrap()).unwrap();
        let present = |key: &str| [MODEL_KEY_ENV, SIGNING_KEY_ENV].contains(&key);
        decoded.validate_with_environment_presence(present).unwrap();
        assert_eq!(
            decoded.mcp_url(),
            "http://127.0.0.1:18871/mcp/agent-time-events"
        );
        let mut wrong = decoded.clone();
        wrong.gateway.transport_url = decoded.mcp_url();
        assert!(
            wrong
                .validate_with_environment_presence(present)
                .unwrap_err()
                .to_string()
                .contains("gateway.transport_url")
        );
        assert!(
            decoded
                .validate_with_environment_presence(|key| key != MODEL_KEY_ENV)
                .unwrap_err()
                .to_string()
                .contains("model.api_key_env")
        );
        assert!(
            decoded
                .validate_with_environment_presence(|key| key != SIGNING_KEY_ENV)
                .unwrap_err()
                .to_string()
                .contains("gateway.private_key_env")
        );
    }

    #[tokio::test]
    async fn caller_cleanup_survives_waiter_drop_and_refuses_expired_success() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        use veoveo_testing_support::lifecycle::owner::{self, CleanupKind};
        const MODE: &str = "VEOVEO_AGENT_TIME_CLEANUP_CONTROL";
        let Ok(mode) = std::env::var(MODE) else {
            let selector = std::thread::current().name().unwrap().to_owned();
            for mode in ["interrupted", "expired", "late_ok", "late_error"] {
                let result = std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", &selector, "--nocapture"])
                    .env(MODE, mode)
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "isolated caller cleanup mode {mode} failed: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
            }
            return;
        };
        let polled = Arc::new(AtomicUsize::new(0));
        let completed = Arc::new(AtomicUsize::new(0));
        let cleanup = Arc::new(tokio::sync::Mutex::new(CallerCleanup::default()));
        let retained = cleanup.clone();
        let began = Arc::new(tokio::sync::Notify::new());
        let resume = Arc::new(tokio::sync::Notify::new());
        let result: Result<()> = owner::run(async {
            owner::register_cleanup(
                CleanupKind::Remote,
                "agent_time_events_control",
                "original-sdk-close",
                move || async move { retained.lock().await.finish().await },
            )?;
            let polls = polled.clone();
            let finishes = completed.clone();
            let entered = began.clone();
            let gate = resume.clone();
            cleanup.lock().await.close = Some(Box::pin(async move {
                polls.fetch_add(1, Ordering::SeqCst);
                entered.notify_one();
                gate.notified().await;
                finishes.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }));
            if matches!(mode.as_str(), "late_ok" | "late_error") {
                let polls = polled.clone();
                let finishes = completed.clone();
                let late_error = mode == "late_error";
                cleanup.lock().await.deadline =
                    Some(std::time::Instant::now() + Duration::from_millis(100));
                cleanup.lock().await.close = Some(Box::pin(std::future::poll_fn(move |_| {
                    polls.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(150));
                    finishes.fetch_add(1, Ordering::SeqCst);
                    std::task::Poll::Ready(if late_error {
                        Err(anyhow!("original close failure"))
                    } else {
                        Ok(())
                    })
                })));
                let error = cleanup.lock().await.finish().await.unwrap_err();
                if late_error {
                    assert!(error.to_string().contains("original close failure"));
                }
                assert!(
                    cleanup.lock().await.finish().await.is_err(),
                    "late completion became success on retry"
                );
            } else if mode == "expired" {
                cleanup.lock().await.deadline =
                    Some(std::time::Instant::now() - Duration::from_secs(1));
                resume.notify_one(); // A ready success must not bypass the original cap.
                assert!(cleanup.lock().await.finish().await.is_err());
            } else {
                let retained_wait = cleanup.clone();
                let mut waiter = Box::pin(async move { retained_wait.lock().await.finish().await });
                tokio::select! {
                    _ = began.notified() => {},
                    outcome = &mut waiter => panic!("close unexpectedly settled: {outcome:?}"),
                }
                drop(waiter);
                resume.notify_one();
            }
            anyhow::bail!("fixture work interrupted with original caller still owned")
        })
        .await;
        assert!(result.is_err(), "cleanup cannot erase the work failure");
        if mode == "expired" {
            assert_eq!(polled.load(Ordering::SeqCst), 0);
            assert_eq!(completed.load(Ordering::SeqCst), 0);
            assert!(cleanup.lock().await.failed);
            assert!(cleanup.lock().await.close.is_some());
        } else {
            assert_eq!(
                polled.load(Ordering::SeqCst),
                1,
                "cleanup reconstructed the consuming future"
            );
            assert_eq!(
                completed.load(Ordering::SeqCst),
                1,
                "registered cleanup did not resume the owned close"
            );
            assert!(cleanup.lock().await.close.is_none());
            if mode.starts_with("late_") {
                assert!(cleanup.lock().await.failed);
            }
        }
    }

    fn event_fixture() -> veoveo_time_mcp::contract::TemporalEvent {
        use veoveo_time_mcp::contract::*;
        TemporalEvent {
            event_id: TemporalEventId::parse("event-fixture").unwrap(),
            name: "event binding fixture".into(),
            due: TimeInstant::from_total_nanoseconds(
                0,
                0,
                AuthorityBinding::new(
                    AuthorityReleaseId::parse("time-release-tzdb").unwrap(),
                    AuthorityReleaseId::parse("time-release-leaps").unwrap(),
                )
                .unwrap(),
            )
            .unwrap(),
            state: TemporalEventState::Scheduled,
            record_version: TimeVersion::FIRST,
        }
    }
    #[test]
    fn event_observations_bind_request_identity_payload_version_and_content_uri() {
        use veoveo_time_mcp::contract::*;
        let original = event_fixture();
        let request = CreateTemporalEventRequest {
            name: original.name.clone(),
            due: original.due.clone(),
            idempotency_key: "fixture".into(),
        };
        admit_created(&request, &original).unwrap();
        let mut wrong = original.clone();
        wrong.name = "another event".into();
        assert!(admit_created(&request, &wrong).is_err());
        wrong = original.clone();
        wrong.due.nanosecond = SubsecondNanoseconds::new(1).unwrap();
        assert!(admit_created(&request, &wrong).is_err());
        for state in [TemporalEventState::Due, TemporalEventState::Cancelled] {
            let mut current = original.clone();
            current.state = state;
            current.record_version = original.record_version.checked_next().unwrap();
            admit_event(&original, &current, state, current.record_version).unwrap();
            let mut wrong = current.clone();
            wrong.event_id = TemporalEventId::parse("event-other").unwrap();
            assert!(admit_event(&original, &wrong, state, current.record_version).is_err());
            wrong = current.clone();
            wrong.name = "another event".into();
            assert!(admit_event(&original, &wrong, state, current.record_version).is_err());
            wrong = current.clone();
            wrong.due.nanosecond = SubsecondNanoseconds::new(1).unwrap();
            assert!(admit_event(&original, &wrong, state, current.record_version).is_err());
            assert!(admit_event(&original, &current, state, original.record_version).is_err());
        }
        let root = smoke_tmpdir().unwrap();
        let journal = Journal::new(
            fs::File::create(root.join("events.jsonl")).unwrap(),
            root.clone(),
        );
        let requested = TimeResource::Event(original.event_id.clone())
            .to_uri()
            .unwrap();
        let wrong_uri = TimeResource::Event(TemporalEventId::parse("event-other").unwrap())
            .to_uri()
            .unwrap();
        let response = rmcp::model::ReadResourceResult::new(vec![ResourceContents::text(
            serde_json::to_string(&original).unwrap(),
            wrong_uri.as_str(),
        )]);
        let result: Result<TemporalEvent> = decode_current(&requested, response, |event| {
            journal.append(Observation::Event {
                phase: "wrong_uri",
                event,
            })
        });
        assert!(result.is_err());
        assert_eq!(
            journal.events()[0].event,
            original,
            "refused observation must stay owned"
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn journal_failure_retains_received_event_through_cancelled_waiter_and_cleanup() {
        const MODE: &str = "VEOVEO_AGENT_TIME_JOURNAL_CONTROL";
        if std::env::var_os(MODE).is_none() {
            let selector = std::thread::current().name().unwrap().to_owned();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", &selector, "--nocapture"])
                .env(MODE, "child")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "isolated journal control failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        use veoveo_testing_support::lifecycle::owner::{self, CleanupKind};
        let root = smoke_tmpdir().unwrap();
        let path = root.join("events.jsonl");
        fs::write(&path, []).unwrap();
        // Real descriptor refuses writes immediately after the typed receipt.
        let journal = Journal::new(fs::File::open(&path).unwrap(), root.clone());
        let original = event_fixture();
        let retained = journal.clone();
        let result: Result<()> = owner::run(async {
            owner::register_cleanup(CleanupKind::Remote,"agent_time_journal_control","received-event",move ||async move {retained.flush()})?;
            let work_journal = journal.clone();
            let event = original.clone();
            let notified = std::sync::Arc::new(tokio::sync::Notify::new());
            let observed = notified.clone();
            let mut waiter = Box::pin(async move {
                assert!(work_journal.append(Observation::Event {phase:"received",event:&event}).is_err());
                observed.notify_one();
                std::future::pending::<()>().await;
            });
            tokio::select! { _ = notified.notified() => {}, _ = &mut waiter => panic!("work unexpectedly settled"), }
            drop(waiter);
            anyhow::bail!("cancelled fixture work")
        }).await;
        assert!(result.is_err());
        assert_eq!(journal.events()[0].event, original);
        let snapshot = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("retained-events-")
            })
            .expect("owned cleanup did not preserve observations");
        let preserved: Vec<journal::RetainedEvent> =
            serde_json::from_slice(&fs::read(snapshot).unwrap()).unwrap();
        assert_eq!(preserved[0].event, original);
        assert!(
            journal.flush().is_err(),
            "successful preservation erased first persistence failure"
        );
        fs::remove_dir_all(root).unwrap();
    }
}

pub(crate) struct Binaries<'a> {
    pub gateway: &'a Path,
    pub time: &'a Path,
    pub artifact: &'a Path,
    pub agent: &'a Path,
}

#[path = "agent_time_events/journal.rs"]
mod journal;
use journal::{Journal, Observation};

type CloseFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>;
#[derive(Default)]
struct CallerCleanup {
    close: Option<CloseFuture>,
    deadline: Option<std::time::Instant>,
    failed: bool,
}
impl CallerCleanup {
    async fn finish(&mut self) -> Result<()> {
        use veoveo_testing_support::lifecycle::owner;
        if self.close.is_none() {
            anyhow::ensure!(!self.failed, "original agent event caller cleanup failed");
            return Ok(());
        }
        let cap = owner::cleanup_deadline()?;
        let end = *self.deadline.get_or_insert(cap);
        if std::time::Instant::now() >= end {
            self.failed = true;
            anyhow::bail!("original agent event caller cleanup deadline expired");
        }
        if let Some(close) = self.close.as_mut() {
            match tokio::time::timeout_at(end.into(), close).await {
                Ok(result) => {
                    self.close = None;
                    let late = std::time::Instant::now() >= end;
                    self.failed |= result.is_err() || late;
                    // Preserve an actual SDK close error, including a late error.
                    result?;
                    anyhow::ensure!(
                        !late,
                        "original agent event caller close completed after its deadline"
                    );
                }
                Err(_) => {
                    self.failed = true;
                    anyhow::bail!("original agent event caller close remains unresolved");
                }
            }
        }
        anyhow::ensure!(!self.failed, "original agent event caller cleanup failed");
        Ok(())
    }
}

pub(crate) async fn run(binaries: Binaries<'_>, base: &Path, reference: &Path) -> Result<()> {
    use veoveo_time_mcp::contract::{
        CancelTemporalEventRequest, ClockCurrent, CreateTemporalEventRequest, TemporalEvent,
        TemporalEventState, TimeInstant,
    };
    for executable in [
        binaries.gateway,
        binaries.time,
        binaries.artifact,
        binaries.agent,
    ] {
        assert_executable(executable)?;
    }
    let tmpdir = smoke_tmpdir()?;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    fs::set_permissions(&tmpdir, fs::Permissions::from_mode(0o700))?;
    println!("agent Time event workspace: {}", tmpdir.display());
    let journal = Journal::new(
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(tmpdir.join("events.jsonl"))?,
        tmpdir.clone(),
    );
    let retained_journal = journal.clone();
    veoveo_testing_support::lifecycle::owner::register_cleanup(
        veoveo_testing_support::lifecycle::owner::CleanupKind::Remote,
        "agent_time_events_observations",
        &tmpdir.to_string_lossy(),
        move || async move { retained_journal.flush() },
    )?;
    let time_base = url::Url::parse("http://127.0.0.1:18870")?;
    let gateway_base = url::Url::parse("http://127.0.0.1:18871")?;
    let model_base = url::Url::parse("http://127.0.0.1:18872")?;
    let catalog = qualification_catalog(base, reference, &time_base)?;
    let control_plane = tmpdir.join("gateway.json");
    fs::write(&control_plane, serde_json::to_vec_pretty(&catalog)?)?;
    let admitted_manifest = qualification_manifest(&catalog, &gateway_base, &model_base, &tmpdir)?;
    let manifest = tmpdir.join("manifest.json");
    fs::write(&manifest, serde_json::to_vec_pretty(&admitted_manifest)?)?;
    let data = tmpdir.join("agent");
    // Prepare one Gateway-owned composition before starting any service. The
    // artifact-only Store fixture prepares a different plan at the same generation.
    let platform = spawn_gateway_platform_store(binaries.gateway, &control_plane).await?;
    let artifact_port = reserve_local_port()?;
    let artifact_url = url::Url::parse(&format!("http://127.0.0.1:{artifact_port}"))?;
    let mut artifact_env = platform.runtime_env();
    artifact_env.extend([
        (
            "ARTIFACT_SERVICE_BIND",
            format!("127.0.0.1:{artifact_port}").into(),
        ),
        ("ARTIFACT_PUBLIC_BASE_URL", artifact_url.to_string().into()),
        ("VEOVEO_INTERNAL_TRUST_JWKS", INTERNAL_TRUST_JWKS.into()),
        ("ARTIFACT_STORE", "memory".into()),
        (
            "ARTIFACT_ALLOWED_AUDIENCES",
            "media,timeseries,optimization,duckdb,frames,map,datasheet".into(),
        ),
    ]);
    let _artifact = ChildGuard::spawn(
        binaries.artifact,
        Vec::<OsString>::new(),
        artifact_env,
        &tmpdir.join("artifact.log"),
    )?;
    wait_for_http(artifact_url.join("healthz")?.as_str()).await?;
    let support = veoveo_testing_support::artifacts::executable(
        "veoveo-gateway-composition",
        "gateway-smoke-support",
    )?;
    let signing = run_checked(&support, ["gateway-private-key-der-b64".into()], [])?;
    let spawn_time = || -> Result<ChildGuard> {
        let mut env = platform.runtime_env();
        env.push(("VEOVEO_INTERNAL_TRUST_JWKS", INTERNAL_TRUST_JWKS.into()));
        ChildGuard::spawn(
            binaries.time,
            [
                "--port".into(),
                "18870".into(),
                "--public-base-url".into(),
                PUBLIC_BASE_URL.into(),
                "--allow-loopback-hosts".into(),
                "--acquisition-scratch-root".into(),
                tmpdir.join("acquisitions").into_os_string(),
                "--release-root".into(),
                tmpdir.join("releases").into_os_string(),
            ],
            env,
            &tmpdir.join("time.log"),
        )
    };
    let mut time = spawn_time()?;
    wait_for_http(time_base.join("time/readyz")?.as_str()).await?;
    let mut gateway = ChildGuard::spawn(
        binaries.gateway,
        gateway_serve_args(18871, &platform),
        [
            (
                "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
                INTERNAL_SIGNING_KEY_DER_B64.into(),
            ),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                signing.trim().into(),
            ),
        ],
        &tmpdir.join("gateway.log"),
    )?;
    wait_for_http(gateway_base.join("healthz")?.as_str()).await?;
    let model_ready = tmpdir.join("model.ready");
    let model_log = tmpdir.join("model.log");
    let mut model = ChildGuard::spawn(
        &veoveo_testing_support::artifacts::executable("veoveo-agent-kernel", "agent-smoke")?,
        [
            "fake-openai-llm".into(),
            "--port".into(),
            "18872".into(),
            "--ready-file".into(),
            model_ready.clone().into_os_string(),
        ],
        [],
        &model_log,
    )
    .map(ChildGuard::with_owner_scope)?;
    wait_for_file(&model_ready).await?;
    let selected = &catalog.profiles[0];
    let registration = &catalog.oauth_clients[0];
    let mut transport = gateway_base.clone();
    transport.set_path(&format!("/mcp/{PROFILE}"));
    let spawn_agent = || -> Result<ChildGuard> {
        let mut env = platform.runtime_env();
        env.extend([
            (MODEL_KEY_ENV, "fixture".into()),
            (SIGNING_KEY_ENV, signing.trim().into()),
        ]);
        ChildGuard::spawn(
            binaries.agent,
            [
                "run".into(),
                "--manifest".into(),
                manifest.clone().into_os_string(),
                "--data-dir".into(),
                data.clone().into_os_string(),
            ],
            env,
            &tmpdir.join("agent.log"),
        )
    };
    let mut agent = spawn_agent()?;
    let mut token_args = vec![
        "gateway-token-exchange".into(),
        "--token-url".into(),
        gateway_base.join("oauth/token")?.to_string().into(),
        "--client-id".into(),
        registration.id.as_str().into(),
        "--audience".into(),
        format!("{PUBLIC_BASE_URL}/oauth/token").into(),
        "--resource".into(),
        selected.protected_resource.as_str().into(),
        "--work-context".into(),
        registration.default_work_context.as_str().into(),
    ];
    for scope in &selected.required_scopes {
        token_args.extend(["--scope".into(), scope.as_str().into()]);
    }
    let token = run_checked(&support, token_args, [])?;
    use veoveo_testing_support::lifecycle::owner::{self, CleanupKind};
    let caller_cleanup = std::sync::Arc::new(tokio::sync::Mutex::new(CallerCleanup::default()));
    let retained_cleanup = caller_cleanup.clone();
    let caller_registration = owner::register_cleanup(
        CleanupKind::Remote,
        "agent_time_events_caller",
        transport.as_str(),
        move || async move { retained_cleanup.lock().await.finish().await },
    )?;
    let client = connect_mcp_client(transport.as_str(), token.trim()).await?;
    let caller = client.peer().clone();
    // Transfer the actual SDK handle synchronously before the next await.
    caller_cleanup
        .try_lock()
        .expect("uncontended new caller cleanup")
        .close = Some(Box::pin(async move { client.cancel().await }));
    // Startup remains inside the maintained command owner. This interval covers
    // authenticated event work and explicit client/process cleanup; the outer
    // owner can cancel it earlier and drains registered acquisitions.
    let end = tokio::time::Instant::now() + Duration::from_secs(300);
    let work_end = end - Duration::from_secs(20);
    let phases = tokio::time::timeout_at(work_end, async {
        // This marker requires a real post-ACK resource notification and an
        // authoritative read by the actual kernel episode through Gateway.
        wait_for_log_occurrences(&model_log, "time fixture: current events not due", 1, 45).await?;
        let create =
            |name: &str, clock: ClockCurrent, offset: i64| -> Result<CreateTemporalEventRequest> {
                let now = clock.time.instant();
                let total = now
                    .total_nanoseconds()
                    .checked_add(i128::from(offset) * 1_000_000_000)
                    .context("event due overflow")?;
                Ok(CreateTemporalEventRequest {
                    name: name.into(),
                    due: TimeInstant::from_total_nanoseconds(
                        total,
                        now.uncertainty_nanoseconds,
                        now.authority.clone(),
                    )?,
                    idempotency_key: uuid::Uuid::now_v7().to_string(),
                })
            };
        let live = create(
            "live-due",
            read(&caller, TimeResource::ClockCurrent).await?,
            20,
        )?;
        journal.append(Observation::CreateIntent { request: &live })?;
        let first: TemporalEvent = event_call(&caller, "create_temporal_event", &live).await?;
        journal.append(Observation::Event {
            phase: "created",
            event: &first,
        })?;
        admit_created(&live, &first)?;
        wait_for_log_occurrences(
            &model_log,
            &format!("time fixture: observed scheduled {}", first.event_id),
            1,
            15,
        )
        .await?;
        wait_for_log_occurrences(
            &model_log,
            &format!("time fixture: due effect {}", first.event_id),
            1,
            45,
        )
        .await?;
        let due = read_event(&caller, &first, &journal, "live_due").await?;
        admit_event(
            &first,
            &due,
            TemporalEventState::Due,
            first.record_version.checked_next()?,
        )?;
        let cancel = create(
            "cancelled",
            read(&caller, TimeResource::ClockCurrent).await?,
            120,
        )?;
        journal.append(Observation::CreateIntent { request: &cancel })?;
        let scheduled: TemporalEvent =
            event_call(&caller, "create_temporal_event", &cancel).await?;
        journal.append(Observation::Event {
            phase: "before_cancel",
            event: &scheduled,
        })?;
        admit_created(&cancel, &scheduled)?;
        let cancel_request = CancelTemporalEventRequest {
            event_id: scheduled.event_id.clone(),
            expected_record_version: scheduled.record_version,
        };
        journal.append(Observation::CancelIntent {
            request: &cancel_request,
        })?;
        let cancelled: TemporalEvent =
            event_call(&caller, "cancel_temporal_event", &cancel_request).await?;
        journal.append(Observation::Event {
            phase: "cancelled",
            event: &cancelled,
        })?;
        admit_event(
            &scheduled,
            &cancelled,
            TemporalEventState::Cancelled,
            scheduled.record_version.checked_next()?,
        )?;
        wait_for_log_occurrences(
            &model_log,
            &format!("time fixture: observed cancelled {}", cancelled.event_id),
            1,
            30,
        )
        .await?;
        let current_cancelled =
            read_event(&caller, &cancelled, &journal, "current_cancelled").await?;
        admit_event(
            &cancelled,
            &current_cancelled,
            TemporalEventState::Cancelled,
            cancelled.record_version,
        )?;
        let duplicate_marker = format!("time fixture: due effect {}", first.event_id);
        let duplicate_count = fs::read_to_string(&model_log)?
            .matches(&duplicate_marker)
            .count();
        let replay: TemporalEvent = event_call(&caller, "create_temporal_event", &live).await?;
        journal.append(Observation::Event {
            phase: "duplicate",
            event: &replay,
        })?;
        admit_event(&due, &replay, TemporalEventState::Due, due.record_version)?;
        wait_for_log_occurrences(&model_log, &duplicate_marker, duplicate_count + 1, 30).await?;
        let restart = create(
            "listener-gap",
            read(&caller, TimeResource::ClockCurrent).await?,
            20,
        )?;
        journal.append(Observation::CreateIntent { request: &restart })?;
        let gap: TemporalEvent = event_call(&caller, "create_temporal_event", &restart).await?;
        journal.append(Observation::Event {
            phase: "before_time_restart",
            event: &gap,
        })?;
        admit_created(&restart, &gap)?;
        journal.append(Observation::ReplacementIntent { component: "time" })?;
        time.stop_checked().await?;
        tokio::time::sleep(Duration::from_secs(22)).await;
        time = spawn_time()?;
        wait_for_http(time_base.join("time/readyz")?.as_str()).await?;
        wait_for_log_occurrences(
            &model_log,
            &format!("time fixture: due effect {}", gap.event_id),
            1,
            45,
        )
        .await?;
        let restored = read_event(&caller, &gap, &journal, "after_time_restart").await?;
        admit_event(
            &gap,
            &restored,
            TemporalEventState::Due,
            gap.record_version.checked_next()?,
        )?;
        let restored_marker = format!("time fixture: due effect {}", gap.event_id);
        let restored_count = fs::read_to_string(&model_log)?
            .matches(&restored_marker)
            .count();
        journal.append(Observation::ReplacementIntent {
            component: "kernel",
        })?;
        agent.stop_checked().await?;
        let before = wakes(&platform).await?;
        anyhow::ensure!(
            before.iter().any(|wake| wake.acked_by_episode.is_some()),
            "kernel replacement has no acknowledged wake/episode identity to retain"
        );
        journal.append(Observation::Wakes {
            phase: "before_kernel_restart",
            wakes: &before,
        })?;
        agent = spawn_agent()?;
        wait_for_log_occurrences(&model_log, &restored_marker, restored_count + 1, 45).await?;
        agent.stop_checked().await?;
        let after = wakes(&platform).await?;
        for old in &before {
            anyhow::ensure!(
                after.iter().any(|new| new.id == old.id
                    && new.created_at == old.created_at
                    && old
                        .acked_by_episode
                        .as_ref()
                        .is_none_or(|episode| new.acked_by_episode.as_ref() == Some(episode))),
                "kernel replacement changed retained wake/episode identity"
            );
        }
        anyhow::ensure!(
            after.len() > before.len(),
            "kernel replacement produced no current-resource reconciliation wake"
        );
        journal.append(Observation::Wakes {
            phase: "after_kernel_restart",
            wakes: &after,
        })?;
        let memory = duckdb::Connection::open(data.join("memory.duckdb"))?;
        let mut statement =
            memory.prepare("SELECT event_id,state FROM time_effects ORDER BY event_id")?;
        let effects = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let expected = BTreeSet::from([first.event_id.to_string(), gap.event_id.to_string()]);
        anyhow::ensure!(
            effects.len() == 2
                && effects.iter().map(|r| r.0.clone()).collect::<BTreeSet<_>>() == expected
                && effects.iter().all(|r| r.1 == "due"),
            "ledger semantic effects differ from independently observed Due identities"
        );
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("agent Time event work deadline")
    .and_then(|result| result);
    let cleanup = tokio::time::timeout_at(end, async {
        let journal_result = journal.flush();
        let caller_result = caller_cleanup.lock().await.finish().await;
        let registration_result = if caller_result.is_ok() {
            caller_registration.settled()
        } else {
            Ok(())
        };
        let agent_result = agent.stop_checked().await;
        let model_result = model.stop_checked().await;
        let gateway_result = gateway.stop_checked().await;
        let time_result = time.stop_checked().await;
        journal_result
            .and(caller_result)
            .and(registration_result)
            .and(agent_result)
            .and(model_result)
            .and(gateway_result)
            .and(time_result)
    })
    .await
    .context("agent Time event cleanup deadline");
    match (phases, cleanup) {
        (Ok(()), Ok(Ok(()))) => {
            journal.append(Observation::Passed { effects: 2 })?;
            println!(
                "agent Time event evidence: {}",
                tmpdir.join("events.jsonl").display()
            );
            Ok(())
        }
        (Err(work), Ok(Ok(()))) => Err(work),
        (work, cleanup) => Err(anyhow!(
            "agent Time event scenario/cleanup failed: work={work:?}; cleanup={cleanup:?}"
        )),
    }
}

fn admit_created(
    request: &veoveo_time_mcp::contract::CreateTemporalEventRequest,
    event: &veoveo_time_mcp::contract::TemporalEvent,
) -> Result<()> {
    anyhow::ensure!(
        event.name == request.name && event.due == request.due,
        "created event immutable payload differs from its request"
    );
    anyhow::ensure!(
        event.state == veoveo_time_mcp::contract::TemporalEventState::Scheduled
            && event.record_version == veoveo_time_mcp::contract::TimeVersion::FIRST,
        "creation must acknowledge the initial Scheduled event"
    );
    Ok(())
}
fn admit_event(
    original: &veoveo_time_mcp::contract::TemporalEvent,
    current: &veoveo_time_mcp::contract::TemporalEvent,
    state: veoveo_time_mcp::contract::TemporalEventState,
    version: veoveo_time_mcp::contract::TimeVersion,
) -> Result<()> {
    anyhow::ensure!(
        current.event_id == original.event_id
            && current.name == original.name
            && current.due == original.due,
        "event observation changed identity or immutable payload"
    );
    anyhow::ensure!(
        current.state == state && current.record_version == version,
        "event observation state/version differs from the retained transition"
    );
    Ok(())
}
fn decode_current<T: serde::de::DeserializeOwned>(
    requested: &veoveo_types::ResourceUri,
    result: rmcp::model::ReadResourceResult,
    observe: impl FnOnce(&T) -> Result<()>,
) -> Result<T> {
    anyhow::ensure!(
        result.contents.len() == 1,
        "current Time read requires exactly one text body"
    );
    let rmcp::model::ResourceContents::TextResourceContents { uri, text, .. } = &result.contents[0]
    else {
        anyhow::bail!("current Time read requires text contents");
    };
    let value = serde_json::from_str(text).context("decode current Time owner resource")?;
    observe(&value)?;
    anyhow::ensure!(
        uri.as_str() == requested.as_str(),
        "current Time content URI differs from request"
    );
    Ok(value)
}
async fn read_event(
    caller: &rmcp::Peer<rmcp::RoleClient>,
    original: &veoveo_time_mcp::contract::TemporalEvent,
    journal: &Journal,
    phase: &'static str,
) -> Result<veoveo_time_mcp::contract::TemporalEvent> {
    caller.clear_response_cache().await;
    let uri = TimeResource::Event(original.event_id.clone()).to_uri()?;
    let response = caller
        .read_resource(ReadResourceRequestParams::new(uri.as_str()))
        .await?;
    decode_current(&uri, response, |event| {
        journal.append(Observation::Event { phase, event })
    })
}

async fn event_call<I: serde::Serialize, O: serde::de::DeserializeOwned>(
    caller: &rmcp::Peer<rmcp::RoleClient>,
    name: &str,
    input: &I,
) -> Result<O> {
    let tool =
        veoveo_gateway_contract::GatewayToolName::from_parts(&"time".parse()?, &name.parse()?)?;
    veoveo_testing_support::installed::tools::call(caller, tool, input).await
}
async fn read<T: serde::de::DeserializeOwned>(
    caller: &rmcp::Peer<rmcp::RoleClient>,
    address: TimeResource,
) -> Result<T> {
    caller.clear_response_cache().await;
    let uri = address.to_uri()?;
    let response = caller
        .read_resource(ReadResourceRequestParams::new(uri.as_str()))
        .await?;
    decode_current(&uri, response, |_: &T| Ok(()))
}
async fn wakes(
    platform: &PlatformStoreSmoke,
) -> Result<Vec<veoveo_agent_runtime::persistence::WakeRecord>> {
    use veoveo_platform_store::{PlatformStore, StoreConfig, StoreCredentials};
    let store = PlatformStore::connect(
        StoreConfig::builder(
            &platform.endpoint,
            &platform.namespace,
            &platform.database,
            StoreCredentials::database(SURREAL_RUNTIME_USER, SURREAL_RUNTIME_PASSWORD),
        )
        .build()?,
    )
    .await?;
    let mut result = store
        .client()
        .query(include_str!(
            "../queries/agent_kernel/select_from_wake_where.surql"
        ))
        .await?
        .check()?;
    let wakes: Vec<veoveo_agent_runtime::persistence::WakeRecord> = result.take(0)?;
    anyhow::ensure!(wakes.len()<=128 &&wakes.iter().all(|wake|matches!(&wake.payload,veoveo_agent_runtime::persistence::WakePayload::ResourceChanged{uri} if uri.as_str()=="time://events")),"qualification wake corpus escaped its selected resource");
    Ok(wakes)
}

/// Observe only an owned fixture's diagnostics; this does not poll a provider.
async fn wait_for_log_occurrences(
    file: &Path,
    needle: &str,
    expected: usize,
    seconds: u64,
) -> Result<()> {
    let end = tokio::time::Instant::now() + Duration::from_secs(seconds);
    loop {
        if fs::read_to_string(file)
            .unwrap_or_default()
            .matches(needle)
            .count()
            >= expected
        {
            return Ok(());
        }
        anyhow::ensure!(
            tokio::time::Instant::now() < end,
            "owned model did not deliver the required phase marker"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
