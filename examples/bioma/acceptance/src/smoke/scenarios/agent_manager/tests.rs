//! CPU controls for the installed owner client; no Manager/provider is launched.
use super::*;
use std::collections::BTreeMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn fixture() -> Result<input::Input> {
    let sha = digest(b"fixture");
    let owner = Uuid::now_v7();
    serde_json::from_value(serde_json::json!({
        "schema":"veoveo.ai/agent-manager-journey-input/v1","callerTokenFile":"/private/token","workContext":"operations","owner":owner,
        "controller":{"namespace":"veoveo-agents","deployment":"veoveo-agent-manager","uid":Uuid::now_v7(),"image":format!("registry.test/manager@{sha}"),"configMap":"manager","configSha256":sha},
        "modulePlan":{"gatewayDeployment":"mcp-gateway","gatewayUid":Uuid::now_v7(),"configMap":"module-plan","sha256":sha,"generation":"1","credentialRevision":"fixture-runtime-1"},
        "template":{"id":"idle","name":"Idle","tenant":"bioma","workContexts":["operations"],"requiredDeployerScopes":["operator:use"],"profile":"agent","scopes":["operator:use"],"roles":["uav-pilot"],"membership":"contributor","models":["pilot-model"],"tools":[],"resourceSubscriptions":[],"parameters":{},"workload":{"namespace":"veoveo-agents","configMap":"idle-template","configDigest":sha,"image":format!("registry.test/kernel@{sha}"),"databaseSecret":"runtime","storageClass":"local-path","storageGib":1,"cpuMillis":500,"memoryMib":1024,"modelSecrets":[{"reference":"model-key","secret":"model","key":"api-key"}]}},
        "model":{"id":"pilot-model","revision":sha},"definition":"qualification-idle","instance":"qualification-idle-one","timeoutSeconds":240
    })).map_err(Into::into)
}
fn idle() -> BTreeMap<String, String> {
    BTreeMap::from([("manifest.json".into(),serde_json::json!({"agent":{},"model":{},"gateway":{},"episode":{},"schedule":{"heartbeatIntervalS":3600},"resourceSubscriptions":[],"preamble":"idle"}).to_string())])
}
fn instance(input: &input::Input) -> wire::ManagedInstance {
    wire::ManagedInstance {
        id: input.instance.clone(),
        name: "Idle".into(),
        definition: input.definition.clone(),
        owner: input.owner,
        work_context: input.work_context.clone(),
        template: input.template.id.clone(),
        requested_revision: digest(b"published"),
        active_revision: Some(digest(b"published")),
        generation: 1,
        active_generation: 1,
        desired: wire::InstanceDesired::Running,
        observed: wire::InstancePhase::Ready,
        client_id: veoveo_gateway_contract::OAuthClientId::parse("fixture-agent")
            .expect("static client"),
        principal: Uuid::now_v7(),
        storage_gib: 1,
        operation: Uuid::now_v7(),
        updated_at: chrono::Utc::now(),
    }
}
fn operation(instance: &wire::ManagedInstance) -> wire::LifecycleOperation {
    wire::LifecycleOperation {
        id: instance.operation,
        instance: instance.id.clone(),
        generation: instance.generation,
        phase: instance.observed,
        message: None,
        updated_at: instance.updated_at,
    }
}
#[test]
fn agent_manager_idle_input_and_typed_sse_refuse_actionable_or_foreign_facts() -> Result<()> {
    let input = fixture()?;
    let target = InstallationTarget::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../installation-target.json"),
    )?;
    input.validate(&target)?;
    input::admit_idle(&idle(), 240)?;
    ensure!(input::admit_idle(&idle(), 3600).is_err());
    let mut data = idle();
    data.insert("0001_wake.sql".into(), "SELECT true".into());
    ensure!(input::admit_idle(&data, 240).is_err());
    for patch in [
        serde_json::json!({"prompt":"do work"}),
        serde_json::json!({"resourceSubscriptions":["time://contract"]}),
    ] {
        let mut manifest: serde_json::Value = serde_json::from_str(&idle()["manifest.json"])?;
        manifest
            .as_object_mut()
            .expect("manifest")
            .extend(patch.as_object().expect("patch").clone());
        ensure!(
            input::admit_idle(
                &BTreeMap::from([("manifest.json".into(), manifest.to_string())]),
                240
            )
            .is_err()
        );
    }
    let mut wrong = fixture()?;
    wrong.controller.namespace = "other".into();
    ensure!(wrong.validate(&target).is_err());
    let mut wrong = fixture()?;
    wrong.work_context = WorkContextId::parse("foreign")?;
    ensure!(wrong.validate(&target).is_err());
    let mut wrong = fixture()?;
    wrong.timeout_seconds = 301;
    ensure!(wrong.validate(&target).is_err());
    let revision = digest(b"view");
    let event = sse_stream::Sse::default()
        .event("change")
        .id(revision.as_str())
        .data(serde_json::to_string(&wire::CatalogWake {
            revision: revision.clone(),
        })?);
    ensure!(admit_event(&event)? == Some(revision));
    for event in [
        sse_stream::Sse {
            id: None,
            ..event.clone()
        },
        sse_stream::Sse {
            id: Some(digest(b"other").to_string()),
            ..event.clone()
        },
        sse_stream::Sse::default().event("expired"),
        sse_stream::Sse::default()
            .event("other")
            .data("private-synthetic-marker"),
        sse_stream::Sse::default()
            .event("change")
            .data("private-synthetic-marker"),
    ] {
        ensure!(admit_event(&event).is_err());
        ensure!(
            !admit_event(&event)
                .unwrap_err()
                .to_string()
                .contains("private-synthetic-marker")
        );
    }
    ensure!(admit_event(&sse_stream::Sse::default())?.is_none());
    let mut value = instance(&input);
    admit_instance(&input, &value)?;
    value.owner = Uuid::now_v7();
    ensure!(admit_instance(&input, &value).is_err());
    value = instance(&input);
    value.id = wire::AgentManagedInstanceId::parse("foreign")?;
    ensure!(admit_instance(&input, &value).is_err());
    Ok(())
}
async fn read_request(socket: &mut tokio::net::TcpStream) -> Result<(String, Vec<u8>)> {
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        ensure!(socket.read(&mut byte).await? == 1, "test request ended");
        header.push(byte[0]);
        ensure!(header.len() <= 16 * 1024, "test header cap");
    }
    let text = std::str::from_utf8(&header)?;
    let length = text
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .and_then(|s| s.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    ensure!(length <= 65536, "test body cap");
    let mut body = vec![0; length];
    socket.read_exact(&mut body).await?;
    Ok((text.lines().next().context("test method")?.into(), body))
}
async fn respond(socket: &mut tokio::net::TcpStream, value: &impl Serialize) -> Result<()> {
    let body = serde_json::to_vec(value)?;
    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await?;
    socket.write_all(&body).await?;
    socket.shutdown().await?;
    Ok(())
}
fn client() -> Result<reqwest::Client> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}
fn owner(
    input: input::Input,
    url: url::Url,
    journal: Arc<PrivateCallerJournal>,
) -> Result<Journey> {
    Ok(Journey::new(
        input,
        client()?,
        url,
        journal,
        Instant::now() + Duration::from_secs(5),
    ))
}
#[tokio::test]
async fn agent_manager_lost_dispatch_receipt_reconciles_once_and_rejects_stale_generation()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let journal = Arc::new(PrivateCallerJournal::create(
        &root.path().join("journal.jsonl"),
    )?);
    let input = fixture()?;
    let value = instance(&input);
    let receipt = operation(&value);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let mut journey = owner(
        input,
        url::Url::parse(&format!("http://{}/admin/admin/", listener.local_addr()?))?,
        journal,
    )?;
    journey.published_revision = Some(value.requested_revision.clone());
    let request = wire::ProvisionInstance {
        request_id: Uuid::now_v7(),
        id: journey.input.instance.clone(),
        name: "Idle".into(),
        definition: journey.input.definition.clone(),
        revision: value.requested_revision.clone(),
    };
    journey.intent(
        Action::Provision,
        request.request_id,
        Mutation::Provision(&request),
    )?;
    let served = value.clone();
    let received = request.clone();
    let server = tokio::spawn(async move {
        for phase in 0..5 {
            let (mut socket, _) = listener.accept().await?;
            let (line, body) = read_request(&mut socket).await?;
            match phase {
                0 => {
                    ensure!(line == "POST /admin/admin/agent-instances HTTP/1.1");
                    let actual: wire::ProvisionInstance = serde_json::from_slice(&body)?;
                    ensure!(actual.request_id == received.request_id && actual.id == received.id);
                    socket.shutdown().await?;
                }
                1 | 3 => {
                    ensure!(line.starts_with("GET /admin/admin/agent-instances/"));
                    let mut current = served.clone();
                    if phase == 3 {
                        current.generation = 2;
                    }
                    respond(&mut socket, &current).await?;
                }
                2 => respond(&mut socket, &receipt).await?,
                4 => respond(&mut socket, &served).await?,
                _ => unreachable!(),
            }
        }
        Ok::<_, anyhow::Error>(())
    });
    ensure!(
        journey
            .mutate::<wire::LifecycleOperation>(
                reqwest::Method::POST,
                &["agent-instances"],
                &request,
                Action::Provision,
                request.request_id
            )
            .await
            .is_err()
    );
    journey.refresh_instance().await?;
    ensure!(
        journey
            .intent(
                Action::Provision,
                request.request_id,
                Mutation::Provision(&request)
            )
            .is_err(),
        "uncertain dispatch was replayed"
    );
    ensure!(
        journey.refresh_instance().await.is_err(),
        "stale/unowned generation accepted"
    );
    // A contradictory acknowledged operation also refuses current otherwise-valid data.
    journey.expected_operation = Some(Uuid::now_v7());
    ensure!(journey.refresh_instance().await.is_err());
    tokio::time::timeout(Duration::from_secs(4), server).await???;
    let report = std::fs::read_to_string(root.path().join("journal.jsonl"))?;
    ensure!(report.contains("uncertain") && report.contains(&request.request_id.to_string()));
    ensure!(
        report
            .lines()
            .filter(|l| l.contains("\"phase\":\"intent\""))
            .count()
            == 1
    );
    Ok(())
}
#[test]
fn agent_manager_registered_cleanup_closes_actual_sse_on_error_cancel_and_expired_cap() -> Result<()>
{
    const MODE: &str = "VEOVEO_AGENT_MANAGER_NATIVE_MODE";
    if let Ok(mode) = std::env::var(MODE) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        return runtime.block_on(async {
            let root=tempfile::tempdir()?;
            let journal=Arc::new(PrivateCallerJournal::create(&root.path().join("journal.jsonl"))?);
            let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let state=Arc::new(tokio::sync::Mutex::new(owner(fixture()?,url::Url::parse(&format!("http://{}/admin/admin/",listener.local_addr()?))?,journal)?));
            let server=tokio::spawn(async move {
                let (mut socket,_)=listener.accept().await?;let (line,_)=read_request(&mut socket).await?;ensure!(line=="GET /admin/admin/agent-events HTTP/1.1");
                socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").await?;
                let revision=digest(b"initial");
                let event=sse_stream::Sse::default().event("change").id(revision.as_str()).data(serde_json::to_string(&wire::CatalogWake { revision })?);
                socket.write_all(&bytes::Bytes::from(event)).await?;
                let mut byte=[0];ensure!(tokio::time::timeout(Duration::from_secs(4),socket.read(&mut byte)).await??==0,"owned SSE body stayed live"); Ok::<_,anyhow::Error>(())
            });
            let retained=state.clone();let selected=state.clone();
            let result:Result<()>=owner::run(async {
                owner::register_cleanup(CleanupKind::Remote,"agent-manager-native","actual-stream",move||async move { retained.lock().await.finish().await })?;
                selected.lock().await.arm().await?;
                if mode=="cancel" {
                    ensure!(std::process::Command::new("kill").args(["-TERM",&std::process::id().to_string()]).status()?.success());
                    std::future::pending::<Result<()>>().await
                } else if mode=="expired" {
                    selected.lock().await.cleanup_cap=Some(Instant::now()-Duration::from_secs(1));
                    Err(anyhow!("controlled operation failure"))
                } else { Err(anyhow!("controlled operation failure")) }
            }).await;
            ensure!(result.is_err());
            tokio::time::timeout(Duration::from_secs(5),server).await???;
            let mut state=state.lock().await;ensure!(state.events.is_none() && state.client.is_none());
            if mode=="expired" { ensure!(state.failed && state.finish().await.is_err(),"expired close was later promoted to success"); } else { ensure!(state.cleanup_done && !state.failed); }
            Ok(())
        });
    }
    for mode in ["error", "cancel", "expired"] {
        let directory = tempfile::tempdir()?;
        std::fs::create_dir(directory.path().join("groups"))?;
        let deadline = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
            + 10_000;
        let output = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", "agent_manager::journey::tests::agent_manager_registered_cleanup_closes_actual_sse_on_error_cancel_and_expired_cap", "--nocapture"])
            .env(MODE, mode)
            .env("VEOVEO_SMOKE_DEADLINE_UNIX_MS", deadline.to_string())
            .env("VEOVEO_SMOKE_CLEANUP_SECONDS", "2")
            .env("VEOVEO_SMOKE_LOCAL_GROUPS", directory.path().join("groups"))
            .output()?;
        ensure!(
            output.status.success(),
            "isolated Manager cleanup control failed"
        );
    }
    Ok(())
}

fn original_create(input: &input::Input) -> wire::CreateDefinition {
    wire::CreateDefinition {
        request_id: Uuid::now_v7(),
        id: input.definition.clone(),
        name: "Idle".into(),
        description: "Owned".into(),
        source: wire::DefinitionSource::Blank {
            content: wire::Content {
                model: input.model.clone(),
                instructions: "Idle".into(),
                tools: vec![],
                budgets: wire::Budgets {
                    max_output_tokens: 64,
                    max_completion_calls: 1,
                    max_tool_calls: 0,
                    deadline_seconds: 30,
                },
                execution: wire::Execution::Managed {
                    template: input.template.id.clone(),
                    template_revision: input.template_revision(),
                    parameters: Default::default(),
                    resource_subscriptions: vec![],
                },
            },
        },
    }
}

#[tokio::test]
async fn agent_manager_initial_sse_then_eof_cannot_qualify_fast_ready_or_archived() -> Result<()> {
    for phase in [wire::InstancePhase::Ready, wire::InstancePhase::Archived] {
        let root = tempfile::tempdir()?;
        let input = fixture()?;
        let mut value = instance(&input);
        value.observed = phase;
        let receipt = operation(&value);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mut journey = owner(
            input,
            url::Url::parse(&format!("http://{}/admin/admin/", listener.local_addr()?))?,
            Arc::new(PrivateCallerJournal::create(
                &root.path().join("journal.jsonl"),
            )?),
        )?;
        journey.expected_generation = Some(1);
        journey.published_revision = Some(value.requested_revision.clone());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await?;
            let (line, _) = read_request(&mut socket).await?;
            ensure!(line.contains("agent-events"));
            let revision = digest(b"initial");
            let event = sse_stream::Sse::default()
                .event("change")
                .id(revision.as_str())
                .data(serde_json::to_string(&wire::CatalogWake { revision })?);
            let bytes = bytes::Bytes::from(event);
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",bytes.len()).as_bytes()).await?;
            socket.write_all(&bytes).await?;
            socket.shutdown().await?;
            // The old GET-first path could qualify these immediately settled views.
            let mut reads = 0;
            while let Ok(Ok((mut socket, _))) =
                tokio::time::timeout(Duration::from_millis(150), listener.accept()).await
            {
                let (line, _) = read_request(&mut socket).await?;
                if line.contains("agent-operations") {
                    respond(&mut socket, &receipt).await?;
                } else {
                    respond(&mut socket, &value).await?;
                }
                reads += 1;
            }
            Ok::<_, anyhow::Error>(reads)
        });
        journey.arm().await?;
        ensure!(
            journey.await_phase(phase, 1).await.is_err(),
            "initial wake alone qualified terminal view"
        );
        ensure!(
            tokio::time::timeout(Duration::from_secs(3), server).await??? == 0,
            "current view read before delivered post-dispatch change"
        );
    }
    Ok(())
}

#[tokio::test]
async fn agent_manager_conflicting_create_wrong_draft_never_dispatches_destructive_cleanup()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let input = fixture()?;
    let request = original_create(&input);
    let definition = wire::Definition {
        id: input.definition.clone(),
        name: request.name.clone(),
        description: request.description.clone(),
        owner: input.owner,
        work_context: input.work_context.clone(),
        revision: 1,
        status: wire::DefinitionStatus::Enabled,
        disabled: false,
        draft_digest: digest(b"foreign"),
        published_digest: None,
        audience: vec![],
        updated_at: chrono::Utc::now(),
    };
    let wire::DefinitionSource::Blank { mut content } = request.source.clone() else {
        unreachable!()
    };
    content.instructions = "Other concurrent author's content".into();
    let draft = wire::Draft {
        definition: input.definition.clone(),
        revision: 1,
        content,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let mut journey = owner(
        input,
        url::Url::parse(&format!("http://{}/admin/admin/", listener.local_addr()?))?,
        Arc::new(PrivateCallerJournal::create(
            &root.path().join("journal.jsonl"),
        )?),
    )?;
    let server = tokio::spawn(async move {
        for phase in 0..3 {
            let (mut socket, _) = listener.accept().await?;
            let (line, _) = read_request(&mut socket).await?;
            match phase {
                0 => {
                    ensure!(line.starts_with("POST "));
                    socket.write_all(b"HTTP/1.1 409 Conflict\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await?;
                    socket.shutdown().await?;
                }
                1 => {
                    ensure!(line.starts_with("GET "));
                    respond(&mut socket, &definition).await?;
                }
                2 => {
                    ensure!(line.starts_with("GET ") && line.contains("/draft"));
                    respond(&mut socket, &draft).await?;
                }
                _ => unreachable!(),
            }
        }
        ensure!(
            tokio::time::timeout(Duration::from_millis(150), listener.accept())
                .await
                .is_err(),
            "destructive request reached foreign draft"
        );
        Ok::<_, anyhow::Error>(())
    });
    journey.intent(
        Action::Create,
        request.request_id,
        Mutation::Create(&request),
    )?;
    ensure!(
        journey
            .mutate::<wire::Definition>(
                reqwest::Method::POST,
                &["agent-definitions"],
                &request,
                Action::Create,
                request.request_id
            )
            .await
            .is_err()
    );
    ensure!(
        journey.cleanup_remote().await.is_err(),
        "foreign draft admitted for cleanup"
    );
    ensure!(
        journey.dispatch.archive_definition.is_none()
            && journey.dispatch.stop.is_none()
            && journey.dispatch.archive_instance.is_none()
    );
    tokio::time::timeout(Duration::from_secs(3), server).await???;
    Ok(())
}
