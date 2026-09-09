//! Public Console acceptance; browser scripts only operate DOM and project observations.
use super::*;
use std::io::{Read, Seek, SeekFrom, Write};

#[path = "artifact_upload/resume.rs"]
pub(crate) mod resume;

#[derive(Debug, Deserialize, Serialize)]
struct Receipt {
    upload_id: String,
    artifact_id: String,
    artifact_uri: String,
    filename: String,
    byte_len: u64,
    sha256: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    phase: String,
    text: String,
    upload_id: Option<String>,
    accepted: u64,
    receipt: Option<Receipt>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PolicyProbe {
    status: u16,
    allowed: bool,
    explanation: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Evidence {
    schema: &'static str,
    source_revision: String,
    page_url: String,
    hardware: HardwareIdentity,
    policy: PolicyProbe,
    preflight_only: bool,
    steps: Vec<String>,
    screenshots: Vec<(String, String)>,
    accepted_before_reload: u64,
    elapsed_seconds: f64,
    large_receipt: Option<Receipt>,
    csv_receipt: Option<Receipt>,
}

pub(crate) async fn verify(
    public_base: &str,
    cdp_base: &str,
    evidence_root: &Path,
    bytes: u64,
    timeout: Duration,
    preflight_only: bool,
) -> Result<()> {
    ensure!(
        Url::parse(public_base)?.scheme() == "https",
        "upload acceptance requires public HTTPS"
    );
    ensure!(
        bytes > u64::from(u32::MAX),
        "large transfer must exceed 4 GiB"
    );
    let run_id = uuid::Uuid::now_v7().to_string();
    let directory = evidence_root.join(git_revision()?).join(&run_id);
    fs::create_dir_all(&directory)?;
    let page_url = console_acceptance_url(public_base, "/artifacts");
    let (mut cdp, target, session) = open_headed_target(cdp_base, &page_url).await?;
    let result = tokio::time::timeout(
        timeout,
        run(
            &mut cdp,
            &session,
            &page_url,
            &directory,
            &run_id,
            bytes,
            preflight_only,
        ),
    )
    .await
    .context("Console artifact upload acceptance timed out");
    // Closing our target also stops local transfers on failure. Durable server state remains recoverable.
    let close = close_target(&mut cdp, &target).await;
    let evidence = result??;
    close?;
    let output = directory.join("evidence.json");
    fs::write(&output, serde_json::to_vec_pretty(&evidence)?)?;
    println!(
        "Console artifact upload {} passed. Evidence: {}",
        if preflight_only {
            "preflight"
        } else {
            "large-transfer acceptance"
        },
        output.display()
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run(
    cdp: &mut Cdp,
    session: &str,
    page_url: &str,
    directory: &Path,
    run_id: &str,
    bytes: u64,
    preflight_only: bool,
) -> Result<Evidence> {
    wait_for_document(cdp, session).await?;
    let hardware = hardware_check(cdp, session).await?;
    println!("Headed hardware browser preflight passed");
    let mut evidence = Evidence {
        schema: "veoveo.io/console-artifact-upload-acceptance/v1",
        source_revision: git_revision()?,
        page_url: page_url.into(),
        hardware,
        policy: authorized_policy(cdp, session, page_url).await?,
        preflight_only,
        steps: Vec::new(),
        screenshots: Vec::new(),
        accepted_before_reload: 0,
        elapsed_seconds: 0.0,
        large_receipt: None,
        csv_receipt: None,
    };
    println!("Authenticated public upload policy passed");
    wait_selector(
        cdp,
        session,
        "button[aria-label='Open uploads']",
        Duration::from_secs(60),
    )
    .await?;
    click(cdp, session, "button[aria-label='Open uploads']").await?;
    wait_selector(cdp, session, "#upload-files", Duration::from_secs(30)).await?;
    screenshot(cdp, session, directory, "selection", &mut evidence).await?;
    step(
        &mut evidence,
        "Headed hardware graphics and authenticated public policy passed",
    );
    if preflight_only {
        return Ok(evidence);
    }

    let fixture_dir = directory.join("fixtures");
    fs::create_dir(&fixture_dir)?;
    let large_name = format!("upload-acceptance-{run_id}.bin");
    let csv_name = format!("upload-acceptance-{run_id}.csv");
    let large_path = fixture_dir.join(&large_name);
    let csv_path = fixture_dir.join(&csv_name);
    let invalid_path = fixture_dir.join("unsupported.html");
    make_large_file(&large_path, bytes, run_id.as_bytes(), false)?;
    fs::write(&csv_path, b"name,value\nalpha,1\nbeta,2\n")?;
    fs::write(&invalid_path, b"<html>unsupported MIME fixture</html>")?;
    let hash_path = large_path.clone();
    let expected_hash = tokio::task::spawn_blocking(move || hash_file(&hash_path));
    set_files(
        cdp,
        session,
        "#upload-files",
        &[&large_path, &csv_path, &invalid_path],
    )
    .await?;
    let large = row(cdp, session, &large_name).await?;
    ensure!(
        large.phase == "Selected" && large.upload_id.is_none(),
        "selection started transfer: {large:?}"
    );
    ensure!(
        row(cdp, session, "unsupported.html").await?.phase == "Needs attention",
        "invalid type was admitted"
    );
    set_files(cdp, session, "#upload-files", &[&large_path]).await?;
    let count: u64 = cdp.evaluate(session, &format!("[...document.querySelectorAll('.upload-entry-title strong')].filter(n=>n.textContent==={}).length", serde_json::to_string(&large_name)?), false).await?;
    ensure!(count == 1, "accidental duplicate selection was retained");
    click(cdp, session, ".upload-start button").await?;
    let started = tokio::time::Instant::now();
    let partial = wait_row(cdp, session, &large_name, Duration::from_secs(180), |r| {
        r.accepted >= 64 * 1024 * 1024
    })
    .await?;
    ensure!(
        partial.accepted < bytes,
        "large upload completed before pause/resume acceptance"
    );
    screenshot(cdp, session, directory, "transferring", &mut evidence).await?;
    click_label(cdp, session, &format!("Pause {large_name}")).await?;
    wait_row(cdp, session, &large_name, Duration::from_secs(10), |r| {
        r.phase == "Paused"
    })
    .await?;
    evidence.accepted_before_reload = partial.accepted;
    step(
        &mut evidence,
        &format!("Paused with {} durably accepted bytes", partial.accepted),
    );
    cdp.command(
        "Page.reload",
        serde_json::json!({"ignoreCache":true}),
        Some(session),
    )
    .await?;
    wait_for_document(cdp, session).await?;
    hardware_check(cdp, session).await?;
    wait_selector(
        cdp,
        session,
        "button[aria-label='Open uploads']",
        Duration::from_secs(60),
    )
    .await?;
    click(cdp, session, "button[aria-label='Open uploads']").await?;
    let recovered = wait_row(cdp, session, &large_name, Duration::from_secs(60), |r| {
        r.phase == "Select file" && r.accepted > 0
    })
    .await?;
    ensure!(
        recovered.upload_id == partial.upload_id && recovered.accepted >= partial.accepted,
        "reload lost accepted bytes or identity"
    );
    let wrong_dir = fixture_dir.join("wrong");
    fs::create_dir(&wrong_dir)?;
    let wrong_path = wrong_dir.join(&large_name);
    make_large_file(&wrong_path, bytes, b"wrong file", true)?;
    let selector = format!(
        "input[aria-label={}]",
        serde_json::to_string(&format!("Original file for {large_name}"))?
    );
    set_files(cdp, session, &selector, &[&wrong_path]).await?;
    let mismatch = wait_row(cdp, session, &large_name, Duration::from_secs(90), |r| {
        r.text.contains("does not match the saved parts")
    })
    .await?;
    ensure!(
        mismatch.upload_id == recovered.upload_id && mismatch.accepted == recovered.accepted,
        "wrong file changed saved progress"
    );
    screenshot(
        cdp,
        session,
        directory,
        "wrong-file-recovery",
        &mut evidence,
    )
    .await?;
    step(
        &mut evidence,
        "Reload retained identity; same-named wrong file rejected with accepted progress intact",
    );
    set_files(cdp, session, &selector, &[&large_path]).await?;
    wait_row(cdp, session, &large_name, Duration::from_secs(90), |r| {
        r.phase == "Uploading" && r.accepted > recovered.accepted
    })
    .await?;
    click(cdp, session, ".upload-panel header button").await?;
    cdp.evaluate::<bool>(
        session,
        "(()=>{location.hash='#/tasks';return true})()",
        false,
    )
    .await?;
    let before_navigation = persisted_row(cdp, session, &large_name).await?.accepted;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    loop {
        hardware_check(cdp, session).await?;
        if persisted_row(cdp, session, &large_name).await?.accepted > before_navigation {
            break;
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "navigation stopped transfer"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    step(
        &mut evidence,
        "Closing the panel and navigating preserved transfer",
    );
    cdp.evaluate::<bool>(
        session,
        "(()=>{location.hash='#/artifacts';return true})()",
        false,
    )
    .await?;
    click(cdp, session, "button[aria-label='Open uploads']").await?;
    let mut finishing_captured = false;
    let mut last_progress = tokio::time::Instant::now();
    loop {
        hardware_check(cdp, session).await?;
        let current = row(cdp, session, &large_name).await?;
        if last_progress.elapsed() >= Duration::from_secs(30) {
            println!(
                "Large upload: {} / {} accepted bytes; {}",
                current.accepted, bytes, current.phase
            );
            last_progress = tokio::time::Instant::now();
        }
        if current.phase == "Finishing upload" && !finishing_captured {
            screenshot(cdp, session, directory, "finishing", &mut evidence).await?;
            finishing_captured = true;
            cdp.command(
                "Page.reload",
                serde_json::json!({"ignoreCache":true}),
                Some(session),
            )
            .await?;
            wait_for_document(cdp, session).await?;
            hardware_check(cdp, session).await?;
            wait_selector(
                cdp,
                session,
                "button[aria-label='Open uploads']",
                Duration::from_secs(60),
            )
            .await?;
            click(cdp, session, "button[aria-label='Open uploads']").await?;
        }
        if current.phase == "Ready" {
            evidence.large_receipt = current.receipt;
            break;
        }
        ensure!(
            !["Needs attention", "Sign in to continue", "Cancelled"]
                .contains(&current.phase.as_str()),
            "large upload failed: {current:?}"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    evidence.elapsed_seconds = started.elapsed().as_secs_f64();
    let receipt = evidence
        .large_receipt
        .as_ref()
        .context("Ready row has no receipt")?;
    ensure!(
        receipt.byte_len == bytes
            && receipt.filename == large_name
            && receipt.sha256 == expected_hash.await??,
        "large file receipt failed length or independent SHA-256 verification"
    );
    ensure!(
        receipt.artifact_uri == format!("artifact://{}", receipt.artifact_id),
        "noncanonical artifact URI"
    );
    // The CSV may have lost its local handle when reloading while the large file was active.
    let csv = row(cdp, session, &csv_name).await?;
    if csv.phase != "Ready" {
        let selector = format!(
            "input[aria-label={}]",
            serde_json::to_string(&format!("Original file for {csv_name}"))?
        );
        set_files(cdp, session, &selector, &[&csv_path]).await?;
    }
    let csv = wait_row(cdp, session, &csv_name, Duration::from_secs(90), |r| {
        r.phase == "Ready"
    })
    .await?;
    evidence.csv_receipt = csv.receipt;
    ensure!(
        evidence
            .csv_receipt
            .as_ref()
            .context("CSV missing receipt")?
            .sha256
            == hash_file(&csv_path)?,
        "CSV receipt integrity failed"
    );
    screenshot(cdp, session, directory, "ready-desktop", &mut evidence).await?;
    cdp.command(
        "Emulation.setDeviceMetricsOverride",
        serde_json::json!({"width":390,"height":844,"deviceScaleFactor":1,"mobile":false}),
        Some(session),
    )
    .await?;
    screenshot(cdp, session, directory, "ready-narrow", &mut evidence).await?;
    click_label(cdp, session, &format!("View artifact {large_name}")).await?;
    wait_selector(cdp, session, ".drawer-status", Duration::from_secs(60)).await?;
    screenshot(cdp, session, directory, "artifact-drawer", &mut evidence).await?;
    step(
        &mut evidence,
        "Large and CSV receipts match independent file hashes; completed-row View opens ArtifactDrawer",
    );
    fs::remove_dir_all(&fixture_dir)?;
    Ok(evidence)
}

async fn hardware_check(cdp: &mut Cdp, session: &str) -> Result<HardwareIdentity> {
    assert_page_visible(cdp, session).await?;
    let hardware: HardwareIdentity = cdp.evaluate(session, HARDWARE_PREFLIGHT, true).await?;
    hardware.validate()?;
    cdp.assert_no_software_renderer_events()?;
    Ok(hardware)
}

async fn authorized_policy(cdp: &mut Cdp, session: &str, page_url: &str) -> Result<PolicyProbe> {
    let mut current = policy(cdp, session).await?;
    if current.status == 401 || !current.allowed {
        // A fresh OAuth authorization picks up newly admitted scopes using the existing IdP session.
        let path = Url::parse(page_url)?;
        let return_path = format!(
            "{}?{}#{}",
            path.path(),
            path.query().unwrap_or_default(),
            path.fragment().unwrap_or_default()
        );
        let mut login = path.clone();
        login.set_path("/auth/login");
        login.set_fragment(None);
        login
            .query_pairs_mut()
            .clear()
            .append_pair("return_to", &return_path);
        cdp.command(
            "Page.navigate",
            serde_json::json!({"url":login.as_str()}),
            Some(session),
        )
        .await?;
        wait_for_requested_document(cdp, session, page_url).await?;
        hardware_check(cdp, session).await?;
        current = policy(cdp, session).await?;
    }
    ensure!(
        current.status == 200 && current.allowed,
        "public upload policy unavailable: {:?}",
        current
    );
    Ok(current)
}

async fn policy(cdp: &mut Cdp, session: &str) -> Result<PolicyProbe> {
    cdp.evaluate(session, r#"(async()=>{const r=await fetch('/console/api/artifact-uploads/policy');let p={};try{p=await r.json()}catch{}return {status:r.status,allowed:p.allowed===true,explanation:p.explanation??p.message??'No policy response'}})()"#, true).await
}

fn step(evidence: &mut Evidence, message: &str) {
    println!("{message}");
    evidence.steps.push(message.into());
}

async fn screenshot(
    cdp: &mut Cdp,
    session: &str,
    directory: &Path,
    name: &str,
    evidence: &mut Evidence,
) -> Result<()> {
    hardware_check(cdp, session).await?;
    let path = directory.join(format!("{name}.png"));
    let sha = capture_screenshot(cdp, session, &path).await?;
    evidence.screenshots.push((path.display().to_string(), sha));
    Ok(())
}

async fn wait_selector(
    cdp: &mut Cdp,
    session: &str,
    selector: &str,
    timeout: Duration,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        hardware_check(cdp, session).await?;
        let exists: bool = cdp
            .evaluate(
                session,
                &format!(
                    "Boolean(document.querySelector({}))",
                    serde_json::to_string(selector)?
                ),
                false,
            )
            .await?;
        if exists {
            return Ok(());
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Console control missing: {selector}"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn click(cdp: &mut Cdp, session: &str, selector: &str) -> Result<()> {
    let clicked: bool = cdp.evaluate(session, &format!("(()=>{{const e=document.querySelector({});if(!e||e.disabled)return false;e.click();return true}})()", serde_json::to_string(selector)?), false).await?;
    ensure!(clicked, "enabled Console control missing: {selector}");
    Ok(())
}

async fn click_label(cdp: &mut Cdp, session: &str, label: &str) -> Result<()> {
    click(
        cdp,
        session,
        &format!("button[aria-label={}]", serde_json::to_string(label)?),
    )
    .await
}

async fn set_files(cdp: &mut Cdp, session: &str, selector: &str, paths: &[&Path]) -> Result<()> {
    let document = cdp
        .command("DOM.getDocument", serde_json::json!({}), Some(session))
        .await?;
    let root = document
        .pointer("/root/nodeId")
        .context("missing DOM root")?;
    let node = cdp
        .command(
            "DOM.querySelector",
            serde_json::json!({"nodeId":root,"selector":selector}),
            Some(session),
        )
        .await?;
    let node_id = node
        .get("nodeId")
        .and_then(Value::as_u64)
        .filter(|id| *id > 0)
        .context("file input missing")?;
    let files = paths
        .iter()
        .map(|path| fs::canonicalize(path).map(|path| path.display().to_string()))
        .collect::<std::io::Result<Vec<_>>>()?;
    cdp.command(
        "DOM.setFileInputFiles",
        serde_json::json!({"nodeId":node_id,"files":files}),
        Some(session),
    )
    .await?;
    Ok(())
}

async fn row(cdp: &mut Cdp, session: &str, filename: &str) -> Result<Row> {
    let mut result = persisted_row(cdp, session, filename).await?;
    #[derive(Deserialize)]
    struct DomRow {
        phase: String,
        text: String,
    }
    let dom: DomRow = cdp.evaluate(session, &format!(r#"(()=>{{const n=[...document.querySelectorAll('.upload-entry')].find(n=>n.querySelector('strong')?.textContent==={});return {{phase:n?.dataset.uploadPhase??'',text:n?.innerText??''}}}})()"#, serde_json::to_string(filename)?), false).await?;
    result.phase = dom.phase;
    result.text = dom.text;
    Ok(result)
}

async fn persisted_row(cdp: &mut Cdp, session: &str, filename: &str) -> Result<Row> {
    cdp.evaluate(session, &format!(r#"(()=>{{for(const k of Object.keys(localStorage)){{if(!k.startsWith('veoveo.uploads.v1:'))continue;const n=JSON.parse(localStorage.getItem(k)).find(n=>n.descriptor.filename==={});if(n)return {{phase:'',text:'',uploadId:n.uploadId??null,accepted:n.accepted,receipt:n.receipt??null}}}}return {{phase:'',text:'',uploadId:null,accepted:0,receipt:null}}}})()"#, serde_json::to_string(filename)?), false).await
}

async fn wait_row(
    cdp: &mut Cdp,
    session: &str,
    filename: &str,
    timeout: Duration,
    predicate: impl Fn(&Row) -> bool,
) -> Result<Row> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        hardware_check(cdp, session).await?;
        let current = row(cdp, session, filename).await?;
        if predicate(&current) {
            return Ok(current);
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "upload did not reach expected state: {current:?}"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn make_large_file(path: &Path, bytes: u64, marker: &[u8], wrong: bool) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    file.set_len(bytes)?;
    file.write_all(marker)?;
    if wrong {
        for offset in (0..bytes).step_by(1024 * 1024) {
            file.seek(SeekFrom::Start(offset))?;
            file.write_all(&[0xa5])?;
        }
    }
    file.seek(SeekFrom::End(-(marker.len() as i64)))?;
    file.write_all(marker)?;
    file.sync_all()?;
    Ok(())
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut buffer = vec![0; 1024 * 1024];
    let mut hash = Sha256::new();
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    Ok(hex::encode(hash.finalize()))
}

fn git_revision() -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()?;
    ensure!(output.status.success(), "Git revision lookup failed");
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}
