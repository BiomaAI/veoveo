//! Continue an existing acceptance upload after interruption of its observing harness.
use super::*;

pub(crate) async fn verify_resume(
    public_base: &str,
    cdp_base: &str,
    original: &Path,
    timeout: Duration,
) -> Result<()> {
    ensure!(
        Url::parse(public_base)?.scheme() == "https",
        "resume acceptance requires public HTTPS"
    );
    let run_id = original
        .file_name()
        .and_then(|v| v.to_str())
        .context("missing original run identity")?;
    ensure!(
        uuid::Uuid::parse_str(run_id)?.get_version_num() == 7,
        "invalid original run identity"
    );
    let fixtures = original.join("fixtures");
    let large_name = format!("upload-acceptance-{run_id}.bin");
    let csv_name = format!("upload-acceptance-{run_id}.csv");
    let large = fixtures.join(&large_name);
    let csv = fixtures.join(&csv_name);
    let bytes = fs::metadata(&large)?.len();
    ensure!(
        bytes > u64::from(u32::MAX),
        "resume fixture must exceed 4 GiB"
    );
    let directory = original.join(format!("resume-{}", uuid::Uuid::now_v7()));
    fs::create_dir(&directory)?;
    let page_url = console_acceptance_url(public_base, "/artifacts");
    let (mut cdp, target, session) = open_headed_target(cdp_base, &page_url).await?;
    let result = tokio::time::timeout(timeout, async {
        wait_for_document(&mut cdp, &session).await?;
        let hardware = hardware_check(&mut cdp, &session).await?;
        let policy = authorized_policy(&mut cdp, &session, &page_url).await?;
        ensure!(policy.status == 200 && policy.allowed, "resume requires currently authorized upload access: {policy:?}");
        let mut evidence = Evidence {
            schema: "veoveo.io/console-artifact-upload-resume-acceptance/v1", source_revision: git_revision()?, page_url: page_url.clone(),
            hardware, policy, preflight_only: false, steps: vec![format!("Continues saved fixture in {}", original.display())],
            screenshots: vec![], accepted_before_reload: 0, elapsed_seconds: 0.0, large_receipt: None, csv_receipt: None,
        };
        wait_selector(&mut cdp, &session, "button[aria-label='Open uploads']", Duration::from_secs(90)).await?;
        click(&mut cdp, &session, "button[aria-label='Open uploads']").await?;
        let restored = wait_row(&mut cdp, &session, &large_name, Duration::from_secs(90), |r| r.upload_id.is_some() && r.accepted > 0).await?;
        evidence.accepted_before_reload = restored.accepted;
        step(&mut evidence, &format!("Recovered existing upload with {} saved bytes", restored.accepted));
        let path = large.clone();
        let hash = tokio::task::spawn_blocking(move || hash_file(&path));
        if !["Ready", "Finishing upload"].contains(&restored.phase.as_str()) {
            let selector = format!("input[aria-label={}]", serde_json::to_string(&format!("Original file for {large_name}"))?);
            wait_selector(&mut cdp, &session, &selector, Duration::from_secs(30)).await?;
            set_files(&mut cdp, &session, &selector, &[&large]).await?;
        }
        let started = tokio::time::Instant::now();
        let mut last_progress = started;
        let mut captured_finishing = false;
        loop {
            hardware_check(&mut cdp, &session).await?;
            let current = row(&mut cdp, &session, &large_name).await?;
            ensure!(current.upload_id == restored.upload_id && current.accepted >= restored.accepted, "resume changed identity or lost accepted progress");
            if last_progress.elapsed() >= Duration::from_secs(30) {
                println!("Resumed upload: {} / {} accepted bytes; {}", current.accepted, bytes, current.phase);
                last_progress = tokio::time::Instant::now();
            }
            if current.phase == "Finishing upload" && !captured_finishing {
                screenshot(&mut cdp, &session, &directory, "finishing", &mut evidence).await?;
                captured_finishing = true;
            }
            if current.phase == "Ready" { evidence.large_receipt = current.receipt; break; }
            ensure!(!["Needs attention", "Sign in to continue", "Cancelled"].contains(&current.phase.as_str()), "resume failed: {current:?}");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        evidence.elapsed_seconds = started.elapsed().as_secs_f64();
        let receipt = evidence.large_receipt.as_ref().context("Ready upload has no receipt")?;
        ensure!(receipt.byte_len == bytes && receipt.filename == large_name && receipt.sha256 == hash.await??, "resumed receipt failed independent size or hash verification");
        let artifact_id = receipt.artifact_id.clone();
        verify_download(&mut cdp, &session, &artifact_id, bytes).await?;
        let small = row(&mut cdp, &session, &csv_name).await?;
        if small.phase != "Ready" {
            let selector = format!("input[aria-label={}]", serde_json::to_string(&format!("Original file for {csv_name}"))?);
            set_files(&mut cdp, &session, &selector, &[&csv]).await?;
        }
        let small = wait_row(&mut cdp, &session, &csv_name, Duration::from_secs(180), |r| r.phase == "Ready").await?;
        evidence.csv_receipt = small.receipt;
        ensure!(evidence.csv_receipt.as_ref().context("CSV missing receipt")?.sha256 == hash_file(&csv)?, "CSV digest mismatch");
        screenshot(&mut cdp, &session, &directory, "ready-desktop", &mut evidence).await?;
        cdp.command("Emulation.setDeviceMetricsOverride", serde_json::json!({"width":390,"height":844,"deviceScaleFactor":1,"mobile":false}), Some(&session)).await?;
        screenshot(&mut cdp, &session, &directory, "ready-narrow", &mut evidence).await?;
        click_label(&mut cdp, &session, &format!("View artifact {large_name}")).await?;
        wait_selector(&mut cdp, &session, ".drawer-status", Duration::from_secs(90)).await?;
        screenshot(&mut cdp, &session, &directory, "artifact-drawer", &mut evidence).await?;
        step(&mut evidence, "Original upload completed with exact bytes/hash, public HEAD/Range, and ArtifactDrawer");
        Ok::<_, anyhow::Error>(evidence)
    }).await.context("resumed upload observation timed out");
    let close = close_target(&mut cdp, &target).await;
    let evidence = result??;
    close?;
    fs::write(
        directory.join("evidence.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;
    println!(
        "Resumed public large upload passed. Evidence: {}",
        directory.display()
    );
    // Keep this generated fixture until all downstream consumer checks have finished.
    Ok(())
}

async fn verify_download(cdp: &mut Cdp, session: &str, artifact: &str, bytes: u64) -> Result<()> {
    #[derive(Debug, Deserialize)]
    struct Download {
        head: u16,
        length: String,
        range: u16,
        content_range: String,
        received: usize,
    }
    let result: Download = cdp.evaluate(session, &format!(r#"(async()=>{{
        const url='/console/api/artifacts/'+{}+'/download';
        const head=await fetch(url,{{method:'HEAD'}});
        const range=await fetch(url,{{headers:{{Range:'bytes=0-1023'}}}});
        const reader=range.body?.getReader();let received=0;
        if(range.status!==206)await reader?.cancel();
        else while(reader){{const next=await reader.read();if(next.done)break;received+=next.value.byteLength;if(received>1024){{await reader.cancel();break}}}}
        return {{head:head.status,length:head.headers.get('content-length')??'',range:range.status,content_range:range.headers.get('content-range')??'',received}};
    }})()"#, serde_json::to_string(artifact)?), true).await?;
    ensure!(
        result.head == 200
            && result.length == bytes.to_string()
            && result.range == 206
            && result.received == 1024
            && result.content_range == format!("bytes 0-1023/{bytes}"),
        "public download mismatch: {result:?}"
    );
    Ok(())
}
