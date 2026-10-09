//! Output from one fresh stock SSH shell; no retained replay protocol.
use crate::{MAX_CHUNK_BYTES, Result, RuntimeFailure};
use russh::ChannelMsg;
use std::time::Duration;
use tokio::sync::mpsc;

/// Private adapter bytes. Never derive field-by-field Debug for process output.
#[derive(PartialEq, Eq)]
pub enum TerminalOutput {
    Data(Vec<u8>),
}

fn bytes(message: &ChannelMsg) -> Result<Option<&[u8]>> {
    let data = match message {
        ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, ext: 1 } => data,
        ChannelMsg::ExtendedData { .. } => return Err(RuntimeFailure::TerminalFailed),
        _ => return Ok(None),
    };
    if data.len() > MAX_CHUNK_BYTES {
        return Err(RuntimeFailure::TerminalBounds);
    }
    Ok(Some(data))
}

pub(crate) async fn pump_output(
    reader: &mut russh::ChannelReadHalf,
    output: &mpsc::Sender<Result<TerminalOutput>>,
) -> Result<()> {
    let mut pending = None;
    loop {
        let message = match pending.take() {
            Some(message) => Some(message),
            None => reader.wait().await,
        };
        let Some(message) = message else {
            return Ok(());
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
                            // Preserve channel control messages after the current byte batch.
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
            // Stock SSH sends exit status after EOF. Keep reading controls until
            // Close so a failed shell cannot settle as successful completion.
            ChannelMsg::Eof => {}
            ChannelMsg::Close => return Ok(()),
            ChannelMsg::ExitStatus { exit_status } if exit_status != 0 => {
                return Err(RuntimeFailure::TerminalFailed);
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
    fn stock_output_admits_only_bounded_stdout_and_stderr() {
        assert_eq!(
            bytes(&ChannelMsg::Data {
                data: vec![0, 255].into()
            })
            .unwrap(),
            Some(&[0, 255][..])
        );
        assert_eq!(
            bytes(&ChannelMsg::ExtendedData {
                ext: 1,
                data: vec![27].into()
            })
            .unwrap(),
            Some(&[27][..])
        );
        assert!(
            bytes(&ChannelMsg::ExtendedData {
                ext: 0xFE00_0001,
                data: vec![10, 0].into()
            })
            .is_err()
        );
        assert!(
            bytes(&ChannelMsg::Data {
                data: vec![0; MAX_CHUNK_BYTES + 1].into()
            })
            .is_err()
        );
    }
}
