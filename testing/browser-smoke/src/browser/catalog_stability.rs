//! Observe installed catalog refreshes and exercise actual sidebar navigation.
use super::*;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Sample {
    status: u16,
    elapsed_ms: u64,
    app_count: usize,
    nav_buttons: usize,
    uris: BTreeSet<String>,
    degradations: Vec<ConsoleAppCatalogDegradation>,
    badges: Vec<String>,
}

pub(super) async fn verify(
    cdp: &mut Cdp,
    target: &str,
    session: &str,
    expected: &BTreeSet<&str>,
    directory: &Path,
) -> Result<()> {
    cdp.evaluate::<bool>(session, r#"(()=>{
      window.__catalogChurn=[];
      window.__catalogExpected=document.querySelectorAll('button.nav-app').length;
      window.__catalogObserver=new MutationObserver(()=>{
        const badges=[...document.querySelectorAll('.nav-app-unavailable')].map(e=>e.textContent);
        const count=document.querySelectorAll('button.nav-app').length;
        if(badges.length||count!==window.__catalogExpected)window.__catalogChurn.push({badges,count});
      });
      window.__catalogObserver.observe(document.querySelector('.sidebar'),{subtree:true,childList:true,characterData:true});
      return true;
    })()"#, false).await?;
    let mut samples = Vec::new();
    // Six private-catalog TTLs, including every intermediate UI mutation.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let hardware: HardwareIdentity = cdp.evaluate(session, HARDWARE_PREFLIGHT, true).await?;
        hardware.validate()?;
        cdp.assert_no_software_renderer_events()?;
        let sample: Sample = cdp
            .evaluate(
                session,
                r#"(async()=>{
          const started=performance.now();
          const r=await fetch('/console/api/apps',{credentials:'same-origin'});
          const body=await r.json();
          return {status:r.status,elapsedMs:Math.round(performance.now()-started),appCount:(body.apps||[]).length,
            navButtons:document.querySelectorAll('button.nav-app').length,
            uris:(body.apps||[]).map(a=>a.resourceUri),
            degradations:body.degradations||[],
            badges:[...document.querySelectorAll('.nav-app-unavailable')].map(e=>e.textContent)};
        })()"#,
                true,
            )
            .await?;
        fs::write(
            directory.join("latest-catalog-sample.json"),
            serde_json::to_vec_pretty(&sample)?,
        )?;
        // Gateway refresh may report a pending upstream discovery while the
        // complete cached App set remains usable. It must not mark an App
        // unavailable or lose any catalog member during that refresh.
        ensure!(
            sample.status == 200
                && sample
                    .degradations
                    .iter()
                    .all(|failure| failure.code == "discovery_pending")
                && sample.badges.is_empty(),
            "App catalog became incomplete: {}",
            serde_json::to_string(&sample)?
        );
        ensure!(
            sample.app_count == sample.uris.len(),
            "App catalog has duplicate entries: {}",
            serde_json::to_string(&sample)?
        );
        ensure!(
            sample.nav_buttons == expected.len(),
            "Sidebar changed during catalog refresh: {}",
            serde_json::to_string(&sample)?
        );
        ensure!(
            sample
                .uris
                .iter()
                .all(|uri| expected.contains(uri.as_str()))
                && expected
                    .iter()
                    .filter(|uri| !sample.uris.contains(**uri))
                    .all(|uri| {
                        uri.strip_prefix("ui://")
                            .and_then(|rest| rest.split_once('/'))
                            .is_some_and(|(server, _)| {
                                sample.degradations.iter().any(|failure| {
                                    failure.server == server && failure.code == "discovery_pending"
                                })
                            })
                    }),
            "App catalog changed beyond declared pending discoveries: {}",
            serde_json::to_string(&sample)?
        );
        samples.push(sample);
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    let churn: Vec<serde_json::Value> = cdp
        .evaluate(
            session,
            "(()=>{window.__catalogObserver.disconnect();return window.__catalogChurn;})()",
            false,
        )
        .await?;
    ensure!(churn.is_empty(), "Sidebar availability churn: {churn:?}");

    let mut routes = Vec::new();
    for (server, title, route) in [
        ("datasheet", "Workbench", "#/apps/datasheet/workbench"),
        ("uav-sim", "Live Cameras", "#/apps/uav-sim/live"),
    ] {
        let clicked: bool = cdp.evaluate(session, &format!(r#"(()=>{{
          const g=[...document.querySelectorAll('.nav-app-group')].find(g=>g.querySelector('summary span')?.textContent.trim().toLowerCase()==={});
          if(!g)return false;g.open=true;
          const b=[...g.querySelectorAll('button.nav-app')].find(b=>b.textContent.trim()==={});
          if(!b)return false;b.click();return true;
        }})()"#, serde_json::to_string(&server.replace('-', " "))?, serde_json::to_string(title)?), false).await?;
        ensure!(clicked, "Sidebar App button missing for {server}/{title}");
        wait_for_console_app_body(cdp, target, session, server, title).await?;
        let hash: String = cdp.evaluate(session, "location.hash", false).await?;
        ensure!(hash == route, "App click produced {hash}, expected {route}");
        cdp.command(
            "Page.reload",
            serde_json::json!({"ignoreCache":true}),
            Some(session),
        )
        .await?;
        wait_for_document(cdp, session).await?;
        let hardware: HardwareIdentity = cdp.evaluate(session, HARDWARE_PREFLIGHT, true).await?;
        hardware.validate()?;
        wait_for_console_app_body(cdp, target, session, server, title).await?;
        let restored: String = cdp.evaluate(session, "location.hash", false).await?;
        ensure!(restored == route, "Reload changed the clean App route");
        routes.push(route);
    }
    fs::write(
        directory.join("catalog-stability.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema":"veoveo.io/console-catalog-stability/v1", "samples":samples,
            "sidebarChurn":churn,"clickedAndReloaded":routes
        }))?,
    )?;
    Ok(())
}
