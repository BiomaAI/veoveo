//! Live RRD delivery for direct spool files and authenticated ingest parts.

use std::{
    collections::BTreeSet,
    fs::File,
    io::{self, BufReader, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use bytes::Bytes;
use re_build_info::CrateVersion;
use re_log_encoding::{Decoder, EncodingOptions, rrd::Encoder};
use re_log_types::LogMsg;
use tokio::{io::AsyncReadExt, sync::mpsc};
use veoveo_recording_hub::{ingest_part_sequence, ingest_segment_parts_directory};

pub type LiveRrdReceiver = mpsc::Receiver<Result<Bytes, io::Error>>;

pub fn stream_live_rrd(segment_path: PathBuf) -> LiveRrdReceiver {
    let (sender, receiver) = mpsc::channel(32);
    if segment_path.exists() {
        tokio::spawn(tail_growing_file(segment_path, sender));
    } else {
        tokio::task::spawn_blocking(move || {
            let error_sender = sender.clone();
            if let Err(error) = stream_ingest_parts(&segment_path, sender) {
                let _ = error_sender.blocking_send(Err(io::Error::other(error.to_string())));
            }
        });
    }
    receiver
}

async fn tail_growing_file(path: PathBuf, sender: mpsc::Sender<Result<Bytes, io::Error>>) {
    let mut file = match tokio::fs::File::open(&path).await {
        Ok(file) => file,
        Err(error) => {
            let _ = sender.send(Err(error)).await;
            return;
        }
    };
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        match file.read(&mut buffer).await {
            Ok(0) => tokio::time::sleep(Duration::from_millis(100)).await,
            Ok(read) => {
                if sender
                    .send(Ok(Bytes::copy_from_slice(&buffer[..read])))
                    .await
                    .is_err()
                {
                    return;
                }
            }
            Err(error) => {
                let _ = sender.send(Err(error)).await;
                return;
            }
        }
    }
}

fn stream_ingest_parts(
    segment_path: &Path,
    sender: mpsc::Sender<Result<Bytes, io::Error>>,
) -> Result<()> {
    let parts_directory = ingest_segment_parts_directory(segment_path);
    let mut encoder = Encoder::new_eager(
        CrateVersion::LOCAL,
        EncodingOptions::PROTOBUF_COMPRESSED,
        ChannelWriter(sender),
    )
    .context("opening live ingest RRD encoder")?;
    let mut streamed = BTreeSet::new();
    loop {
        let parts = ordered_parts(&parts_directory)?;
        let mut appended = false;
        for (sequence, path) in parts {
            if streamed.contains(&sequence) {
                continue;
            }
            let file = match File::open(&path) {
                Ok(file) => file,
                Err(error)
                    if error.kind() == io::ErrorKind::NotFound && !parts_directory.exists() =>
                {
                    encoder.finish()?;
                    encoder.flush_blocking()?;
                    return Ok(());
                }
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("opening live ingest part {}", path.display()));
                }
            };
            let decoder = Decoder::<LogMsg>::decode_eager(BufReader::new(file))
                .with_context(|| format!("decoding live ingest part {}", path.display()))?;
            for message in decoder {
                encoder
                    .append(&message.with_context(|| {
                        format!("decoding live ingest part {}", path.display())
                    })?)?;
            }
            encoder.flush_blocking()?;
            streamed.insert(sequence);
            appended = true;
        }
        if !parts_directory.exists() {
            encoder.finish()?;
            encoder.flush_blocking()?;
            return Ok(());
        }
        if !appended {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn ordered_parts(directory: &Path) -> Result<Vec<(u64, PathBuf)>> {
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut parts = entries
        .filter_map(|entry| match entry {
            Ok(entry) if entry.file_type().is_ok_and(|kind| kind.is_file()) => {
                ingest_part_sequence(&entry.path()).map(|sequence| Ok((sequence, entry.path())))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error.into())),
        })
        .collect::<Result<Vec<_>>>()?;
    parts.sort_by_key(|(sequence, _)| *sequence);
    Ok(parts)
}

struct ChannelWriter(mpsc::Sender<Result<Bytes, io::Error>>);

impl Write for ChannelWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0
            .blocking_send(Ok(Bytes::copy_from_slice(buffer)))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "live RRD client closed"))?;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use re_sdk::RecordingStreamBuilder;
    use re_sdk_types::archetypes::Scalars;

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn ordered_parts_stream_as_one_live_rrd() {
        let (recording, storage) = RecordingStreamBuilder::new("inspection-camera")
            .recording_id("run-a")
            .memory()
            .unwrap();
        recording
            .log("sensor/value", &Scalars::single(42.0))
            .unwrap();
        let messages = storage.take();
        let store_info = messages
            .iter()
            .find(|message| matches!(message, LogMsg::SetStoreInfo(_)))
            .unwrap();
        let data = messages
            .iter()
            .find(|message| !matches!(message, LogMsg::SetStoreInfo(_)))
            .unwrap();

        let directory = tempfile::tempdir().unwrap();
        let segment_path = directory.path().join("segment.rrd");
        let parts_directory = ingest_segment_parts_directory(&segment_path);
        std::fs::create_dir(&parts_directory).unwrap();
        for sequence in 0..2 {
            let mut encoder = Encoder::new_eager(
                CrateVersion::LOCAL,
                EncodingOptions::PROTOBUF_COMPRESSED,
                Vec::new(),
            )
            .unwrap();
            encoder.append(store_info).unwrap();
            encoder.append(data).unwrap();
            encoder.finish().unwrap();
            std::fs::write(
                parts_directory.join(format!("{sequence:020}.rrd")),
                encoder.into_inner().unwrap(),
            )
            .unwrap();
        }

        let mut receiver = stream_live_rrd(segment_path);
        let mut streamed = Vec::new();
        loop {
            match tokio::time::timeout(Duration::from_millis(250), receiver.recv()).await {
                Ok(Some(Ok(chunk))) => streamed.extend_from_slice(&chunk),
                Ok(Some(Err(error))) => panic!("live stream failed: {error}"),
                Ok(None) => panic!("live stream ended before segment rollover"),
                Err(_) => break,
            }
        }
        std::fs::remove_dir_all(parts_directory).unwrap();
        while let Some(result) = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("live stream did not end after rollover")
        {
            streamed.extend_from_slice(&result.unwrap());
        }

        let decoded = Decoder::<LogMsg>::decode_eager(BufReader::new(Cursor::new(streamed)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(decoded.len(), 4);
    }
}
