//! Installed Workspace acceptance. Synthetic microphone audio feeds the real
//! AudioWorklet, authenticated PCM route and CUDA worker; no API responses are mocked.
use super::*;
use std::time::{Duration, Instant};

pub(crate) async fn verify(
    base: &str,
    endpoint: &str,
    output: &Path,
    fixture: &Path,
) -> Result<()> {
    ensure!(
        Url::parse(base)?.scheme() == "https",
        "installed HTTPS required"
    );
    let fixture = fixture.canonicalize()?;
    let directory = output.join(uuid::Uuid::now_v7().to_string());
    fs::create_dir_all(&directory)?;
    // Persistent queues reject duplicate files. Give this run its own fixture
    // without clearing another person's upload history.
    let upload = directory.join(format!("speech-{}.wav", uuid::Uuid::now_v7()));
    fs::copy(&fixture, &upload)?;
    let upload = upload.canonicalize()?;
    let page = format!("{}/workspace/", base.trim_end_matches('/'));
    let (mut cdp, target, session) = open_headed_target(endpoint, &page).await?;
    let result = tokio::time::timeout(
        Duration::from_secs(180),
        run(&mut cdp, &target, &session, &directory, &upload),
    )
    .await;
    let close = close_target(&mut cdp, &target).await;
    result.context("Speech browser acceptance timed out")??;
    close?;
    println!("Speech browser evidence: {}", directory.display());
    Ok(())
}

async fn hardware(cdp: &mut Cdp, session: &str) -> Result<HardwareIdentity> {
    let identity: HardwareIdentity = cdp.evaluate(session, HARDWARE_PREFLIGHT, true).await?;
    identity.validate()?;
    Ok(identity)
}

async fn wait(cdp: &mut Cdp, session: &str, expression: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        hardware(cdp, session).await?;
        if cdp.evaluate::<bool>(session, expression, true).await? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            let diagnostic: serde_json::Value = cdp.evaluate(session,
                "({alerts:[...document.querySelectorAll('[role=alert]')].map(e=>e.textContent),dictation:document.querySelector('.dictation')?.textContent,requests:window.speechAcceptance?.requests?.slice(-12)})", false).await?;
            anyhow::bail!("Workspace did not settle: {expression}; {diagnostic}");
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn click(cdp: &mut Cdp, session: &str, text: &str) -> Result<()> {
    let text = serde_json::to_string(text)?;
    let expression = format!(
        "(() => {{ const b=[...document.querySelectorAll('button,a')].find(x=>x.textContent.trim()==={text} && !x.disabled && x.getClientRects().length); if(!b)return false; b.click();return true; }})()"
    );
    wait(cdp, session, &expression).await
}

async fn fill(cdp: &mut Cdp, session: &str, selector: &str, value: &str) -> Result<()> {
    let script = format!(
        "(() => {{ const e=document.querySelector({}); if(!e)return false; Object.getOwnPropertyDescriptor(e.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:HTMLInputElement.prototype,'value').set.call(e,{});e.dispatchEvent(new Event('input',{{bubbles:true}}));return true; }})()",
        serde_json::to_string(selector)?,
        serde_json::to_string(value)?
    );
    wait(cdp, session, &script).await
}

async fn run(
    cdp: &mut Cdp,
    target: &str,
    session: &str,
    output: &Path,
    fixture: &Path,
) -> Result<()> {
    let window = cdp
        .command(
            "Browser.getWindowForTarget",
            serde_json::json!({"targetId":target}),
            None,
        )
        .await?;
    ensure!(
        window["bounds"]["width"].as_u64().unwrap_or(0) > 0
            && window["bounds"]["height"].as_u64().unwrap_or(0) > 0,
        "headed window required"
    );
    let initial_hardware = hardware(cdp, session).await?;
    wait(cdp, session, "Boolean(document.querySelector('.new-chat'))").await?;
    click(cdp, session, "New chat").await?;
    fill(
        cdp,
        session,
        "input[placeholder='e.g. Planning our next release']",
        "Speech acceptance · private draft",
    )
    .await?;
    click(cdp, session, "Create chat").await?;
    wait(
        cdp,
        session,
        "Boolean(document.querySelector('textarea[aria-label=Message]'))",
    )
    .await?;
    let chat_url: String = cdp.evaluate(session, "location.href", false).await?;
    let chat_id = Url::parse(&chat_url)?
        .query_pairs()
        .find(|(key, _)| key == "chat")
        .context("new chat URL has no chat id")?
        .1
        .into_owned();
    let card = format!(
        "document.querySelector({})",
        serde_json::to_string(&format!(
            "article[aria-label=\"Activity: speech__transcribe\"]:has(a[href=\"/workspace/?chat={chat_id}&panel=activity\"])"
        ))?
    );
    fill(
        cdp,
        session,
        "textarea[aria-label=Message]",
        "Review first.",
    )
    .await?;
    let fixture_base64 = serde_json::to_string(&STANDARD.encode(fs::read(fixture)?))?;
    let install = format!(
        r#"(async()=>{{
      const bytes=Uint8Array.from(atob({fixture_base64}), x=>x.charCodeAt(0));
      const decoder=new AudioContext();const audio=await decoder.decodeAudioData(bytes.buffer);await decoder.close();
      window.speechAcceptance={{streams:[],contexts:[],requests:[]}};
      const originalFetch=window.fetch.bind(window);
      window.fetch=async(input,init)=>{{const url=typeof input==='string'?input:input.url;if(url.includes('/speech/dictation/')&&url.endsWith('/chunks/2')&&!window.speechAcceptance.delayed){{window.speechAcceptance.delayed=true;await new Promise(resolve=>setTimeout(resolve,2500));}}const response=await originalFetch(input,init);if(url.includes('/workspace/api/'))window.speechAcceptance.requests.push({{path:new URL(url,location.href).pathname,method:init?.method??'GET',status:response.status}});return response;}};
      navigator.mediaDevices.getUserMedia=async()=>{{const context=new AudioContext();await context.resume();const destination=context.createMediaStreamDestination();const source=context.createBufferSource();source.buffer=audio;source.connect(destination);source.start(context.currentTime+0.3);window.speechAcceptance.streams.push(destination.stream);window.speechAcceptance.contexts.push(context);return destination.stream;}};
      return true;
    }})()"#
    );
    ensure!(cdp.evaluate::<bool>(session, &install, true).await?);
    click(cdp, session, "Dictate").await?;
    wait(
        cdp,
        session,
        "document.body.innerText.includes('Listening · private draft')",
    )
    .await?;
    ensure!(
        cdp.evaluate::<bool>(
            session,
            "document.querySelector('button[aria-label=\"Send message\"]').disabled",
            false
        )
        .await?
    );
    wait(cdp, session, "(document.querySelector('.dictation-preview')?.textContent.length??0)>15 && !document.querySelector('.dictation-preview').textContent.startsWith('Text appears')").await?;
    // The source fixture lasts 7.4 seconds; finish after the browser has supplied it.
    tokio::time::sleep(Duration::from_secs(5)).await;
    hardware(cdp, session).await?;
    click(cdp, session, "Stop dictation").await?;
    wait(cdp, session, "document.querySelector('textarea[aria-label=Message]').value.toLowerCase().includes('old portrait')").await?;
    let reviewed: String = cdp
        .evaluate(
            session,
            "document.querySelector('textarea[aria-label=Message]').value",
            false,
        )
        .await?;
    ensure!(
        reviewed.starts_with("Review first."),
        "dictation replaced typed edits"
    );
    ensure!(cdp.evaluate::<bool>(session, "window.speechAcceptance.delayed && window.speechAcceptance.streams.every(s=>s.getTracks().every(t=>t.readyState==='ended')) && !window.speechAcceptance.requests.some(r=>r.method==='POST' && ['messages','runs','operations'].some(p=>r.path.endsWith('/'+p)))", false).await?, "dictation sent a message, invoked an agent, or kept capture alive");
    let draft_shot = capture_screenshot(cdp, session, &output.join("review.png")).await?;
    click(cdp, session, "Dictate").await?;
    wait(
        cdp,
        session,
        "document.body.innerText.includes('Listening · private draft')",
    )
    .await?;
    click(cdp, session, "Cancel").await?;
    wait(
        cdp,
        session,
        "window.speechAcceptance.streams.every(s=>s.getTracks().every(t=>t.readyState==='ended'))",
    )
    .await?;
    let after_cancel: String = cdp
        .evaluate(
            session,
            "document.querySelector('textarea[aria-label=Message]').value",
            false,
        )
        .await?;
    ensure!(
        after_cancel == reviewed,
        "cancel changed the reviewed draft"
    );
    ensure!(cdp.evaluate::<bool>(session, "(()=>{document.querySelector('button[aria-label=\"Send message\"]').click();return true})()", false).await?);
    wait(
        cdp,
        session,
        "document.querySelector('textarea[aria-label=Message]').value===''",
    )
    .await?;
    ensure!(cdp.evaluate::<bool>(session, "!window.speechAcceptance.requests.some(r=>r.method==='POST' && r.path.endsWith('/runs'))", false).await?, "unselected agent invoked");
    click(cdp, session, "Transcribe a recording").await?;
    let previous_uploads: Vec<String> = cdp.evaluate(session,
        "[...document.querySelectorAll('select[aria-label=\"Recording to transcribe\"] option')].map(o=>o.value)", false).await?;
    click(cdp, session, "Upload recording").await?;
    wait(
        cdp,
        session,
        "Boolean(document.querySelector('#upload-files'))",
    )
    .await?;
    let node = cdp.command("Runtime.evaluate", serde_json::json!({"expression":"document.querySelector('#upload-files')","returnByValue":false}), Some(session)).await?;
    let object = value_string(&node, "/result/objectId")?;
    cdp.command(
        "DOM.setFileInputFiles",
        serde_json::json!({"objectId":object,"files":[fixture]}),
        Some(session),
    )
    .await?;
    click(cdp, session, "Upload 1 file").await?;
    wait(
        cdp,
        session,
        &format!(
            "document.body.innerText.includes('Ready') && document.body.innerText.includes({})",
            serde_json::to_string(
                &fixture
                    .file_name()
                    .context("fixture filename")?
                    .to_string_lossy()
            )?
        ),
    )
    .await?;
    ensure!(cdp.evaluate::<bool>(session, "(()=>{const b=document.querySelector('button[aria-label=\"Close uploads; transfers continue\"]');if(!b)return false;b.click();return true})()", false).await?);
    wait(cdp, session, "(document.querySelector('select[aria-label=\"Recording to transcribe\"]')?.options.length??0)>1").await?;
    let select_upload = format!(
        "(()=>{{const e=document.querySelector('select[aria-label=\"Recording to transcribe\"]');const o=[...e.options].find(o=>o.value && !{}.includes(o.value));if(!o)return false;e.value=o.value;e.dispatchEvent(new Event('change',{{bubbles:true}}));return true}})()",
        serde_json::to_string(&previous_uploads)?
    );
    wait(cdp, session, &select_upload).await?;
    let source_uri: String = cdp
        .evaluate(
            session,
            "document.querySelector('select[aria-label=\"Recording to transcribe\"]').value",
            false,
        )
        .await?;
    let source_uri = format!(
        "artifact://{}",
        uuid::Uuid::parse_str(
            source_uri
                .rsplit('/')
                .next()
                .context("uploaded Artifact ID")?
        )?
    );
    let recording_started = Instant::now();
    click(cdp, session, "Transcribe").await?;
    click(cdp, session, "Open Activity").await?;
    wait(cdp, session, &format!("Boolean({card})")).await?;
    cdp.command("Page.reload", serde_json::json!({}), Some(session))
        .await?;
    wait_for_document(cdp, session).await?;
    hardware(cdp, session).await?;
    wait(cdp, session, &format!("(()=>{{const b=[...({card}?.querySelectorAll('button')??[])].find(b=>b.textContent==='Open transcript');if(!b)return false;b.click();return true}})()")).await?;
    wait(cdp, session, &format!("{card}?.querySelector('.speech-segments')?.textContent.toLowerCase().includes('old portrait')??false")).await?;
    let recording_elapsed = recording_started.elapsed().as_millis();
    ensure!(
        cdp.evaluate::<bool>(
            session,
            &format!(
                "(()=>{{{card}.querySelector('.speech-segments button').click();return true}})()"
            ),
            false
        )
        .await?
    );
    wait(
        cdp,
        session,
        &format!("({card}?.querySelector('.speech-result audio')?.currentTime??0)>0.1"),
    )
    .await?;
    let result: Value = cdp.evaluate(session, &format!("(()=>{{const s={card}.querySelector('.speech-result');return {{text:s.querySelector('.speech-segments').textContent,downloads:[...s.querySelectorAll('a')].map(a=>a.href),duration:s.querySelector('audio').duration}}}})()"), false).await?;
    ensure!(cdp.evaluate::<bool>(session, &format!("(async()=>{{const links=[...{card}.querySelectorAll('.speech-result>a')];if(links.length!==2)return false;const [json,vtt]=await Promise.all(links.map(a=>fetch(a.href)));if(!json.ok||!vtt.ok)return false;const doc=await json.json();return doc.schema==='veoveo.speech-transcript/v1' && doc.source_artifact_uri==={} && (await vtt.text()).startsWith('WEBVTT')}})()", serde_json::to_string(&source_uri)?), true).await?, "transcript/caption downloads did not match the new recording");
    let final_hardware = hardware(cdp, session).await?;
    let transcript_shot = capture_screenshot(cdp, session, &output.join("transcript.png")).await?;
    let evidence = serde_json::json!({"schema":"veoveo.io/speech-browser-acceptance/v1","createdAt":chrono::Utc::now(),"chatUrl":chat_url,"microphone":"synthetic fixture MediaStream through real AudioWorklet and CUDA; hardware microphone not qualified","initialHardware":initial_hardware,"finalHardware":final_hardware,"reviewedDraft":reviewed,"cancelPreservedDraft":true,"explicitSend":true,"taskObservedAfterReload":true,"sourceArtifactUri":source_uri,"recordingElapsedMillis":recording_elapsed,"downloadsVerified":true,"injectedChunkDelayMillis":2500,"result":result,"screenshots":{"review":draft_shot,"transcript":transcript_shot}});
    fs::write(
        output.join("evidence.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;
    Ok(())
}
