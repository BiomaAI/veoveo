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
    let media_state_db = tmpdir.join("media-state.duckdb");
    let gateway_state_db = tmpdir.join("gateway-state.duckdb");
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
        &media_state_db,
        &provider_base,
        &plane.url,
        &tmpdir.join("media.log"),
    )?;
    wait_for_http(&format!("{media_base}/media/healthz")).await?;

    let auth_private_key = run_checked(conformance, ["gateway-private-key-der-b64".into()], [])?;
    let auth_private_key = auth_private_key.trim().to_string();
    let control_db = spawn_gateway_control_db(gateway, control_plane).await?;
    let mut gateway_child = ChildGuard::spawn(
        gateway,
        gateway_serve_args(gateway_port, &control_db.url, &gateway_state_db),
        [
            ("VEOVEO_INTERNAL_TOKEN_SECRET", INTERNAL_SECRET.into()),
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
            "agent": { "id": "smoke-agent", "display_name": "Smoke Agent" },
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
                "task_deadline_s": 120,
                "task_poll_interval_ms": 300
            },
            "memory": {
                "memory_write_tables": ["notes"]
            },
            "context": {
                "sections": [{
                    "name": "Recent episodes",
                    "priority": 1,
                    "sql": "SELECT seq, summary FROM kernel.episodes ORDER BY seq DESC LIMIT 5"
                }]
            },
            "migrations_dir": "migrations",
            "preamble": "You operate hosted tools through a gateway. Follow instructions exactly."
        }))?,
    )?;
    let agent_envs = || {
        [
            ("SMOKE_LLM_API_KEY", "fake".into()),
            (
                "SMOKE_AGENT_PRIVATE_KEY_DER_B64",
                auth_private_key.clone().into(),
            ),
        ]
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
            ledger.query_row("SELECT COUNT(*) FROM kernel.episodes", [], |row| row.get(0))?;
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
            "SELECT COUNT(*), COUNT(*) FILTER (outcome = 'completed') FROM kernel.episodes",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if episodes != 2 || completed != 2 {
            bail!("expected 2 completed episodes, found {completed}/{episodes}");
        }
        let final_output: String = ledger.query_row(
            "SELECT final_output FROM kernel.episodes ORDER BY seq DESC LIMIT 1",
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
            "SELECT COUNT(*) FROM kernel.episodes WHERE summary IS NOT NULL",
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
