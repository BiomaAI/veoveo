use super::*;
pub(crate) async fn surreal_integration() -> Result<()> {
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    let name = format!("veoveo-surreal-smoke-{}", uuid::Uuid::new_v4().simple());
    let mut _container = ContainerGuard::new(name.clone())?;
    run_checked(
        Path::new("docker"),
        [
            "run".into(),
            "--cidfile".into(),
            _container.cid_file().as_os_str().to_os_string(),
            "--detach".into(),
            "--name".into(),
            name.clone().into(),
            "--publish".into(),
            format!("127.0.0.1:{port}:8000").into(),
            "--tmpfs".into(),
            "/data:rw,size=1073741824,uid=65532,gid=65532,mode=0700".into(),
            "surrealdb/surrealdb@sha256:681c6c22c287421b5c7d99e0fde79b6e0d32c36c1ddeaab2762a1661cb04cd20".into(),
            "start".into(),
            "--bind".into(),
            "0.0.0.0:8000".into(),
            "--user".into(),
            "root".into(),
            "--pass".into(),
            "root".into(),
            "rocksdb:/data/veoveo.db".into(),
        ],
        [],
    )?;
    let ready_url = format!("http://127.0.0.1:{port}/ready");
    let mut ready = false;
    for _ in 0..120 {
        if http_ok(&ready_url).await? {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    if !ready {
        bail!("timed out waiting for SurrealDB 3.3.0 at {ready_url}");
    }

    let endpoint = format!("ws://127.0.0.1:{port}");
    let environment = [
        ("VEOVEO_SURREAL_INTEGRATION", "1".into()),
        ("VEOVEO_SURREAL_URL", endpoint.clone().into()),
        ("VEOVEO_SURREAL_ENDPOINT", endpoint.into()),
        ("VEOVEO_SURREAL_USER", "root".into()),
        ("VEOVEO_SURREAL_USERNAME", "root".into()),
        ("VEOVEO_SURREAL_PASSWORD", "root".into()),
    ];
    // Resolve one feature union for the integration targets and collect every
    // target's result before returning a failure.
    println!("==> live SurrealDB integration batch");
    let output = run_checked(
        Path::new("cargo"),
        [
            "test".into(),
            "--locked".into(),
            "--workspace".into(),
            "--all-features".into(),
            "--test".into(),
            "surreal_integration".into(),
            "--test".into(),
            "control_store".into(),
            "--test".into(),
            "gateway_state".into(),
            "--test".into(),
            "current_authority".into(),
            "--test".into(),
            "audit_cli".into(),
            "--test".into(),
            "reads".into(),
            "--test".into(),
            "detail_schema".into(),
            "--no-fail-fast".into(),
            "--".into(),
            "--nocapture".into(),
            "--test-threads=1".into(),
        ],
        environment,
    )?;
    print!("{output}");
    println!("surreal integration smoke ok");
    Ok(())
}
