use super::Command;
use crate::worker::{WorkerConnection, WorkerEvent};
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::{
    sync::{OwnedSemaphorePermit, mpsc, watch},
    time::Instant,
};
use veoveo_speech_contract::{
    dictation::{DictationSnapshot, DictationStatus, MAX_CHUNK_BYTES},
    transcript::MAX_DICTATION_SECONDS,
};

pub(super) async fn run(
    connection: WorkerConnection,
    commands: mpsc::Receiver<Command>,
    updates: watch::Sender<DictationSnapshot>,
    sample_rate: u32,
    _slot: OwnedSemaphorePermit,
) {
    if process(connection, commands, &updates, sample_rate)
        .await
        .is_err()
    {
        updates.send_modify(|state| state.status = DictationStatus::Failed);
    }
}

async fn process(
    connection: WorkerConnection,
    mut commands: mpsc::Receiver<Command>,
    updates: &watch::Sender<DictationSnapshot>,
    sample_rate: u32,
) -> Result<()> {
    let (mut events, mut audio) = connection.split();
    let deadline = Instant::now() + Duration::from_secs(u64::from(MAX_DICTATION_SECONDS) + 15);
    let mut idle = Instant::now() + Duration::from_secs(10);
    let mut total = 0u64;
    let mut last: Option<(u32, [u8; 32])> = None;
    let mut flushing = false;
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline.min(idle)) => anyhow::bail!("dictation timed out"),
            event = events.event() => match event? {
                WorkerEvent::Transcript { complete, transcript } => {
                    transcript.validate(MAX_DICTATION_SECONDS)?;
                    updates.send_modify(|state| {
                        state.transcript = Some(transcript);
                        if complete { state.status = DictationStatus::Completed; }
                    });
                    if complete { return Ok(()); }
                }
                _ => anyhow::bail!("dictation inference failed"),
            },
            command = commands.recv() => match command {
                None => return Ok(()),
                Some(Command::Cancel { reply }) => {
                    // Close the inference connection before acknowledging cancellation.
                    drop(audio); drop(events);
                    updates.send_modify(|state| { state.status = DictationStatus::Cancelled; state.transcript = None; });
                    let _ = reply.send(Ok(()));
                    return Ok(());
                }
                Some(Command::Finish { reply }) => {
                    if !flushing {
                        tokio::time::timeout(Duration::from_secs(5), audio.pcm(&[])).await??;
                        flushing = true;
                        idle = Instant::now() + Duration::from_secs(15);
                        updates.send_modify(|state| state.status = DictationStatus::Flushing);
                    }
                    let _ = reply.send(Ok(()));
                }
                Some(Command::Chunk { sequence, bytes, reply }) => {
                    let hash: [u8; 32] = Sha256::digest(&bytes).into();
                    if last == Some((sequence, hash)) {
                        let _ = reply.send(Ok(()));
                        continue;
                    }
                    let next = updates.borrow().next_sequence;
                    let validation = validate_frame(&bytes, sequence, next, flushing, total, sample_rate);
                    if let Err(error) = validation {
                        let _ = reply.send(Err(error));
                        anyhow::bail!("invalid audio frame");
                    }
                    tokio::time::timeout(Duration::from_secs(5), audio.pcm(&bytes)).await??;
                    total += bytes.len() as u64 / 4;
                    last = Some((sequence, hash));
                    updates.send_modify(|state| state.next_sequence += 1);
                    idle = Instant::now() + Duration::from_secs(10);
                    let _ = reply.send(Ok(()));
                }
            }
        }
    }
}

fn validate_frame(
    bytes: &[u8],
    sequence: u32,
    next: u32,
    flushing: bool,
    total: u64,
    rate: u32,
) -> Result<()> {
    ensure!(
        !flushing
            && sequence == next
            && !bytes.is_empty()
            && bytes.len() <= MAX_CHUNK_BYTES
            && bytes.len().is_multiple_of(4),
        "invalid audio sequence or size"
    );
    ensure!(
        total + bytes.len() as u64 / 4 <= u64::from(rate) * u64::from(MAX_DICTATION_SECONDS),
        "dictation duration exceeded"
    );
    ensure!(
        bytes.chunks_exact(4).all(|chunk| {
            let sample = f32::from_le_bytes(chunk.try_into().expect("four byte chunk"));
            sample.is_finite() && (-1.0..=1.0).contains(&sample)
        }),
        "invalid PCM sample"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audio_admission_bounds_memory_time_and_sequence() {
        let bytes = 0.5f32.to_le_bytes();
        assert!(validate_frame(&bytes, 0, 0, false, 0, 16000).is_ok());
        assert!(validate_frame(&bytes, 2, 0, false, 0, 16000).is_err());
        assert!(validate_frame(&bytes, 0, 0, true, 0, 16000).is_err());
        assert!(validate_frame(&bytes, 0, 0, false, 16000 * 120, 16000).is_err());
        assert!(validate_frame(&f32::NAN.to_le_bytes(), 0, 0, false, 0, 16000).is_err());
    }
}
