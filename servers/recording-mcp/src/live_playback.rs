//! Bounded-history live RRD delivery.
//!
//! A late viewer receives store metadata, static chunks, and temporal chunks
//! whose row IDs fall inside the configured recent-history window. The same
//! encoder then follows newly durable data. This prevents an hour-long active
//! shard from being replayed from byte zero whenever a viewer connects.

use std::{
    collections::BTreeSet,
    fs::File,
    io::{self, BufReader, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, ensure};
use bytes::Bytes;
use re_build_info::CrateVersion;
use re_chunk::Chunk;
use re_log_encoding::{Decoder, EncodingOptions, rrd::Encoder};
use re_log_types::LogMsg;
use tokio::sync::mpsc;
use veoveo_recording_hub::{
    ingest_part_sequence, ingest_segment_parts_directory, ingest_stream_static_context_path,
};

pub type LiveRrdReceiver = mpsc::Receiver<Result<Bytes, io::Error>>;

pub fn stream_live_rrd(segment_path: PathBuf, history: Duration) -> LiveRrdReceiver {
    let (sender, receiver) = mpsc::channel(32);
    tokio::task::spawn_blocking(move || {
        let error_sender = sender.clone();
        let result = if segment_path.exists() {
            stream_growing_file(&segment_path, history, sender)
        } else {
            stream_ingest_parts(&segment_path, history, sender)
        };
        if let Err(error) = result {
            let _ = error_sender.blocking_send(Err(io::Error::other(error.to_string())));
        }
    });
    receiver
}

fn stream_growing_file(
    path: &Path,
    history: Duration,
    sender: mpsc::Sender<Result<Bytes, io::Error>>,
) -> Result<()> {
    let cutoff = history_cutoff(history)?;
    let reader = FollowingFile::open(path, sender.clone())?;
    let decoder = Decoder::<LogMsg>::decode_eager(BufReader::new(reader))
        .with_context(|| format!("opening live RRD {}", path.display()))?;
    let mut encoder = live_encoder(sender)?;
    for message in decoder {
        let message = message.with_context(|| format!("decoding live RRD {}", path.display()))?;
        if message_is_in_live_window(&message, cutoff)? {
            encoder.append(&message)?;
            encoder.flush_blocking()?;
        }
    }
    encoder.finish()?;
    encoder.flush_blocking()?;
    Ok(())
}

fn stream_ingest_parts(
    segment_path: &Path,
    history: Duration,
    sender: mpsc::Sender<Result<Bytes, io::Error>>,
) -> Result<()> {
    let parts_directory = ingest_segment_parts_directory(segment_path);
    let cutoff = history_cutoff(history)?;
    let modified_cutoff = SystemTime::now()
        .checked_sub(history)
        .context("live history exceeds system clock")?;
    let mut encoder = live_encoder(sender)?;
    let static_context = ingest_stream_static_context_path(segment_path)?;
    if static_context.exists() {
        let file = File::open(&static_context)
            .with_context(|| format!("opening live static context {}", static_context.display()))?;
        let decoder = Decoder::<LogMsg>::decode_eager(BufReader::new(file)).with_context(|| {
            format!("decoding live static context {}", static_context.display())
        })?;
        for message in decoder {
            encoder.append(&message.with_context(|| {
                format!("decoding live static context {}", static_context.display())
            })?)?;
        }
        encoder.flush_blocking()?;
    }
    let mut streamed = BTreeSet::new();
    let mut initial_snapshot = true;
    loop {
        let parts = ordered_parts(&parts_directory)?;
        let latest_sequence = parts.last().map(|part| part.sequence);
        let mut appended = false;
        for part in parts {
            if streamed.contains(&part.sequence) {
                continue;
            }
            if initial_snapshot
                && part.modified < modified_cutoff
                && Some(part.sequence) != latest_sequence
            {
                streamed.insert(part.sequence);
                continue;
            }
            let file = match File::open(&part.path) {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("opening live ingest part {}", part.path.display())
                    });
                }
            };
            let decoder = Decoder::<LogMsg>::decode_eager(BufReader::new(file))
                .with_context(|| format!("decoding live ingest part {}", part.path.display()))?;
            for message in decoder {
                let message = message.with_context(|| {
                    format!("decoding live ingest part {}", part.path.display())
                })?;
                if message_is_in_live_window(&message, cutoff)? {
                    encoder.append(&message)?;
                }
            }
            encoder.flush_blocking()?;
            streamed.insert(part.sequence);
            appended = true;
        }
        initial_snapshot = false;
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

fn live_encoder(sender: mpsc::Sender<Result<Bytes, io::Error>>) -> Result<Encoder<ChannelWriter>> {
    Encoder::new_eager(
        CrateVersion::LOCAL,
        EncodingOptions::PROTOBUF_COMPRESSED,
        ChannelWriter(sender),
    )
    .context("opening bounded live RRD encoder")
}

fn history_cutoff(history: Duration) -> Result<u64> {
    ensure!(!history.is_zero(), "live history must be positive");
    let history_nanos = history.as_nanos();
    let now_nanos = chrono::Utc::now()
        .timestamp_nanos_opt()
        .context("current time exceeds nanosecond range")?;
    Ok(u64::try_from(now_nanos)?
        .saturating_sub(u64::try_from(history_nanos).context("live history exceeds u64 nanos")?))
}

fn message_is_in_live_window(message: &LogMsg, cutoff_nanos: u64) -> Result<bool> {
    let LogMsg::ArrowMsg(_, arrow) = message else {
        return Ok(true);
    };
    let chunk = Chunk::from_arrow_msg(arrow).context("decoding live Rerun chunk")?;
    Ok(chunk.is_static()
        || chunk
            .row_ids()
            .any(|row_id| row_id.nanos_since_epoch() >= cutoff_nanos))
}

struct FollowingFile {
    file: File,
    path: PathBuf,
    sender: mpsc::Sender<Result<Bytes, io::Error>>,
}

impl FollowingFile {
    fn open(path: &Path, sender: mpsc::Sender<Result<Bytes, io::Error>>) -> io::Result<Self> {
        Ok(Self {
            file: File::open(path)?,
            path: path.to_owned(),
            sender,
        })
    }

    fn path_was_replaced(&self) -> io::Result<bool> {
        let open = self.file.metadata()?;
        let current = match std::fs::metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(true),
            Err(error) => return Err(error),
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            Ok(open.dev() != current.dev() || open.ino() != current.ino())
        }
        #[cfg(not(unix))]
        {
            Ok(open.len() != current.len() || open.modified()? != current.modified()?)
        }
    }
}

impl Read for FollowingFile {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            let read = self.file.read(buffer)?;
            if read > 0 || self.sender.is_closed() || self.path_was_replaced()? {
                return Ok(read);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[derive(Debug)]
struct LivePart {
    sequence: u64,
    path: PathBuf,
    modified: SystemTime,
}

fn ordered_parts(directory: &Path) -> Result<Vec<LivePart>> {
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut parts = Vec::new();
    for entry in entries {
        let path = entry?.path();
        let Some(sequence) = ingest_part_sequence(&path) else {
            continue;
        };
        let modified = match std::fs::metadata(&path).and_then(|metadata| metadata.modified()) {
            Ok(modified) => modified,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        parts.push(LivePart {
            sequence,
            path,
            modified,
        });
    }
    parts.sort_by_key(|part| part.sequence);
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
        let segment_path = directory
            .path()
            .join(format!("recording.ingest-{}-s0.rrd", uuid::Uuid::now_v7()));
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

        let mut receiver = stream_live_rrd(segment_path, Duration::from_secs(60));
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

    #[cfg(unix)]
    #[test]
    fn growing_file_reaches_eof_when_archive_replaces_it() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("active.rrd");
        std::fs::write(&path, b"live").unwrap();
        let (sender, _receiver) = mpsc::channel(1);
        let mut file = FollowingFile::open(&path, sender).unwrap();
        let mut bytes = [0_u8; 4];
        file.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"live");

        let archive = directory.path().join("archive.rrd");
        std::fs::write(&archive, b"sealed").unwrap();
        std::fs::rename(archive, &path).unwrap();

        assert_eq!(file.read(&mut bytes).unwrap(), 0);
    }

    #[test]
    fn old_temporal_chunks_are_excluded_but_static_data_is_retained() {
        let (recording, storage) = RecordingStreamBuilder::new("inspection-camera")
            .recording_id("run-a")
            .memory()
            .unwrap();
        recording
            .log_static("sensor/calibration", &Scalars::single(1.0))
            .unwrap();
        recording
            .log("sensor/value", &Scalars::single(42.0))
            .unwrap();
        let messages = storage.take();
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, LogMsg::ArrowMsg(_, arrow) if Chunk::from_arrow_msg(arrow).unwrap().is_static()))
        );
        let future_cutoff = u64::MAX;
        let selected = messages
            .iter()
            .filter(|message| message_is_in_live_window(message, future_cutoff).unwrap())
            .collect::<Vec<_>>();
        assert!(
            selected
                .iter()
                .any(|message| matches!(message, LogMsg::SetStoreInfo(_)))
        );
        assert!(
            selected
                .iter()
                .any(|message| matches!(message, LogMsg::ArrowMsg(_, arrow) if Chunk::from_arrow_msg(arrow).unwrap().is_static()))
        );
        assert!(
            !selected
                .iter()
                .any(|message| matches!(message, LogMsg::ArrowMsg(_, arrow) if !Chunk::from_arrow_msg(arrow).unwrap().is_static()))
        );
    }
}
