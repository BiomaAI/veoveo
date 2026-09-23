//! Installed Markdown selection, receipt, download and reload through Workspace.
use super::*;

pub(crate) async fn verify(base: &str, endpoint: &str, output: &Path) -> Result<()> {
    ensure!(
        Url::parse(base)?.scheme() == "https",
        "installed HTTPS required"
    );
    let run = uuid::Uuid::now_v7();
    let directory = output.join(run.to_string());
    fs::create_dir_all(&directory)?;
    let filename = format!("workspace-notes-{run}.md");
    let file = directory.join(&filename);
    let content = "# Workspace notes\n\nMarkdown upload acceptance.\n\n- UTF-8: café\n";
    fs::write(&file, content)?;
    let url = format!("{}/workspace/", base.trim_end_matches('/'));
    let (mut cdp, target, session) = open_headed_target(endpoint, &url).await?;
    let acceptance = async {
        let hardware = hardware_check(&mut cdp, &session).await?;
        wait_selector(&mut cdp, &session, ".workspace .sidebar", Duration::from_secs(30)).await?;
        open_uploads(&mut cdp, &session).await?;
        set_files(&mut cdp, &session, "#upload-files", &[&file]).await?;
        let selected = wait_row(&mut cdp, &session, &filename, Duration::from_secs(20), |row| row.phase == "Selected").await?;
        ensure!(selected.text.contains("text/markdown"), "Markdown media type missing");
        click(&mut cdp, &session, ".upload-start button").await?;
        let ready = wait_row(&mut cdp, &session, &filename, Duration::from_secs(90), |row| row.phase == "Ready").await?;
        let receipt = ready.receipt.context("server receipt missing")?;
        ensure!(receipt.filename == filename && receipt.byte_len == content.len() as u64);
        let actual: String = cdp.evaluate(&session, &format!(
            "(async()=>{{const r=await fetch('/workspace/api/artifacts/'+{}+'/download');if(!r.ok)throw Error('download '+r.status);return r.text();}})()",
            serde_json::to_string(&receipt.artifact_id)?), true).await?;
        ensure!(actual == content, "download differs from selected Markdown");
        cdp.command("Page.reload", serde_json::json!({"ignoreCache":true}), Some(&session)).await?;
        wait_selector(&mut cdp, &session, ".workspace .sidebar", Duration::from_secs(30)).await?;
        open_uploads(&mut cdp, &session).await?;
        let restored = wait_row(&mut cdp, &session, &filename, Duration::from_secs(30), |row| row.phase == "Ready").await?;
        ensure!(restored.receipt.context("restored receipt missing")?.artifact_id == receipt.artifact_id);
        hardware_check(&mut cdp, &session).await?;
        let screenshot = capture_screenshot(&mut cdp, &session, &directory.join("markdown-ready.png")).await?;
        fs::write(directory.join("evidence.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "schema":"veoveo.io/workspace-markdown-acceptance/v1", "sourceRevision":git_revision()?,
            "url":url,"hardware":hardware,"receipt":receipt,"downloadVerified":true,
            "reloadVerified":true,"screenshotSha256":screenshot
        }))?)?;
        Ok::<_, anyhow::Error>(())
    }.await;
    let close = close_target(&mut cdp, &target).await;
    acceptance?;
    close?;
    println!("Workspace Markdown evidence: {}", directory.display());
    Ok(())
}

async fn open_uploads(cdp: &mut Cdp, session: &str) -> Result<()> {
    hardware_check(cdp, session).await?;
    ensure!(cdp.evaluate::<bool>(session, "(()=>{const b=[...document.querySelectorAll('.sidebar button')].find(b=>b.textContent.trim().startsWith('Uploads'));if(!b)return false;b.click();return true})()", false).await?);
    wait_selector(cdp, session, "#upload-files", Duration::from_secs(30)).await
}
