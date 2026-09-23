//! Real worker lifecycle, CUDA inference and bounded live audio. No mocked inference.
mod support;

use anyhow::{Context, Result, bail, ensure};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::{
    process::Command,
    time::{sleep, timeout},
};
use veoveo_speech_contract::transcript::Transcript;
use veoveo_speech_mcp::worker::{
    PROTOCOL, WorkerConnection, WorkerError, WorkerEvent, WorkerRequest,
};

async fn completed(connection: &mut WorkerConnection) -> Result<Transcript> {
    timeout(Duration::from_secs(30), async {
        loop {
            match connection.event().await? {
                WorkerEvent::Accepted => (),
                WorkerEvent::Transcript {
                    complete,
                    transcript,
                } => {
                    transcript.validate(120)?;
                    if complete {
                        return Ok(transcript);
                    }
                }
                event => bail!("unexpected worker event: {event:?}"),
            }
        }
    })
    .await?
}

async fn transcribe(socket: &Path, path: PathBuf) -> Result<(Transcript, f64)> {
    let started = Instant::now();
    let mut connection = WorkerConnection::connect(
        socket,
        &WorkerRequest::File {
            path,
            max_duration_seconds: 120,
        },
    )
    .await?;
    Ok((
        completed(&mut connection).await?,
        started.elapsed().as_secs_f64(),
    ))
}

fn pcm_wav(bytes: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        bytes.get(..4) == Some(b"RIFF") && bytes.get(8..12) == Some(b"WAVE"),
        "invalid fixture WAV"
    );
    let mut offset = 12;
    let mut format = false;
    while offset + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into()?) as usize;
        let payload = bytes
            .get(offset + 8..offset + 8 + size)
            .context("truncated WAV")?;
        match &bytes[offset..offset + 4] {
            b"fmt " => {
                ensure!(payload.len() >= 16, "missing WAV format");
                ensure!(
                    payload[0..4] == [1, 0, 1, 0] && payload[14..16] == [16, 0],
                    "fixture must be mono PCM16"
                );
                ensure!(
                    u32::from_le_bytes(payload[4..8].try_into()?) == 16_000,
                    "fixture must be 16 kHz"
                );
                format = true;
            }
            b"data" => {
                ensure!(format && size.is_multiple_of(2), "invalid PCM fixture");
                return Ok(payload
                    .chunks_exact(2)
                    .flat_map(|sample| {
                        (f32::from(i16::from_le_bytes([sample[0], sample[1]])) / 32768.0)
                            .to_le_bytes()
                    })
                    .collect());
            }
            _ => (),
        }
        offset += 8 + size + size % 2;
    }
    bail!("WAV data missing")
}

#[tokio::test]
#[ignore = "requires NVIDIA CUDA, the locked runner environment and cached pinned model"]
async fn cuda_file_live_cancel_and_input_bounds() -> Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let python = root.join("runner/.venv/bin/python");
    ensure!(
        python.is_file(),
        "run uv sync --project servers/speech-mcp/runner --locked first"
    );
    let work = tempfile::tempdir()?;
    let socket = work.path().join("worker.sock");
    let denied = timeout(
        Duration::from_secs(15),
        Command::new(&python)
            .arg("-m")
            .arg("speech_runner.main")
            .arg("--socket")
            .arg(&socket)
            .arg("--work-dir")
            .arg(work.path())
            .env("CUDA_VISIBLE_DEVICES", "")
            .env_remove("MOONDREAM_API_KEY")
            .kill_on_drop(true)
            .output(),
    )
    .await??;
    ensure!(
        !denied.status.success() && !socket.exists(),
        "worker admitted a CPU fallback"
    );
    ensure!(
        String::from_utf8_lossy(&denied.stderr).contains("qualified CUDA runtime"),
        "missing GPU failed for an unrelated reason"
    );
    let english = work.path().join("english.wav");
    let spanish = work.path().join("spanish.wav");
    std::fs::copy(root.join("testdata/english.wav"), &english)?;
    std::fs::copy(root.join("testdata/spanish.wav"), &spanish)?;
    let log = work.path().join("worker.log");
    let stderr = std::fs::File::create(&log)?;
    let started = Instant::now();
    let mut child = Command::new(&python)
        .arg("-m")
        .arg("speech_runner.main")
        .arg("--socket")
        .arg(&socket)
        .arg("--work-dir")
        .arg(work.path())
        .arg("--capacity")
        .arg("1")
        .env("HF_HUB_OFFLINE", "1")
        .env_remove("MOONDREAM_API_KEY")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr)
        .kill_on_drop(true)
        .spawn()?;
    timeout(Duration::from_secs(90), async {
        while !socket.exists() {
            if let Some(status) = child.try_wait()? {
                bail!("worker exited {status}: {}", std::fs::read_to_string(&log)?);
            }
            sleep(Duration::from_millis(100)).await;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    let load_seconds = started.elapsed().as_secs_f64();
    let mut probe = WorkerConnection::connect(&socket, &WorkerRequest::Probe).await?;
    let readiness = probe.event().await?;
    match &readiness {
        WorkerEvent::Ready {
            protocol,
            device,
            model,
            ..
        } => {
            ensure!(
                protocol == PROTOCOL && device.starts_with("cuda:NVIDIA"),
                "hardware readiness missing"
            );
            ensure!(model == "moondream/parakeet-ultra", "wrong model");
        }
        _ => bail!("worker not ready"),
    }
    let (en, english_seconds) = transcribe(&socket, english.clone()).await?;
    ensure!(
        en.text.to_lowercase().contains("old portrait"),
        "English fixture was not transcribed"
    );
    ensure!(
        en.segments.iter().any(|segment| !segment.words.is_empty()),
        "word timestamps missing"
    );
    let (es, spanish_seconds) = transcribe(&socket, spanish).await?;
    ensure!(
        es.text.to_lowercase().contains("clima extremo")
            && es.text.to_lowercase().contains("planes de viaje"),
        "Spanish fixture was not transcribed"
    );

    let mut outside = WorkerConnection::connect(
        &socket,
        &WorkerRequest::File {
            path: root.join("testdata/english.wav"),
            max_duration_seconds: 120,
        },
    )
    .await?;
    ensure!(
        matches!(
            outside.event().await?,
            WorkerEvent::Error {
                code: WorkerError::InvalidInput
            }
        ),
        "workspace boundary missing"
    );

    let live_request = WorkerRequest::Live {
        sample_rate: 16_000,
        max_duration_seconds: 120,
    };
    let mut live = WorkerConnection::connect(&socket, &live_request).await?;
    ensure!(
        matches!(live.event().await?, WorkerEvent::Accepted),
        "live admission failed"
    );
    let mut busy = WorkerConnection::connect(&socket, &live_request).await?;
    ensure!(
        matches!(
            busy.event().await?,
            WorkerEvent::Error {
                code: WorkerError::Capacity
            }
        ),
        "capacity was not enforced"
    );
    let pcm = pcm_wav(&std::fs::read(&english)?)?;
    let (mut events, mut writer) = live.split();
    let live_started = Instant::now();
    let send = async {
        for chunk in pcm.chunks(16_000) {
            writer.pcm(chunk).await?;
            sleep(Duration::from_millis(250)).await;
        }
        writer.pcm(&[]).await?;
        Ok::<_, anyhow::Error>(writer)
    };
    let observe = async {
        let mut first_update = None;
        loop {
            match events.event().await? {
                WorkerEvent::Transcript {
                    complete,
                    transcript,
                } => {
                    transcript.validate(120)?;
                    first_update.get_or_insert(live_started.elapsed().as_secs_f64());
                    if complete {
                        return Ok::<_, anyhow::Error>((first_update, transcript));
                    }
                }
                event => bail!("live input failed: {event:?}"),
            }
        }
    };
    let (_writer, (first_update_seconds, live_transcript)) =
        timeout(Duration::from_secs(30), async {
            tokio::try_join!(send, observe)
        })
        .await??;
    ensure!(
        live_transcript.text.to_lowercase().contains("old portrait"),
        "live final transcript missing"
    );

    let mut cancelled = WorkerConnection::connect(&socket, &live_request).await?;
    ensure!(
        matches!(cancelled.event().await?, WorkerEvent::Accepted),
        "cancellation fixture admission failed"
    );
    cancelled.pcm(&pcm[..16_000]).await?;
    drop(cancelled);
    // Probe admission with no audio; a successful admission proves released capacity.
    timeout(Duration::from_secs(5), async {
        loop {
            let mut next = WorkerConnection::connect(&socket, &live_request).await?;
            match next.event().await? {
                WorkerEvent::Accepted => return Ok::<_, anyhow::Error>(()),
                WorkerEvent::Error {
                    code: WorkerError::Capacity,
                } => sleep(Duration::from_millis(50)).await,
                event => bail!("unexpected post-cancel state: {event:?}"),
            }
        }
    })
    .await??;
    let evidence = serde_json::json!({"schema":"veoveo.speech-gpu-acceptance/v1", "readiness":readiness,
        "load_seconds":load_seconds, "english_seconds":english_seconds, "spanish_seconds":spanish_seconds,
        "live_first_update_seconds":first_update_seconds, "english":en, "spanish":es,
        "live":live_transcript, "cancellation_released_capacity":true, "path_boundary":true});
    let output = root.join("../../output/development/speech/native-gpu.json");
    std::fs::create_dir_all(output.parent().context("evidence parent")?)?;
    std::fs::write(output, serde_json::to_vec_pretty(&evidence)?)?;
    child.kill().await?;
    child.wait().await?;
    private_sessions(&python, &pcm).await?;
    Ok(())
}

async fn private_sessions(python: &std::path::Path, pcm: &[u8]) -> Result<()> {
    use veoveo_mcp_contract::{PrincipalKind, WorkContextId};
    use veoveo_speech_contract::dictation::{DictationStatus, StartDictation};
    use veoveo_speech_mcp::{dictation::Dictations, process::WorkerProcess};
    let worker = std::sync::Arc::new(WorkerProcess::start(python, 2).await?);
    let sessions = Dictations::new(worker, 1);
    let alice = support::identity(&support::owner("alice"));
    let bob = support::identity(&support::owner("bob"));
    let id = uuid::Uuid::now_v7();
    let start = StartDictation {
        id,
        sample_rate: 16_000,
    };
    sessions.start(alice.clone(), start.clone()).await?;
    ensure!(
        sessions.start(alice.clone(), start).await?.id == id,
        "start replay changed session"
    );
    ensure!(
        sessions.read(&bob, id).await.is_err(),
        "private draft leaked"
    );
    let mut other_context = alice.clone();
    other_context.authority.work_context = WorkContextId::new("other-work-context")?;
    ensure!(
        sessions.read(&other_context, id).await.is_err(),
        "Work Context boundary missing"
    );
    let mut other_session = alice.clone();
    other_session
        .request_context
        .as_mut()
        .unwrap()
        .access_token
        .session_family = bob
        .request_context
        .as_ref()
        .unwrap()
        .access_token
        .session_family
        .clone();
    ensure!(
        sessions.read(&other_session, id).await.is_err(),
        "browser session boundary missing"
    );
    let mut automated = alice.clone();
    automated.actor.kind = PrincipalKind::Service;
    ensure!(
        sessions.read(&automated, id).await.is_err(),
        "service could access microphone draft"
    );
    for (sequence, chunk) in pcm.chunks(16_000).enumerate() {
        let receipt = sessions
            .chunk(&alice, id, sequence as u32, chunk.to_vec())
            .await?;
        ensure!(
            receipt.next_sequence == sequence as u32 + 1,
            "audio sequence not acknowledged"
        );
        if sequence == 0 {
            let retry = sessions.chunk(&alice, id, 0, chunk.to_vec()).await?;
            ensure!(retry.next_sequence == 1, "duplicate audio appended");
        }
        sleep(Duration::from_millis(250)).await;
    }
    let result = sessions.finish(&alice, id, false).await?;
    ensure!(
        result.status == DictationStatus::Completed
            && result
                .transcript
                .as_ref()
                .is_some_and(|t| t.text.to_lowercase().contains("old portrait")),
        "private dictation final missing"
    );
    let repeated = sessions.finish(&alice, id, false).await?;
    ensure!(
        repeated.transcript.unwrap().text == result.transcript.unwrap().text,
        "final replay changed text"
    );
    let cancelled_id = uuid::Uuid::now_v7();
    sessions
        .start(
            alice.clone(),
            StartDictation {
                id: cancelled_id,
                sample_rate: 16_000,
            },
        )
        .await?;
    sessions
        .chunk(&alice, cancelled_id, 0, pcm[..16_000].to_vec())
        .await?;
    let cancelled = sessions.finish(&alice, cancelled_id, true).await?;
    ensure!(
        cancelled.status == DictationStatus::Cancelled && cancelled.transcript.is_none(),
        "cancel retained private text"
    );
    Ok(())
}
