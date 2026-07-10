use super::*;

/// The agent-kernel keystone: durable detach and resume across processes.
///
/// Phase 1 boots the agent with a scripted fake LLM that dispatches
/// `media__run` (a webhook-delayed task, guaranteed to outlive the episode)
/// and halts after the episode — the descriptor's only home is the ledger.
/// Phase 2 boots a fresh process with no prompt: boot recovery arms a
/// watcher, `McpTaskResumer` rehydrates the task from its persisted
/// descriptor, the webhook completes it, and the result wakes a follow-up
/// episode that consumes it.
pub(crate) async fn agent_kernel_detach_resume(
    conformance: &Path,
    media: &Path,
    gateway: &Path,
    control_plane: &Path,
    artifact_service: &Path,
    agent: &Path,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(media)?;
    assert_executable(gateway)?;
    assert_executable(artifact_service)?;
    assert_executable(agent)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    // The smoke control plane pins its media upstream to 18801, so this
    // scenario shares gateway_task_run's port block and must never run
    // concurrently with it (the suite is sequential).
    let provider_port = 18830u16;
    let media_port = 18801u16;
    let gateway_port = 18832u16;
    let llm_port = 18833u16;
    let media_base = format!("http://127.0.0.1:{media_port}");
    let gateway_base = format!("http://127.0.0.1:{gateway_port}");
    let provider_base = format!("http://127.0.0.1:{provider_port}");

    let provider_ready = tmpdir.join("provider.ready");
    let llm_ready = tmpdir.join("llm.ready");
    let agent_data_dir = tmpdir.join("agent");
    let ledger_path = agent_data_dir.join("memory.duckdb");

    let mut provider = spawn_fake_media_provider(
        conformance,
        provider_port,
        &provider_ready,
        &tmpdir.join("provider.log"),
        Some(4000),
    )?;
    wait_for_file_and_http(&provider_ready, &format!("{provider_base}/api/v3/models")).await?;

    let mut llm = ChildGuard::spawn(
        conformance,
        [
            "fake-openai-llm".into(),
            "--port".into(),
            llm_port.to_string().into(),
            "--ready-file".into(),
            llm_ready.as_os_str().to_os_string(),
        ],
        [],
        &tmpdir.join("llm.log"),
    )?;
    wait_for_file(&llm_ready).await?;

    let plane =
        spawn_artifact_service_smoke(artifact_service, &tmpdir.join("artifact-service.log"))
            .await?;
    let mut media_child = spawn_media_memory_smoke(
        media,
        media_port,
        &media_base,
        &plane.platform,
        &provider_base,
        &plane.url,
        &tmpdir.join("media.log"),
    )?;
    wait_for_http(&format!("{media_base}/media/healthz")).await?;

    let auth_private_key = run_checked(conformance, ["gateway-private-key-der-b64".into()], [])?;
    let auth_private_key = auth_private_key.trim().to_string();
    let platform_store = spawn_gateway_platform_store(gateway, control_plane).await?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, &platform_store),
        [
            (
                "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
                INTERNAL_SIGNING_KEY_DER_B64.into(),
            ),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                auth_private_key.clone().into(),
            ),
        ],
        &tmpdir.join("gateway.log"),
    )?;
    wait_for_http(&format!("{gateway_base}/healthz")).await?;
    assert_ready_profiles(&gateway_base, 2).await?;

    let migrations_dir = tmpdir.join("migrations");
    fs::create_dir_all(&migrations_dir)?;
    fs::write(
        migrations_dir.join("0001_domain.sql"),
        "CREATE TABLE IF NOT EXISTS notes (note TEXT NOT NULL, source TEXT);\n",
    )?;

    let manifest_path = tmpdir.join("agent-manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "agent": { "tenant": "smoke", "id": "smoke-agent", "display_name": "Smoke Agent" },
            "model": {
                "base_url": format!("http://127.0.0.1:{llm_port}/v1"),
                "api_key_env": "SMOKE_LLM_API_KEY",
                "model": "fake/kimi"
            },
            "gateway": {
                "url": gateway_base,
                "profile": "operator",
                "client_id": "operator-service",
                "audience": format!("{PUBLIC_BASE_URL}/oauth/token"),
                "resource": format!("{PUBLIC_BASE_URL}/mcp/operator"),
                "scopes": ["operator:use"],
                "private_key_env": "SMOKE_AGENT_PRIVATE_KEY_DER_B64",
                "private_key_kid": "test-key"
            },
            "episode": {
                "max_turns": 6,
                "task_deadline_s": 120
            },
            "memory": {
                "memory_write_tables": ["notes"]
            },
            "context": {
                "sections": [{
                    "name": "Recent episodes",
                    "priority": 1,
                    "sql": "SELECT seq, summary FROM agent_memory.episode_log ORDER BY seq DESC LIMIT 5"
                }]
            },
            "migrations_dir": "migrations",
            "preamble": "You operate hosted tools through a gateway. Follow instructions exactly."
        }))?,
    )?;
    let agent_envs = || {
        let mut env = platform_store.runtime_env();
        env.extend([
            ("SMOKE_LLM_API_KEY", "fake".into()),
            (
                "SMOKE_AGENT_PRIVATE_KEY_DER_B64",
                auth_private_key.clone().into(),
            ),
        ]);
        env
    };

    // Phase 1: one episode dispatches the media task and halts; the
    // descriptor survives only in the ledger.
    let phase_one = run_checked(
        agent,
        [
            "run".into(),
            "--manifest".into(),
            manifest_path.as_os_str().to_os_string(),
            "--data-dir".into(),
            agent_data_dir.as_os_str().to_os_string(),
            "--prompt".into(),
            "Generate one smoke image with the media tools.".into(),
            "--halt-after-episode".into(),
        ],
        agent_envs(),
    )?;
    contains(&phase_one, "task detached")?;
    contains(&phase_one, "media__run")?;
    contains(&phase_one, "memory_query")?;
    contains(&phase_one, "halt-after-episode set")?;

    {
        let ledger = duckdb::Connection::open(&ledger_path)?;
        let (state, descriptor_complete, tool_name): (String, bool, String) = ledger.query_row(
            "SELECT state, descriptor_complete, tool_name FROM kernel.task_ledger",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        if state != "pending" || !descriptor_complete || tool_name != "media__run" {
            bail!(
                "phase 1 ledger had ({state}, {descriptor_complete}, {tool_name}), \
                 expected a complete pending media__run descriptor"
            );
        }
        let episodes: i64 =
            ledger.query_row("SELECT COUNT(*) FROM agent_memory.episode_log", [], |row| {
                row.get(0)
            })?;
        if episodes != 1 {
            bail!("phase 1 recorded {episodes} episodes, expected 1");
        }
    }

    // Phase 2: a fresh process resumes the task from the ledger alone.
    let agent_log = tmpdir.join("agent-resume.log");
    let mut agent_child = ChildGuard::spawn(
        agent,
        [
            "run".into(),
            "--manifest".into(),
            manifest_path.as_os_str().to_os_string(),
            "--data-dir".into(),
            agent_data_dir.as_os_str().to_os_string(),
        ],
        agent_envs(),
        &agent_log,
    )?;
    wait_for_log_substring(&agent_log, "task result consumed", 240).await?;
    agent_child.stop();

    {
        let ledger = duckdb::Connection::open(&ledger_path)?;
        let (state, consumed): (String, Option<String>) = ledger.query_row(
            "SELECT state, consumed_by_episode FROM kernel.task_ledger",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if state != "resolved" || consumed.is_none() {
            bail!("resume left the task ({state}, consumed: {consumed:?})");
        }
        let (episodes, completed): (i64, i64) = ledger.query_row(
            "SELECT COUNT(*), COUNT(*) FILTER (outcome = 'completed') FROM agent_memory.episode_log",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if episodes != 2 || completed != 2 {
            bail!("expected 2 completed episodes, found {completed}/{episodes}");
        }
        let final_output: String = ledger.query_row(
            "SELECT final_output FROM agent_memory.episode_log ORDER BY seq DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        if !final_output.contains("OBJECTIVE COMPLETE") {
            bail!("wake episode did not complete the objective: {final_output}");
        }
        let notes: i64 = ledger.query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))?;
        if notes != 1 {
            bail!("memory_write recorded {notes} notes, expected 1");
        }
        let summaries: i64 = ledger.query_row(
            "SELECT COUNT(*) FROM agent_memory.episode_log WHERE summary IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        if summaries != 2 {
            bail!("expected 2 episode summaries, found {summaries}");
        }
    }

    // The episodic plane must be readable after an unclean stop: the live
    // segment has no footer and no clean shutdown, and still decodes.
    let timeline = run_checked(
        agent,
        [
            "timeline".into(),
            "--data-dir".into(),
            agent_data_dir.as_os_str().to_os_string(),
            "--entities".into(),
            "/agent/**".into(),
            "--max-rows".into(),
            "200".into(),
        ],
        [],
    )?;
    contains(&timeline, "media__run")?;
    contains(&timeline, "/agent/tasks/")?;

    gateway_child.stop();
    media_child.stop();
    provider.stop();
    llm.stop();
    cleanup.remove_on_drop();
    println!("agent kernel detach/resume smoke ok");
    Ok(())
}

/// The agent-kernel scheduler: heartbeats, operator wakes, budgets, and
/// fail-closed manifests.
///
/// A long-running agent boots against the full gateway stack with a fast
/// heartbeat, is woken by an operator `agent ask` over the loopback HTTP
/// endpoint, and has a sibling data-dir run its episode into a per-episode
/// budget breach. An invalid manifest must refuse to boot.
pub(crate) async fn agent_kernel_scheduler(
    conformance: &Path,
    media: &Path,
    gateway: &Path,
    control_plane: &Path,
    artifact_service: &Path,
    agent: &Path,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(media)?;
    assert_executable(gateway)?;
    assert_executable(artifact_service)?;
    assert_executable(agent)?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let provider_port = 18840u16;
    let media_port = 18801u16;
    let gateway_port = 18842u16;
    let llm_port = 18843u16;
    let media_base = format!("http://127.0.0.1:{media_port}");
    let gateway_base = format!("http://127.0.0.1:{gateway_port}");
    let provider_base = format!("http://127.0.0.1:{provider_port}");

    let provider_ready = tmpdir.join("provider.ready");
    let llm_ready = tmpdir.join("llm.ready");

    let mut provider = spawn_fake_media_provider(
        conformance,
        provider_port,
        &provider_ready,
        &tmpdir.join("provider.log"),
        Some(4000),
    )?;
    wait_for_file_and_http(&provider_ready, &format!("{provider_base}/api/v3/models")).await?;
    let mut llm = ChildGuard::spawn(
        conformance,
        [
            "fake-openai-llm".into(),
            "--port".into(),
            llm_port.to_string().into(),
            "--ready-file".into(),
            llm_ready.as_os_str().to_os_string(),
        ],
        [],
        &tmpdir.join("llm.log"),
    )?;
    wait_for_file(&llm_ready).await?;
    let plane =
        spawn_artifact_service_smoke(artifact_service, &tmpdir.join("artifact-service.log"))
            .await?;
    let mut media_child = spawn_media_memory_smoke(
        media,
        media_port,
        &media_base,
        &plane.platform,
        &provider_base,
        &plane.url,
        &tmpdir.join("media.log"),
    )?;
    wait_for_http(&format!("{media_base}/media/healthz")).await?;
    let auth_private_key = run_checked(conformance, ["gateway-private-key-der-b64".into()], [])?;
    let auth_private_key = auth_private_key.trim().to_string();
    let platform_store = spawn_gateway_platform_store(gateway, control_plane).await?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, &platform_store),
        [
            (
                "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
                INTERNAL_SIGNING_KEY_DER_B64.into(),
            ),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                auth_private_key.clone().into(),
            ),
        ],
        &tmpdir.join("gateway.log"),
    )?;
    wait_for_http(&format!("{gateway_base}/healthz")).await?;
    assert_ready_profiles(&gateway_base, 2).await?;

    let write_manifest = |name: &str, extra: serde_json::Value| -> Result<std::path::PathBuf> {
        let mut manifest = serde_json::json!({
            "agent": { "tenant": "smoke", "id": "smoke-scheduler", "display_name": "Scheduler Smoke Agent" },
            "model": {
                "base_url": format!("http://127.0.0.1:{llm_port}/v1"),
                "api_key_env": "SMOKE_LLM_API_KEY",
                "model": "fake/kimi"
            },
            "gateway": {
                "url": gateway_base,
                "profile": "operator",
                "client_id": "operator-service",
                "audience": format!("{PUBLIC_BASE_URL}/oauth/token"),
                "resource": format!("{PUBLIC_BASE_URL}/mcp/operator"),
                "scopes": ["operator:use"],
                "private_key_env": "SMOKE_AGENT_PRIVATE_KEY_DER_B64",
                "private_key_kid": "test-key"
            },
            "episode": { "max_turns": 6, "task_deadline_s": 60 },
            "schedule": { "heartbeat_interval_s": 2, "wake_coalesce_window_ms": 100 },
            "preamble": "You operate hosted tools through a gateway. Follow instructions exactly."
        });
        if let (serde_json::Value::Object(base), serde_json::Value::Object(extra)) =
            (&mut manifest, extra)
        {
            base.extend(extra);
        }
        let path = tmpdir.join(name);
        fs::write(&path, serde_json::to_vec_pretty(&manifest)?)?;
        Ok(path)
    };
    let agent_envs = || {
        let mut env = platform_store.runtime_env();
        env.extend([
            ("SMOKE_LLM_API_KEY", "fake".into()),
            (
                "SMOKE_AGENT_PRIVATE_KEY_DER_B64",
                auth_private_key.clone().into(),
            ),
        ]);
        env
    };

    // Fail-closed boot: an unknown manifest field must refuse to start.
    let invalid_manifest = tmpdir.join("invalid-manifest.json");
    fs::write(
        &invalid_manifest,
        serde_json::to_vec_pretty(&serde_json::json!({ "surprise": true }))?,
    )?;
    let invalid = run_raw(
        agent,
        [
            "run".into(),
            "--manifest".into(),
            invalid_manifest.as_os_str().to_os_string(),
            "--data-dir".into(),
            tmpdir.join("invalid-data").as_os_str().to_os_string(),
        ],
        agent_envs(),
    )?;
    if invalid.status.success() {
        bail!("agent accepted an invalid manifest");
    }

    // Budget breach: one tool call is over the per-episode cap.
    let budget_manifest = write_manifest(
        "budget-manifest.json",
        serde_json::json!({
            "budgets": { "per_episode": { "max_tool_calls": 0 } }
        }),
    )?;
    let budget_data_dir = tmpdir.join("budget-data");
    let budget_run = run_checked(
        agent,
        [
            "run".into(),
            "--manifest".into(),
            budget_manifest.as_os_str().to_os_string(),
            "--data-dir".into(),
            budget_data_dir.as_os_str().to_os_string(),
            "--prompt".into(),
            "Count your episodes".into(),
            "--halt-after-episode".into(),
        ],
        agent_envs(),
    )?;
    contains(&budget_run, "episode budget terminated")?;
    {
        let ledger = duckdb::Connection::open(budget_data_dir.join("memory.duckdb"))?;
        let outcome: String = ledger.query_row(
            "SELECT outcome FROM agent_memory.episode_log ORDER BY seq DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        if outcome != "budget_terminated" {
            bail!("budget episode outcome was `{outcome}`");
        }
    }

    // The scheduler proper: heartbeats fire, operator asks preempt.
    let scheduler_manifest = write_manifest("scheduler-manifest.json", serde_json::json!({}))?;
    let scheduler_data_dir = tmpdir.join("scheduler-data");
    let agent_log = tmpdir.join("agent-scheduler.log");
    let mut agent_child = ChildGuard::spawn(
        agent,
        [
            "run".into(),
            "--manifest".into(),
            scheduler_manifest.as_os_str().to_os_string(),
            "--data-dir".into(),
            scheduler_data_dir.as_os_str().to_os_string(),
        ],
        agent_envs(),
        &agent_log,
    )?;
    wait_for_log_substring(&agent_log, "operator endpoint listening", 120).await?;
    wait_for_log_substring(&agent_log, "\"wake_note\":\"heartbeat\"", 120).await?;

    let ask = run_checked(
        agent,
        [
            "ask".into(),
            "--data-dir".into(),
            scheduler_data_dir.as_os_str().to_os_string(),
            "Count your episodes".into(),
        ],
        [],
    )?;
    contains(&ask, "wake_id")?;
    wait_for_log_substring(&agent_log, "\"wake_note\":\"operator\"", 120).await?;

    let status = run_checked(
        agent,
        [
            "status".into(),
            "--data-dir".into(),
            scheduler_data_dir.as_os_str().to_os_string(),
        ],
        [],
    )?;
    contains(&status, "recent_episodes")?;

    // Give the operator episode time to book, then stop and inspect.
    wait_for_log_substring(&agent_log, "EPISODES COUNTED", 120).await?;
    agent_child.stop();
    {
        let ledger = duckdb::Connection::open(scheduler_data_dir.join("memory.duckdb"))?;
        let heartbeat_episodes: i64 = ledger.query_row(
            "SELECT COUNT(*) FROM agent_memory.episode_log WHERE wake_note LIKE '%heartbeat%'",
            [],
            |row| row.get(0),
        )?;
        if heartbeat_episodes < 1 {
            bail!("no heartbeat episodes ran");
        }
        let operator_output: String = ledger.query_row(
            "SELECT final_output FROM agent_memory.episode_log WHERE wake_note LIKE '%operator%'
             ORDER BY seq DESC LIMIT 1",
            [],
            |row| row.get(0),
        )?;
        if !operator_output.contains("EPISODES COUNTED") {
            bail!("operator episode output was `{operator_output}`");
        }
        let handled_wakes: i64 = ledger.query_row(
            "SELECT COUNT(*) FROM kernel.wakes WHERE state = 'handled'",
            [],
            |row| row.get(0),
        )?;
        if handled_wakes < 2 {
            bail!("expected handled wakes, found {handled_wakes}");
        }
    }

    gateway_child.stop();
    media_child.stop();
    provider.stop();
    llm.stop();
    cleanup.remove_on_drop();
    println!("agent kernel scheduler smoke ok");
    Ok(())
}

/// The Pilot mission: the full agent loop over real geodesy and planning.
///
/// One operator objective drives the whole choreography — record a target
/// (memory_write), measure the leg (coordinates__geodesic_inverse, inline),
/// dispatch the planner (optimization__plan, task-required), then record the
/// waypoint when the plan lands and declare the mission planned. The pilot's
/// real domain migrations from configs/agents/pilot are applied verbatim.
pub(crate) async fn agent_pilot_mission(
    conformance: &Path,
    coordinates: &Path,
    optimization: &Path,
    gateway: &Path,
    control_plane: &Path,
    artifact_service: &Path,
    agent: &Path,
) -> Result<()> {
    for bin in [
        conformance,
        coordinates,
        optimization,
        gateway,
        artifact_service,
        agent,
    ] {
        assert_executable(bin)?;
    }

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let coordinates_port = 18850u16;
    let optimization_port = 18851u16;
    let gateway_port = 18852u16;
    let llm_port = 18853u16;
    let coordinates_base = format!("http://127.0.0.1:{coordinates_port}");
    let optimization_base = format!("http://127.0.0.1:{optimization_port}");
    let gateway_base = format!("http://127.0.0.1:{gateway_port}");

    let plane =
        spawn_artifact_service_smoke(artifact_service, &tmpdir.join("artifact-service.log"))
            .await?;
    let mut coordinates_child = spawn_coordinates_smoke(
        coordinates,
        coordinates_port,
        &coordinates_base,
        &plane.url,
        &plane.platform,
        &tmpdir.join("coordinates.log"),
    )?;
    let mut optimization_child = spawn_optimization_smoke(
        optimization,
        optimization_port,
        &optimization_base,
        &tmpdir.join("optimization-state.duckdb"),
        &plane.url,
        &tmpdir.join("optimization.log"),
    )?;
    wait_for_http(&format!("{coordinates_base}/coordinates/healthz")).await?;
    wait_for_http(&format!("{optimization_base}/optimization/healthz")).await?;

    let otlp_port = 18854u16;
    let otlp_ready = tmpdir.join("otlp.ready");
    let otlp_hits = tmpdir.join("otlp.hits");
    let mut otlp = ChildGuard::spawn(
        conformance,
        [
            "otlp-http-sink".into(),
            "--port".into(),
            otlp_port.to_string().into(),
            "--ready-file".into(),
            otlp_ready.as_os_str().to_os_string(),
            "--hits-file".into(),
            otlp_hits.as_os_str().to_os_string(),
        ],
        [],
        &tmpdir.join("otlp.log"),
    )?;
    wait_for_file(&otlp_ready).await?;

    let llm_ready = tmpdir.join("llm.ready");
    let mut llm = ChildGuard::spawn(
        conformance,
        [
            "fake-openai-llm".into(),
            "--port".into(),
            llm_port.to_string().into(),
            "--ready-file".into(),
            llm_ready.as_os_str().to_os_string(),
        ],
        [],
        &tmpdir.join("llm.log"),
    )?;
    wait_for_file(&llm_ready).await?;

    let generated_control_plane = tmpdir.join("gateway.pilot.json");
    run_checked(
        conformance,
        [
            "gateway-pilot-smoke-control-plane".into(),
            "--base".into(),
            control_plane.as_os_str().to_os_string(),
            "--output".into(),
            generated_control_plane.as_os_str().to_os_string(),
            "--coordinates-upstream-url".into(),
            format!("{coordinates_base}/coordinates/mcp").into(),
            "--optimization-upstream-url".into(),
            format!("{optimization_base}/optimization/mcp").into(),
        ],
        [],
    )?;
    let auth_private_key = run_checked(conformance, ["gateway-private-key-der-b64".into()], [])?;
    let auth_private_key = auth_private_key.trim().to_string();
    let platform_store = spawn_gateway_platform_store(gateway, &generated_control_plane).await?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, &platform_store),
        [
            (
                "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
                INTERNAL_SIGNING_KEY_DER_B64.into(),
            ),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                auth_private_key.clone().into(),
            ),
        ],
        &tmpdir.join("gateway.log"),
    )?;
    wait_for_http(&format!("{gateway_base}/healthz")).await?;
    assert_ready_profiles(&gateway_base, 2).await?;

    // The pilot's real domain migrations, applied verbatim.
    let migrations_dir = fs::canonicalize("configs/agents/pilot/migrations")?;
    let manifest_path = tmpdir.join("pilot-manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "agent": { "tenant": "smoke", "id": "pilot-smoke", "display_name": "Pilot Smoke Agent" },
            "model": {
                "base_url": format!("http://127.0.0.1:{llm_port}/v1"),
                "api_key_env": "SMOKE_LLM_API_KEY",
                "model": "fake/kimi"
            },
            "gateway": {
                "url": gateway_base,
                "profile": "operator",
                "client_id": "operator-service",
                "audience": format!("{PUBLIC_BASE_URL}/oauth/token"),
                "resource": format!("{PUBLIC_BASE_URL}/mcp/operator"),
                "scopes": ["operator:use"],
                "private_key_env": "SMOKE_AGENT_PRIVATE_KEY_DER_B64",
                "private_key_kid": "test-key"
            },
            "episode": { "max_turns": 8, "task_deadline_s": 120 },
            "schedule": { "heartbeat_interval_s": 30, "wake_coalesce_window_ms": 100 },
            "memory": {
                "memory_write_tables": ["targets", "missions", "waypoints", "constraints", "beliefs"]
            },
            "context": {
                "sections": [{
                    "name": "Active targets",
                    "priority": 1,
                    "sql": "SELECT target_id, name, lat, lon FROM targets WHERE status = 'active' ORDER BY priority DESC LIMIT 20"
                }]
            },
            "migrations_dir": migrations_dir,
            "preamble": "You are the Pilot. Follow instructions exactly."
        }))?,
    )?;

    let agent_data_dir = tmpdir.join("pilot-data");
    let agent_log = tmpdir.join("pilot.log");
    let mut agent_child = ChildGuard::spawn(
        agent,
        [
            "run".into(),
            "--manifest".into(),
            manifest_path.as_os_str().to_os_string(),
            "--data-dir".into(),
            agent_data_dir.as_os_str().to_os_string(),
            "--prompt".into(),
            "Add target alpha at 37.7749,-122.4194 and plan the visit.".into(),
        ],
        [
            ("SMOKE_LLM_API_KEY", "fake".into()),
            (
                "SMOKE_AGENT_PRIVATE_KEY_DER_B64",
                auth_private_key.clone().into(),
            ),
            ("OTEL_SERVICE_NAME", "veoveo-pilot-smoke".into()),
            (
                "OTEL_EXPORTER_OTLP_ENDPOINT",
                format!("http://127.0.0.1:{otlp_port}").into(),
            ),
        ],
        &agent_log,
    )?;
    wait_for_log_substring(&agent_log, "MISSION PLANNED", 240).await?;
    agent_child.stop();

    {
        let ledger = duckdb::Connection::open(agent_data_dir.join("memory.duckdb"))?;
        let targets: i64 =
            ledger.query_row("SELECT COUNT(*) FROM targets", [], |row| row.get(0))?;
        let waypoints: i64 =
            ledger.query_row("SELECT COUNT(*) FROM waypoints", [], |row| row.get(0))?;
        if targets != 1 || waypoints != 1 {
            bail!("pilot memory had {targets} targets / {waypoints} waypoints, expected 1/1");
        }
        let (task_tool, task_state, consumed): (String, String, Option<String>) = ledger
            .query_row(
                "SELECT tool_name, state, consumed_by_episode FROM kernel.task_ledger",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
        if task_tool != "optimization__plan" || task_state != "resolved" || consumed.is_none() {
            bail!("plan task was ({task_tool}, {task_state}, consumed: {consumed:?})");
        }
        let planned: i64 = ledger.query_row(
            "SELECT COUNT(*) FROM agent_memory.episode_log WHERE final_output LIKE '%MISSION PLANNED%'",
            [],
            |row| row.get(0),
        )?;
        if planned < 1 {
            bail!("no episode declared the mission planned");
        }
    }

    // The decision log shows the whole choreography.
    let timeline = run_checked(
        agent,
        [
            "timeline".into(),
            "--data-dir".into(),
            agent_data_dir.as_os_str().to_os_string(),
            "--entities".into(),
            "/agent/**".into(),
            "--max-rows".into(),
            "300".into(),
        ],
        [],
    )?;
    contains(&timeline, "coordinates__geodesic_inverse")?;
    contains(&timeline, "optimization__plan")?;

    // Replay rebuilds domain truth from the decision log alone.
    let replay = run_checked(
        agent,
        [
            "replay".into(),
            "--manifest".into(),
            manifest_path.as_os_str().to_os_string(),
            "--data-dir".into(),
            agent_data_dir.as_os_str().to_os_string(),
        ],
        [
            ("SMOKE_LLM_API_KEY", "fake".into()),
            (
                "SMOKE_AGENT_PRIVATE_KEY_DER_B64",
                auth_private_key.clone().into(),
            ),
        ],
    )?;
    contains(&replay, "\"applied\":2")?;
    {
        let replayed = duckdb::Connection::open(agent_data_dir.join("memory.replayed.duckdb"))?;
        let (targets, waypoints): (i64, i64) = replayed.query_row(
            "SELECT (SELECT COUNT(*) FROM targets), (SELECT COUNT(*) FROM waypoints)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if targets != 1 || waypoints != 1 {
            bail!("replay rebuilt {targets} targets / {waypoints} waypoints, expected 1/1");
        }
    }

    // Telemetry left the building: the agent exported logs or traces.
    wait_for_file_contains(&otlp_hits, "logs ", "traces ").await?;

    gateway_child.stop();
    coordinates_child.stop();
    optimization_child.stop();
    llm.stop();
    otlp.stop();
    cleanup.remove_on_drop();
    println!("agent pilot mission smoke ok");
    Ok(())
}

/// The real deal: one continuously-running agent process dispatches a
/// genuinely long gateway task, goes to sleep, and is woken by the task's
/// completion push — provably not by polling.
///
/// The proof is arithmetic. The task takes ~15s (webhook-delayed provider);
/// the manifest sets the poll cadence to 60s and the heartbeat to 600s, so a
/// wake episode starting within seconds of completion can only have come
/// from the blocking `tasks/result` long-poll and the task-status
/// notification the gateway forwards. The ledger's episode timestamps pin
/// both halves: the agent really slept (gap well above zero, no episodes in
/// between) and really woke promptly (gap bounded near the task duration).
///
/// With `live: true` the scripted LLM is replaced by the real model from the
/// environment (Cloudflare Workers AI), making the run end-to-end real:
/// real model reasoning, real gateway auth and forwarding, real hosted
/// server, real task lifecycle, real push wake.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn agent_sleep_wake(
    conformance: &Path,
    media: &Path,
    gateway: &Path,
    control_plane: &Path,
    artifact_service: &Path,
    agent: &Path,
    live: bool,
) -> Result<()> {
    for bin in [conformance, media, gateway, artifact_service, agent] {
        assert_executable(bin)?;
    }
    const TASK_DELAY_MS: u64 = 15_000;
    const SLEEP_FLOOR_S: f64 = 8.0;
    const WAKE_CEILING_S: f64 = TASK_DELAY_MS as f64 / 1000.0 + 10.0;

    let live_model = if live {
        let account = env::var("CLOUDFLARE_ACCOUNT_ID").map_err(|_| {
            anyhow!(
                "live mode requires CLOUDFLARE_ACCOUNT_ID and CLOUDFLARE_API_TOKEN in the environment"
            )
        })?;
        env::var("CLOUDFLARE_API_TOKEN")
            .map_err(|_| anyhow!("live mode requires CLOUDFLARE_API_TOKEN in the environment"))?;
        let model =
            env::var("AGENT_LIVE_MODEL").unwrap_or_else(|_| "@cf/moonshotai/kimi-k2.6".to_string());
        println!("live model: {model}");
        Some((
            format!("https://api.cloudflare.com/client/v4/accounts/{account}/ai/v1"),
            "CLOUDFLARE_API_TOKEN".to_string(),
            model,
        ))
    } else {
        None
    };

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let provider_port = 18860u16;
    let media_port = 18801u16;
    let gateway_port = 18862u16;
    let llm_port = 18863u16;
    let media_base = format!("http://127.0.0.1:{media_port}");
    let gateway_base = format!("http://127.0.0.1:{gateway_port}");
    let provider_base = format!("http://127.0.0.1:{provider_port}");

    let provider_ready = tmpdir.join("provider.ready");
    let mut provider = spawn_fake_media_provider(
        conformance,
        provider_port,
        &provider_ready,
        &tmpdir.join("provider.log"),
        Some(TASK_DELAY_MS),
    )?;
    wait_for_file_and_http(&provider_ready, &format!("{provider_base}/api/v3/models")).await?;

    let mut llm = None;
    if live_model.is_none() {
        let llm_ready = tmpdir.join("llm.ready");
        llm = Some(ChildGuard::spawn(
            conformance,
            [
                "fake-openai-llm".into(),
                "--port".into(),
                llm_port.to_string().into(),
                "--ready-file".into(),
                llm_ready.as_os_str().to_os_string(),
            ],
            [],
            &tmpdir.join("llm.log"),
        )?);
        wait_for_file(&llm_ready).await?;
    }

    let plane =
        spawn_artifact_service_smoke(artifact_service, &tmpdir.join("artifact-service.log"))
            .await?;
    let mut media_child = spawn_media_memory_smoke(
        media,
        media_port,
        &media_base,
        &plane.platform,
        &provider_base,
        &plane.url,
        &tmpdir.join("media.log"),
    )?;
    wait_for_http(&format!("{media_base}/media/healthz")).await?;
    let auth_private_key = run_checked(conformance, ["gateway-private-key-der-b64".into()], [])?;
    let auth_private_key = auth_private_key.trim().to_string();
    let platform_store = spawn_gateway_platform_store(gateway, control_plane).await?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, &platform_store),
        [
            (
                "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
                INTERNAL_SIGNING_KEY_DER_B64.into(),
            ),
            (
                "VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64",
                auth_private_key.clone().into(),
            ),
        ],
        &tmpdir.join("gateway.log"),
    )?;
    wait_for_http(&format!("{gateway_base}/healthz")).await?;
    assert_ready_profiles(&gateway_base, 2).await?;

    let (model_base_url, api_key_env, model) = live_model.clone().unwrap_or((
        format!("http://127.0.0.1:{llm_port}/v1"),
        "SMOKE_LLM_API_KEY".to_string(),
        "fake/kimi".to_string(),
    ));
    let manifest_path = tmpdir.join("sleep-wake-manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "agent": { "tenant": "smoke", "id": "sleep-wake", "display_name": "Sleep/Wake Agent" },
            "model": {
                "base_url": model_base_url,
                "api_key_env": api_key_env,
                "model": model,
                "temperature": 0.0
            },
            "gateway": {
                "url": gateway_base,
                "profile": "operator",
                "client_id": "operator-service",
                "audience": format!("{PUBLIC_BASE_URL}/oauth/token"),
                "resource": format!("{PUBLIC_BASE_URL}/mcp/operator"),
                "scopes": ["operator:use"],
                "private_key_env": "SMOKE_AGENT_PRIVATE_KEY_DER_B64",
                "private_key_kid": "test-key"
            },
            "episode": {
                "max_turns": 8,
                "task_deadline_s": 120
            },
            "schedule": { "heartbeat_interval_s": 600, "wake_coalesce_window_ms": 250 },
            "budgets": { "per_episode": { "max_completion_calls": 12, "max_tool_calls": 8 } },
            "preamble": "You operate hosted tools through an MCP gateway. Long-running tools \
                         are dispatched as background tasks: dispatch them, then END YOUR TURN \
                         immediately — you will be woken with the result. Never wait, never \
                         poll, never re-dispatch a task you already started. When an episode \
                         starts with a background task result, acknowledge it in one short \
                         sentence that contains the word DELIVERED, then stop."
        }))?,
    )?;

    let boot_prompt = if live {
        // The real model gets explicit, bounded instructions.
        "Generate exactly one image by calling the media__run tool once, with arguments \
         {\"model\": \"fake/image\", \"input\": {\"prompt\": \"sleep wake live\"}}. After the \
         tool call is dispatched as a background task, say WAITING FOR BACKGROUND TASKS and \
         stop."
    } else {
        "Generate one smoke image with the media tools."
    };

    let agent_data_dir = tmpdir.join("agent-data");
    let agent_log = tmpdir.join("agent.log");
    let mut agent_child = ChildGuard::spawn(
        agent,
        [
            "run".into(),
            "--manifest".into(),
            manifest_path.as_os_str().to_os_string(),
            "--data-dir".into(),
            agent_data_dir.as_os_str().to_os_string(),
            "--prompt".into(),
            boot_prompt.into(),
        ],
        [
            ("SMOKE_LLM_API_KEY", "fake".into()),
            (
                "SMOKE_AGENT_PRIVATE_KEY_DER_B64",
                auth_private_key.clone().into(),
            ),
        ],
        &agent_log,
    )?;

    // The whole life in one process: dispatch, sleep, push wake, consume.
    wait_for_log_substring(&agent_log, "task detached", 240).await?;
    println!("task detached; the agent is now asleep on the long task");
    wait_for_log_substring(&agent_log, "task result consumed", 320).await?;
    agent_child.stop();

    let log = fs::read_to_string(&agent_log)?;
    if log.matches("agent booting").count() != 1 {
        bail!("expected exactly one agent boot; the sleep/wake must be one process");
    }
    contains(&log, "task status notification received")?;

    {
        let ledger = duckdb::Connection::open(agent_data_dir.join("memory.duckdb"))?;
        let episodes: i64 =
            ledger.query_row("SELECT COUNT(*) FROM agent_memory.episode_log", [], |row| {
                row.get(0)
            })?;
        if episodes != 2 {
            bail!("expected exactly 2 episodes (dispatch + wake), found {episodes}");
        }
        let (task_state, consumed): (String, Option<String>) = ledger.query_row(
            "SELECT state, consumed_by_episode FROM kernel.task_ledger",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if task_state != "resolved" || consumed.is_none() {
            bail!("task ended ({task_state}, consumed: {consumed:?})");
        }
        // Slept: a real gap between the dispatch episode ending and the wake
        // episode starting. Woke promptly: the gap is bounded near the task
        // duration — unreachable for the 60s poll or the 600s heartbeat.
        let (sleep_gap_s, dispatch_s, wake_s): (f64, f64, f64) = ledger.query_row(
            "WITH ordered AS (SELECT seq, started_at, finished_at FROM agent_memory.episode_log)
             SELECT
                 epoch(o2.started_at) - epoch(o1.finished_at),
                 epoch(o1.finished_at) - epoch(o1.started_at),
                 epoch(o2.finished_at) - epoch(o2.started_at)
             FROM ordered o1, ordered o2 WHERE o1.seq = 1 AND o2.seq = 2",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        println!(
            "dispatch episode {dispatch_s:.1}s, slept {sleep_gap_s:.1}s, wake episode {wake_s:.1}s"
        );
        if sleep_gap_s < SLEEP_FLOOR_S {
            bail!(
                "agent only slept {sleep_gap_s:.1}s; the task should have held it >= {SLEEP_FLOOR_S}s"
            );
        }
        if sleep_gap_s > WAKE_CEILING_S {
            bail!(
                "agent slept {sleep_gap_s:.1}s (> {WAKE_CEILING_S}s): the wake was not prompt, \
                 so it came from polling, not the push path"
            );
        }
        if live {
            let delivered: i64 = ledger.query_row(
                "SELECT COUNT(*) FROM agent_memory.episode_log WHERE final_output LIKE '%DELIVERED%'",
                [],
                |row| row.get(0),
            )?;
            if delivered < 1 {
                let outputs = ledger
                    .prepare("SELECT seq, final_output FROM agent_memory.episode_log ORDER BY seq")?
                    .query_map([], |row| {
                        Ok(format!(
                            "  episode {}: {}",
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?
                    .join("\n");
                bail!("live model never acknowledged DELIVERED; outputs:\n{outputs}");
            }
        }
    }

    gateway_child.stop();
    media_child.stop();
    provider.stop();
    if let Some(mut llm) = llm {
        llm.stop();
    }
    cleanup.remove_on_drop();
    println!(
        "agent sleep/wake smoke ok ({})",
        if live { "live model" } else { "scripted model" }
    );
    Ok(())
}

/// Poll a child's log file until it contains `needle`.
async fn wait_for_log_substring(file: &Path, needle: &str, attempts: u32) -> Result<()> {
    for _ in 0..attempts {
        if fs::read_to_string(file)
            .map(|contents| contents.contains(needle))
            .unwrap_or(false)
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let contents = fs::read_to_string(file).unwrap_or_default();
    bail!(
        "timed out waiting for `{needle}` in {}\ncontents:\n{contents}",
        file.display()
    );
}
