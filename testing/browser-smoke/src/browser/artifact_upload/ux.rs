//! Small post-transfer interactions against the installed Console.
use super::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Completed {
    large_receipt: Receipt,
}

pub(crate) async fn verify(
    public_base: &str,
    cdp_base: &str,
    completed: &Path,
    evidence_root: &Path,
) -> Result<()> {
    let completed: Completed = serde_json::from_slice(&fs::read(completed)?)?;
    let directory = evidence_root
        .join(git_revision()?)
        .join(format!("ux-{}", uuid::Uuid::now_v7()));
    fs::create_dir_all(&directory)?;
    let page_url = console_acceptance_url(public_base, "/artifacts");
    let (mut cdp, target, session) = open_headed_target(cdp_base, &page_url).await?;
    let result = tokio::time::timeout(Duration::from_secs(420), async {
        wait_for_document(&mut cdp, &session).await?;
        let hardware = hardware_check(&mut cdp, &session).await?;
        let policy = authorized_policy(&mut cdp, &session, &page_url).await?;
        let mut evidence = Evidence { schema: "veoveo.io/console-artifact-upload-ux-acceptance/v1", source_revision: git_revision()?, page_url: page_url.clone(), hardware, policy, preflight_only: false, steps: vec![], screenshots: vec![], accepted_before_reload: 0, elapsed_seconds: 0.0, large_receipt: None, csv_receipt: None };
        wait_selector(&mut cdp, &session, "button[aria-label='Open uploads']", Duration::from_secs(90)).await?;
        cdp.evaluate::<bool>(&session, "(()=>{document.querySelector('button[aria-label=\"Open uploads\"]').focus();return true})()", false).await?;
        key(&mut cdp, &session, "Enter", 13).await?;
        wait_selector(&mut cdp, &session, ".upload-panel", Duration::from_secs(10)).await?;
        cdp.evaluate::<bool>(&session, r#"(()=>{const p=document.querySelector('.upload-panel');const b=[...p.querySelectorAll('button:not([disabled]),input:not([tabindex="-1"])')].filter(n=>n.offsetParent!==null);b.at(-1).focus();return true})()"#, false).await?;
        key(&mut cdp, &session, "Tab", 9).await?;
        let trapped: bool = cdp.evaluate(&session, "document.querySelector('.upload-panel').contains(document.activeElement)&&document.querySelector('.app-shell')?.hasAttribute('inert')", false).await?;
        ensure!(trapped, "upload dialog did not contain keyboard focus");
        key(&mut cdp, &session, "Escape", 27).await?;
        let restored: bool = cdp.evaluate(&session, "document.activeElement?.getAttribute('aria-label')==='Open uploads'", false).await?;
        ensure!(restored, "Escape did not restore keyboard focus");
        println!("Native keyboard entry, focus containment, and restoration passed");

        // An active catalog filter must not interfere with a completed receipt's direct detail read.
        cdp.evaluate::<bool>(&session, r#"(()=>{const e=document.querySelector('input[placeholder="Search artifacts"]');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'acceptance-no-match');e.dispatchEvent(new Event('input',{bubbles:true}));return true})()"#, false).await?;
        click_label(&mut cdp, &session, "Open uploads").await?;
        let ready = wait_row(&mut cdp, &session, &completed.large_receipt.filename, Duration::from_secs(90), |r| r.phase == "Ready").await?;
        ensure!(ready.receipt.as_ref().is_some_and(|r| r.artifact_id == completed.large_receipt.artifact_id && r.byte_len == completed.large_receipt.byte_len), "completed upload did not recover its exact receipt");
        cdp.command("Browser.setPermission", serde_json::json!({"permission":{"name":"clipboard-read"},"setting":"granted","origin":public_base}), None).await?;
        click_label(&mut cdp, &session, &format!("Copy artifact URI for {}", completed.large_receipt.filename)).await?;
        wait_row(&mut cdp, &session, &completed.large_receipt.filename, Duration::from_secs(10), |r| r.text.contains("Artifact URI copied.")).await?;
        let copied: String = cdp.evaluate(&session, "navigator.clipboard.readText()", true).await?;
        ensure!(copied == completed.large_receipt.artifact_uri, "Copy URI did not copy the canonical receipt URI");
        click_label(&mut cdp, &session, &format!("View artifact {}", completed.large_receipt.filename)).await?;
        wait_selector(&mut cdp, &session, ".drawer-status", Duration::from_secs(60)).await?;
        let filter: String = cdp.evaluate(&session, "document.querySelector('input[placeholder=\"Search artifacts\"]').value", false).await?;
        ensure!(filter == "acceptance-no-match", "opening an upload changed catalog filters");
        screenshot(&mut cdp, &session, &directory, "filtered-artifact-desktop", &mut evidence).await?;
        viewport(&mut cdp, &session, 390, 844).await?;
        let close_visible: bool = cdp.evaluate(&session, "(()=>{const r=document.querySelector('button[aria-label=\"Close details\"]').getBoundingClientRect();return r.width>0&&r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight})()", false).await?;
        ensure!(close_visible, "long artifact filename hid the narrow drawer close control");
        screenshot(&mut cdp, &session, &directory, "filtered-artifact-narrow", &mut evidence).await?;
        step(&mut evidence, "Keyboard entry, focus containment/restoration, Copy URI, receipt recovery, and filtered direct artifact detail passed");

        cdp.command("Page.reload", serde_json::json!({}), Some(&session)).await?;
        wait_for_document(&mut cdp, &session).await?;
        wait_selector(&mut cdp, &session, "button[aria-label='Open uploads']", Duration::from_secs(90)).await?;
        click_label(&mut cdp, &session, "Open uploads").await?;
        let filename = format!("cancel-{}.bin", uuid::Uuid::now_v7());
        let file = directory.join(&filename);
        make_large_file(&file, 512 * 1024 * 1024, filename.as_bytes(), false)?;
        set_files(&mut cdp, &session, "#upload-files", &[&file]).await?;
        click(&mut cdp, &session, ".upload-start button").await?;
        let started = wait_row(&mut cdp, &session, &filename, Duration::from_secs(150), |r| r.phase == "Uploading" && r.accepted > 0).await?;
        row_screenshot(&mut cdp, &session, &directory, &filename, "transferring-narrow", &mut evidence).await?;
        viewport(&mut cdp, &session, 1440, 1000).await?;
        row_screenshot(&mut cdp, &session, &directory, &filename, "transferring-desktop", &mut evidence).await?;
        click_label(&mut cdp, &session, &format!("Pause {filename}")).await?;
        cdp.command("Page.reload", serde_json::json!({}), Some(&session)).await?;
        wait_for_document(&mut cdp, &session).await?;
        wait_selector(&mut cdp, &session, "button[aria-label='Open uploads']", Duration::from_secs(90)).await?;
        click_label(&mut cdp, &session, "Open uploads").await?;
        let restored = wait_row(&mut cdp, &session, &filename, Duration::from_secs(30), |r| r.phase == "Select file").await?;
        let wrong = directory.join("wrong"); fs::create_dir(&wrong)?;
        let wrong = wrong.join(&filename);
        make_large_file(&wrong, 512 * 1024 * 1024, b"different bytes", true)?;
        let selector = format!("input[aria-label={}]", serde_json::to_string(&format!("Original file for {filename}"))?);
        set_files(&mut cdp, &session, &selector, &[&wrong]).await?;
        let error = wait_row(&mut cdp, &session, &filename, Duration::from_secs(30), |r| r.text.contains("does not match the saved parts")).await?;
        ensure!(error.upload_id == started.upload_id && error.accepted == restored.accepted, "wrong-file recovery lost accepted progress");
        row_screenshot(&mut cdp, &session, &directory, &filename, "recoverable-error-desktop", &mut evidence).await?;
        viewport(&mut cdp, &session, 390, 844).await?;
        row_screenshot(&mut cdp, &session, &directory, &filename, "recoverable-error-narrow", &mut evidence).await?;
        click_label(&mut cdp, &session, &format!("Cancel {filename}")).await?;
        let cancelled = wait_row(&mut cdp, &session, &filename, Duration::from_secs(90), |r| r.phase == "Cancelled").await?;
        let state: String = cdp.evaluate(&session, &format!("(async()=>{{const r=await fetch('/console/api/artifact-uploads/'+{});return (await r.json()).state}})()", serde_json::to_string(cancelled.upload_id.as_ref().context("cancelled session missing identity")?)?), true).await?;
        ensure!(state == "cancelled" && cancelled.receipt.is_none(), "UI acknowledged cancellation without durable server settlement");
        row_screenshot(&mut cdp, &session, &directory, &filename, "cancelled-narrow", &mut evidence).await?;
        fs::remove_file(file)?; fs::remove_file(wrong)?;
        step(&mut evidence, "Narrow/desktop transfer and wrong-file recovery preserved progress; cancellation reached a durable terminal state");
        verify_finishing(&mut cdp, &session, &directory, &mut evidence).await?;
        evidence.large_receipt = ready.receipt;
        Ok::<_, anyhow::Error>(evidence)
    }).await.context("Console upload UX acceptance timed out");
    let close = close_target(&mut cdp, &target).await;
    let evidence = result??;
    close?;
    fs::write(
        directory.join("evidence.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;
    println!(
        "Console upload UX acceptance passed. Evidence: {}",
        directory.display()
    );
    Ok(())
}

async fn row_screenshot(
    cdp: &mut Cdp,
    session: &str,
    directory: &Path,
    filename: &str,
    name: &str,
    evidence: &mut Evidence,
) -> Result<()> {
    let visible: bool = cdp.evaluate(session, &format!(r#"(()=>{{const n=[...document.querySelectorAll('.upload-entry')].find(n=>n.querySelector('strong')?.textContent==={});if(!n)return false;n.scrollIntoView({{block:'center'}});const r=n.getBoundingClientRect(),p=document.querySelector('.upload-content').getBoundingClientRect();return r.top>=p.top&&r.bottom<=p.bottom&&r.left>=0&&r.right<=innerWidth}})()"#, serde_json::to_string(filename)?), false).await?;
    ensure!(
        visible,
        "upload row and recovery controls are not visible: {filename}"
    );
    screenshot(cdp, session, directory, name, evidence).await
}

async fn verify_finishing(
    cdp: &mut Cdp,
    session: &str,
    directory: &Path,
    evidence: &mut Evidence,
) -> Result<()> {
    let filename = format!("finish-{}.bin", uuid::Uuid::now_v7());
    let file = directory.join(&filename);
    let bytes = 256 * 1024 * 1024;
    make_large_file(&file, bytes, filename.as_bytes(), false)?;
    let sha = hash_file(&file)?;
    set_files(cdp, session, "#upload-files", &[&file]).await?;
    click(cdp, session, ".upload-start button").await?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(240);
    let mut finishing = false;
    let mut progress = tokio::time::Instant::now();
    let receipt = loop {
        hardware_check(cdp, session).await?;
        let current = row(cdp, session, &filename).await?;
        if current.phase == "Finishing upload" && !finishing {
            row_screenshot(
                cdp,
                session,
                directory,
                &filename,
                "finishing-narrow",
                evidence,
            )
            .await?;
            finishing = true;
        }
        if current.phase == "Ready" {
            break current
                .receipt
                .context("finishing fixture has no verified receipt")?;
        }
        if progress.elapsed() >= Duration::from_secs(20) {
            println!(
                "Finishing fixture: {} / {bytes} accepted bytes; {}",
                current.accepted, current.phase
            );
            progress = tokio::time::Instant::now();
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "finishing fixture did not settle: {current:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    ensure!(finishing, "narrow finishing state was not captured");
    ensure!(
        receipt.byte_len == bytes && receipt.sha256 == sha,
        "finishing receipt did not match independent bytes/hash"
    );
    row_screenshot(cdp, session, directory, &filename, "ready-narrow", evidence).await?;
    viewport(cdp, session, 1440, 1000).await?;
    row_screenshot(
        cdp,
        session,
        directory,
        &filename,
        "ready-desktop",
        evidence,
    )
    .await?;
    click(cdp, session, ".upload-panel header button").await?;
    for reload in [false, true] {
        if reload {
            cdp.command("Page.reload", serde_json::json!({}), Some(session))
                .await?;
            wait_for_document(cdp, session).await?;
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        loop {
            hardware_check(cdp, session).await?;
            let listed: bool = cdp.evaluate(session, &format!("[...document.querySelectorAll('tbody tr strong')].some(n=>n.textContent==={})", serde_json::to_string(&filename)?), false).await?;
            if listed {
                break;
            }
            ensure!(
                tokio::time::Instant::now() < deadline,
                "completed upload missing from artifact list; fresh snapshot={reload}"
            );
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
    fs::remove_file(file)?;
    step(
        evidence,
        &format!(
            "Visible narrow Finishing and Ready passed; {} has {bytes} verified bytes with SHA-256 {sha}, and appears in the list before and after reload",
            receipt.artifact_uri
        ),
    );
    Ok(())
}

async fn key(cdp: &mut Cdp, session: &str, key: &str, code: u32) -> Result<()> {
    for event in ["keyDown", "keyUp"] {
        // CDP needs the keyboard-generated text for Enter's default button action.
        let text = if event == "keyDown" && key == "Enter" {
            "\r"
        } else {
            ""
        };
        cdp.command(
            "Input.dispatchKeyEvent",
            serde_json::json!({"type":event,"key":key,"code":key,"windowsVirtualKeyCode":code,"text":text,"unmodifiedText":text}),
            Some(session),
        )
        .await?;
    }
    Ok(())
}
async fn viewport(cdp: &mut Cdp, session: &str, width: u32, height: u32) -> Result<()> {
    cdp.command(
        "Emulation.setDeviceMetricsOverride",
        serde_json::json!({"width":width,"height":height,"deviceScaleFactor":1,"mobile":false}),
        Some(session),
    )
    .await?;
    hardware_check(cdp, session).await?;
    Ok(())
}
