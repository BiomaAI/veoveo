//! Bounded-history live Rerun message delivery.
//!
//! A late viewer receives store metadata, static chunks, and temporal chunks
//! whose row IDs fall inside the configured recent-history window. The same
//! message stream then follows newly durable data. This prevents an hour-long
//! active shard from being replayed from byte zero whenever a viewer connects.

use std::{
    fs::File,
    io::{self, BufReader, Read},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result, ensure};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use re_chunk::Chunk;
use re_log_encoding::Decoder;
use re_log_types::{LogMsg, StoreId};
use tokio::sync::mpsc;
use veoveo_recording_hub::{
    LiveRrdBatchKind, ingest_part_sequence, ingest_segment_parts_directory,
    ingest_stream_static_context_path,
};

pub type LiveMessageReceiver = mpsc::Receiver<Result<LogMsg, io::Error>>;

pub fn stream_live_messages(
    segment_path: PathBuf,
    history: Duration,
    playback_store_id: StoreId,
) -> LiveMessageReceiver {
    let (sender, receiver) = mpsc::channel(32);
    tokio::task::spawn_blocking(move || {
        let error_sender = sender.clone();
        let result = (|| {
            wait_for_live_source(&segment_path, &sender)?;
            if segment_path.exists() {
                stream_growing_file(&segment_path, history, &playback_store_id, &sender)
            } else {
                stream_ingest_parts(&segment_path, history, &playback_store_id, &sender)
            }
        })();
        if let Err(error) = result {
            let _ = error_sender.blocking_send(Err(io::Error::other(error.to_string())));
        }
    });
    receiver
}

fn wait_for_live_source(
    segment_path: &Path,
    sender: &mpsc::Sender<Result<LogMsg, io::Error>>,
) -> Result<()> {
    let parts_directory = ingest_segment_parts_directory(segment_path);
    if segment_path.exists() || parts_directory.exists() {
        return Ok(());
    }
    let parent = segment_path
        .parent()
        .context("live segment path has no parent")?;
    let wake = FilesystemWake::watch(parent)?;
    while !segment_path.exists() && !parts_directory.exists() {
        if sender.is_closed() {
            return Ok(());
        }
        wake.wait()?;
    }
    Ok(())
}

fn stream_growing_file(
    path: &Path,
    history: Duration,
    playback_store_id: &StoreId,
    sender: &mpsc::Sender<Result<LogMsg, io::Error>>,
) -> Result<()> {
    let cutoff = history_cutoff(history)?;
    let mut message_state = LiveMessageState::default();
    let reader = FollowingFile::open(path, sender.clone())?;
    let decoder = Decoder::<LogMsg>::decode_eager(BufReader::new(reader))
        .with_context(|| format!("opening live RRD {}", path.display()))?;
    for message in decoder {
        let mut message =
            message.with_context(|| format!("decoding live RRD {}", path.display()))?;
        message.set_store_id(playback_store_id.clone());
        if message_is_in_live_window(&message, cutoff)? {
            append_messages(std::iter::once(message), &mut message_state, sender)?;
        }
    }
    Ok(())
}

fn stream_ingest_parts(
    segment_path: &Path,
    history: Duration,
    playback_store_id: &StoreId,
    sender: &mpsc::Sender<Result<LogMsg, io::Error>>,
) -> Result<()> {
    let parts_directory = ingest_segment_parts_directory(segment_path);
    let cutoff = history_cutoff(history)?;
    let modified_cutoff = SystemTime::now()
        .checked_sub(history)
        .context("live history exceeds system clock")?;
    if !parts_directory.exists() {
        return Ok(());
    }
    let wake = FilesystemWake::watch(&parts_directory)?;
    let mut message_state = LiveMessageState::default();
    let mut bootstrap_messages = Vec::new();
    let static_context = ingest_stream_static_context_path(segment_path)?;
    if static_context.exists() {
        bootstrap_messages.extend(read_file_messages(&static_context, 0, playback_store_id)?);
    }
    let initial_parts = ordered_parts(&parts_directory)?;
    let latest_initial_sequence = initial_parts.last().map(|part| part.sequence);
    for part in initial_parts {
        if part.modified < modified_cutoff && Some(part.sequence) != latest_initial_sequence {
            continue;
        }
        match read_part_messages(&part.path, cutoff, playback_store_id)? {
            Some(messages) => bootstrap_messages.extend(messages),
            None if !parts_directory.exists() => return Ok(()),
            None => {}
        }
    }
    if !bootstrap_messages.is_empty() {
        let optimized = veoveo_recording_hub::optimize_live_rrd_messages(
            bootstrap_messages,
            LiveRrdBatchKind::Bootstrap,
        )
        .context("optimizing bounded live RRD bootstrap")?;
        append_messages(optimized, &mut message_state, sender)?;
    }
    let mut next_sequence = latest_initial_sequence.map(|sequence| sequence.saturating_add(1));
    loop {
        if next_sequence.is_none() {
            next_sequence = ordered_parts(&parts_directory)?
                .first()
                .map(|part| part.sequence);
        }
        let mut appended = false;
        while let Some(sequence) = next_sequence {
            let path = parts_directory.join(format!("{sequence:020}.rrd"));
            if !path.exists() {
                break;
            }
            if !send_part(&path, cutoff, playback_store_id, &mut message_state, sender)? {
                break;
            }
            next_sequence = Some(sequence.saturating_add(1));
            appended = true;
        }
        if !parts_directory.exists() {
            return Ok(());
        }
        if !appended {
            if sender.is_closed() {
                return Ok(());
            }
            wake.wait()?;
        }
    }
}

fn send_part(
    path: &Path,
    cutoff: u64,
    playback_store_id: &StoreId,
    message_state: &mut LiveMessageState,
    sender: &mpsc::Sender<Result<LogMsg, io::Error>>,
) -> Result<bool> {
    let Some(messages) = read_part_messages(path, cutoff, playback_store_id)? else {
        return Ok(false);
    };
    if !messages
        .iter()
        .any(|message| matches!(message, LogMsg::ArrowMsg(_, _)))
    {
        return Ok(true);
    }
    // A durable part is the forwarder's reactive boundary, not an SDK chunk-quality
    // guarantee. Rerun producers commonly contribute one-row chunks for each entity
    // and archetype. Apply Rerun's own live profile before the browser indexes the
    // newly durable batch, just as we do for the bounded bootstrap.
    let optimized =
        veoveo_recording_hub::optimize_live_rrd_messages(messages, LiveRrdBatchKind::Incremental)
            .with_context(|| format!("optimizing live RRD part {}", path.display()))?;
    append_messages(optimized, message_state, sender)?;
    Ok(true)
}

fn read_part_messages(
    path: &Path,
    cutoff: u64,
    playback_store_id: &StoreId,
) -> Result<Option<Vec<LogMsg>>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("opening live ingest part {}", path.display()));
        }
    };
    read_reader_messages(BufReader::new(file), path, cutoff, playback_store_id).map(Some)
}

fn read_file_messages(
    path: &Path,
    cutoff: u64,
    playback_store_id: &StoreId,
) -> Result<Vec<LogMsg>> {
    let file = File::open(path).with_context(|| format!("opening live RRD {}", path.display()))?;
    read_reader_messages(BufReader::new(file), path, cutoff, playback_store_id)
}

fn read_reader_messages(
    reader: impl io::BufRead,
    path: &Path,
    cutoff: u64,
    playback_store_id: &StoreId,
) -> Result<Vec<LogMsg>> {
    let decoder = Decoder::<LogMsg>::decode_eager(reader)
        .with_context(|| format!("decoding live RRD {}", path.display()))?;
    let mut messages = Vec::new();
    for message in decoder {
        let mut message =
            message.with_context(|| format!("decoding live RRD {}", path.display()))?;
        message.set_store_id(playback_store_id.clone());
        if message_is_in_live_window(&message, cutoff)? {
            messages.push(message);
        }
    }
    Ok(messages)
}

#[derive(Default)]
struct LiveMessageState {
    store_info_sent: bool,
}

fn append_messages(
    messages: impl IntoIterator<Item = LogMsg>,
    state: &mut LiveMessageState,
    sender: &mpsc::Sender<Result<LogMsg, io::Error>>,
) -> Result<()> {
    for message in messages {
        if matches!(message, LogMsg::SetStoreInfo(_)) {
            if state.store_info_sent {
                continue;
            }
            state.store_info_sent = true;
        }
        sender
            .blocking_send(Ok(message))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "live Rerun client closed"))?;
    }
    Ok(())
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
    sender: mpsc::Sender<Result<LogMsg, io::Error>>,
    wake: FilesystemWake,
}

impl FollowingFile {
    fn open(path: &Path, sender: mpsc::Sender<Result<LogMsg, io::Error>>) -> io::Result<Self> {
        let wake = FilesystemWake::watch(path).map_err(io::Error::other)?;
        Ok(Self {
            file: File::open(path)?,
            path: path.to_owned(),
            sender,
            wake,
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
            self.wake.wait().map_err(io::Error::other)?;
        }
    }
}

struct FilesystemWake {
    _watcher: RecommendedWatcher,
    receiver: std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
}

impl FilesystemWake {
    fn watch(path: &Path) -> Result<Self> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        })?;
        watcher.watch(path, RecursiveMode::NonRecursive)?;
        Ok(Self {
            _watcher: watcher,
            receiver,
        })
    }

    fn wait(&self) -> Result<()> {
        self.receiver
            .recv()
            .context("live recording filesystem event channel closed")??;
        while let Ok(event) = self.receiver.try_recv() {
            event?;
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use re_build_info::CrateVersion;
    use re_log_encoding::{EncodingOptions, rrd::Encoder};
    use re_sdk::RecordingStreamBuilder;
    use re_sdk_types::archetypes::Scalars;

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn ordered_parts_stream_as_native_rerun_messages() {
        let (recording, storage) = RecordingStreamBuilder::new("inspection-camera")
            .recording_id("run-a")
            .memory()
            .unwrap();
        recording
            .log("sensor/value", &Scalars::single(42.0))
            .unwrap();
        let first_messages = storage.take();
        let store_info = first_messages
            .iter()
            .find(|message| matches!(message, LogMsg::SetStoreInfo(_)))
            .unwrap();
        let first_data = first_messages
            .iter()
            .find(|message| !matches!(message, LogMsg::SetStoreInfo(_)))
            .unwrap();
        recording
            .log("sensor/value", &Scalars::single(43.0))
            .unwrap();
        recording.flush_blocking().unwrap();
        recording
            .log("sensor/value", &Scalars::single(44.0))
            .unwrap();
        let second_messages = storage.take();
        let second_data = second_messages
            .iter()
            .filter(|message| !matches!(message, LogMsg::SetStoreInfo(_)))
            .collect::<Vec<_>>();
        assert_eq!(second_data.len(), 2);

        let directory = tempfile::tempdir().unwrap();
        let segment_path = directory
            .path()
            .join(format!("recording.ingest-{}-s0.rrd", uuid::Uuid::now_v7()));
        let parts_directory = ingest_segment_parts_directory(&segment_path);
        std::fs::create_dir(&parts_directory).unwrap();
        write_part(&parts_directory, 0, store_info, &[first_data]);

        let playback_store_id = StoreId::recording("playback-dataset", "run-a");
        let mut receiver = stream_live_messages(
            segment_path,
            Duration::from_secs(60),
            playback_store_id.clone(),
        );
        let first_store_info = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("initial live part was not emitted")
            .expect("live stream ended before initial part")
            .expect("initial live part failed");
        let first_data = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("initial live data was not emitted")
            .expect("live stream ended before initial data")
            .expect("initial live data failed");

        write_part(&parts_directory, 1, store_info, &second_data);
        let second_data = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("reactive live part was not emitted")
            .expect("live stream ended before reactive part")
            .expect("reactive live part failed");
        assert!(
            matches!(
                &second_data,
                LogMsg::ArrowMsg(_, arrow)
                    if Chunk::from_arrow_msg(arrow).unwrap().num_rows() == 2
            ),
            "newly durable one-row chunks must be compacted before browser delivery"
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(100), receiver.recv())
                .await
                .is_err(),
            "one durable part must not repeat store metadata"
        );

        let mut streamed = vec![first_store_info, first_data, second_data];
        std::fs::remove_dir_all(parts_directory).unwrap();
        while let Some(result) = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("live stream did not end after rollover")
        {
            streamed.push(result.unwrap());
        }

        assert_eq!(
            streamed
                .iter()
                .filter(|message| matches!(message, LogMsg::SetStoreInfo(_)))
                .count(),
            1,
            "store metadata must not wake the viewer again for every part"
        );
        assert_eq!(
            streamed
                .iter()
                .filter(|message| matches!(message, LogMsg::ArrowMsg(_, _)))
                .count(),
            2
        );
        assert!(
            streamed
                .iter()
                .all(|message| message.store_id() == &playback_store_id)
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn cataloged_segment_waits_reactively_for_its_first_durable_part() {
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
            .find(|message| matches!(message, LogMsg::ArrowMsg(_, _)))
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let segment_path = directory
            .path()
            .join(format!("recording.ingest-{}-s0.rrd", uuid::Uuid::now_v7()));
        let parts_directory = ingest_segment_parts_directory(&segment_path);
        let playback_store_id = StoreId::recording("playback-dataset", "run-a");
        let mut receiver =
            stream_live_messages(segment_path, Duration::from_secs(60), playback_store_id);

        assert!(
            tokio::time::timeout(Duration::from_millis(100), receiver.recv())
                .await
                .is_err(),
            "a cataloged segment must wait instead of ending before its parts directory exists"
        );
        std::fs::create_dir(&parts_directory).unwrap();
        write_part(&parts_directory, 0, store_info, &[data]);
        let first = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("filesystem publication did not wake live playback")
            .expect("live playback ended before its first durable part")
            .expect("live playback failed after its first durable part");
        assert!(matches!(first, LogMsg::SetStoreInfo(_)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn old_tail_parts_are_skipped_without_emitting_empty_store_updates() {
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
            .find(|message| matches!(message, LogMsg::ArrowMsg(_, _)))
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("old.rrd");
        write_part(directory.path(), 0, store_info, &[data]);
        std::fs::rename(directory.path().join("00000000000000000000.rrd"), &path).unwrap();
        let (sender, mut receiver) = mpsc::channel(1);
        let mut state = LiveMessageState::default();

        assert!(
            send_part(
                &path,
                u64::MAX,
                &StoreId::recording("playback-dataset", "run-a"),
                &mut state,
                &sender,
            )
            .unwrap()
        );
        assert!(receiver.try_recv().is_err());
        assert!(!state.store_info_sent);
    }

    fn write_part(directory: &Path, sequence: u64, store_info: &LogMsg, data: &[&LogMsg]) {
        let mut encoder = Encoder::new_eager(
            CrateVersion::LOCAL,
            EncodingOptions::PROTOBUF_COMPRESSED,
            Vec::new(),
        )
        .unwrap();
        encoder.append(store_info).unwrap();
        for message in data {
            encoder.append(message).unwrap();
        }
        encoder.finish().unwrap();
        std::fs::write(
            directory.join(format!("{sequence:020}.rrd")),
            encoder.into_inner().unwrap(),
        )
        .unwrap();
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
