//! Retained bytes and the provider's ordered, generated replay boundary.
use crate::{MAX_CHUNK_BYTES, Result, RuntimeFailure, protocol::terminal::v1 as api};
use prost::Message;
use russh::ChannelMsg;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

pub(crate) const MAIN_EVENT_EXTENDED_DATA: u32 = 0xFE00_0001;
const MAX_EVENT_BYTES: usize = 32;
const REPLAY_DEADLINE: Duration = Duration::from_secs(20);

/// Private adapter events. Bytes must never acquire a field-by-field Debug impl.
#[derive(PartialEq, Eq)]
pub enum TerminalOutput {
    Data(Vec<u8>),
    ReplayComplete,
}

fn replay_complete(data: &[u8]) -> Result<()> {
    if data.is_empty() || data.len() > MAX_EVENT_BYTES {
        return Err(RuntimeFailure::TerminalBounds);
    }
    let event = api::MainProcessEvent::decode(data).map_err(|_| RuntimeFailure::TerminalFailed)?;
    // This exact provider uses prost's canonical encoding. Reject unknown,
    // duplicated or otherwise noncanonical fields instead of silently dropping them.
    if !matches!(
        event.event,
        Some(api::main_process_event::Event::ReplayComplete(_))
    ) || event.encode_to_vec() != data
    {
        return Err(RuntimeFailure::TerminalFailed);
    }
    Ok(())
}

fn bytes(message: &ChannelMsg) -> Result<Option<&[u8]>> {
    let data = match message {
        ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, ext: 1 } => data,
        ChannelMsg::ExtendedData {
            ext: MAIN_EVENT_EXTENDED_DATA,
            ..
        } => return Ok(None),
        ChannelMsg::ExtendedData { .. } => return Err(RuntimeFailure::TerminalFailed),
        _ => return Ok(None),
    };
    if data.len() > MAX_CHUNK_BYTES {
        return Err(RuntimeFailure::TerminalBounds);
    }
    Ok(Some(data))
}

pub(crate) async fn pump_output(
    reader: russh::ChannelReadHalf,
    output: &mpsc::Sender<Result<TerminalOutput>>,
) -> Result<()> {
    let (completed, received) = oneshot::channel();
    let work = pump(reader, output, completed);
    tokio::pin!(work);
    tokio::select! {
        result = &mut work => result,
        boundary = tokio::time::timeout(REPLAY_DEADLINE, received) => {
            boundary.map_err(|_| RuntimeFailure::TerminalFailed)?
                .map_err(|_| RuntimeFailure::TerminalFailed)?;
            work.await
        }
    }
}

async fn pump(
    mut reader: russh::ChannelReadHalf,
    output: &mpsc::Sender<Result<TerminalOutput>>,
    completed: oneshot::Sender<()>,
) -> Result<()> {
    let mut completed = Some(completed);
    let mut pending = None;
    loop {
        let message = match pending.take() {
            Some(message) => Some(message),
            None => reader.wait().await,
        };
        let Some(message) = message else {
            return if completed.is_none() {
                Ok(())
            } else {
                Err(RuntimeFailure::TerminalFailed)
            };
        };
        if let Some(data) = bytes(&message)? {
            if data.is_empty() {
                continue;
            }
            let mut batch = data.to_vec();
            let deadline = tokio::time::Instant::now() + Duration::from_millis(1);
            loop {
                if batch.len() == MAX_CHUNK_BYTES || tokio::time::Instant::now() >= deadline {
                    break;
                }
                match tokio::time::timeout_at(deadline, reader.wait()).await {
                    Err(_) => break,
                    Ok(None) => {
                        pending = Some(ChannelMsg::Eof);
                        break;
                    }
                    Ok(Some(message)) => {
                        if let Some(data) = bytes(&message)?
                            && batch.len() + data.len() <= MAX_CHUNK_BYTES
                        {
                            batch.extend_from_slice(data);
                        } else {
                            // In particular, never batch history with post-fence bytes.
                            pending = Some(message);
                            break;
                        }
                    }
                }
            }
            output
                .send(Ok(TerminalOutput::Data(batch)))
                .await
                .map_err(|_| RuntimeFailure::TerminalFailed)?;
            continue;
        }
        match message {
            ChannelMsg::ExtendedData {
                data,
                ext: MAIN_EVENT_EXTENDED_DATA,
            } => {
                replay_complete(&data)?;
                let completed = completed.take().ok_or(RuntimeFailure::TerminalFailed)?;
                output
                    .send(Ok(TerminalOutput::ReplayComplete))
                    .await
                    .map_err(|_| RuntimeFailure::TerminalFailed)?;
                let _ = completed.send(());
            }
            ChannelMsg::Close | ChannelMsg::Eof => {
                return if completed.is_none() {
                    Ok(())
                } else {
                    Err(RuntimeFailure::TerminalFailed)
                };
            }
            ChannelMsg::Failure | ChannelMsg::ExitSignal { .. } => {
                return Err(RuntimeFailure::TerminalFailed);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_exact_generated_bounded_event_is_accepted() {
        for next_sequence in [0, 1, u64::MAX] {
            let event = api::MainProcessEvent {
                event: Some(api::main_process_event::Event::ReplayComplete(
                    api::MainProcessReplayComplete { next_sequence },
                )),
            }
            .encode_to_vec();
            assert!(replay_complete(&event).is_ok());
            let mut unknown = event.clone();
            unknown.extend([16, 0]);
            assert!(replay_complete(&unknown).is_err());
            assert!(replay_complete(&event.repeat(2)).is_err());
        }
        for bytes in [vec![], vec![0], vec![10, 255], vec![0; MAX_EVENT_BYTES + 1]] {
            assert!(replay_complete(&bytes).is_err());
        }
    }
}
