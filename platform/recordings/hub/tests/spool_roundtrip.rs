//! Integration: push through the embedded proxy exactly like production, then
//! read the durable segments back with the query engine. Exercises the real
//! path (proxy → receiver → spooler → segment file → QueryEngine), the
//! restart-resume `.rN` behavior, and blueprint filtering.

use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use re_grpc_server::{MemoryLimit, ServerOptions, shutdown};
use re_sdk::RecordingStreamBuilder;
use re_sdk_types::archetypes::{Scalars, VideoStream};
use re_sdk_types::components::VideoCodec;
use veoveo_recording_hub::config::{DatasetName, DatasetRoute, SpoolerConfig};
use veoveo_recording_hub::spool::{Spooler, run_blocking};
use veoveo_recording_hub::{VideoClipRequest, extract_video_clip, remux_h264_mp4};

const H264_FIXTURE: &str = "AAAAAQkQAAAAAWdCwAraJbARAAADAAEAAAMABI8SJqAAAAABaM4PyAAAAQYF//9N3EXpvebZSLeWLNgg2SPu73gyNjQgLSBjb3JlIDE2NSByMzIyMiBiMzU2MDVhIC0gSC4yNjQvTVBFRy00IEFWQyBjb2RlYyAtIENvcHlsZWZ0IDIwMDMtMjAyNSAtIGh0dHA6Ly93d3cudmlkZW9sYW4ub3JnL3gyNjQuaHRtbCAtIG9wdGlvbnM6IGNhYmFjPTAgcmVmPTEgZGVibG9jaz0wOjA6MCBhbmFseXNlPTA6MCBtZT1kaWEgc3VibWU9MCBwc3k9MSBwc3lfcmQ9MS4wMDowLjAwIG1peGVkX3JlZj0wIG1lX3JhbmdlPTE2IGNocm9tYV9tZT0xIHRyZWxsaXM9MCA4eDhkY3Q9MCBjcW09MCBkZWFkem9uZT0yMSwxMSBmYXN0X3Bza2lwPTEgY2hyb21hX3FwX29mZnNldD0wIHRocmVhZHM9MSBsb29rYWhlYWRfdGhyZWFkcz0xIHNsaWNlZF90aHJlYWRzPTAgbnI9MCBkZWNpbWF0ZT0xIGludGVybGFjZWQ9MCBibHVyYXlfY29tcGF0PTAgY29uc3RyYWluZWRfaW50cmE9MCBiZnJhbWVzPTAgd2VpZ2h0cD0wIGtleWludD0yIGtleWludF9taW49MiBzY2VuZWN1dD0wIGludHJhX3JlZnJlc2g9MCByYz1jcmYgbWJ0cmVlPTAgY3JmPTIzLjAgcWNvbXA9MC42MCBxcG1pbj0wIHFwbWF4PTY5IHFwc3RlcD00IGlwX3JhdGlvPTEuNDAgYXE9MACAAAABZYiEOhGKAAIY8cAAQPY4AAh5SddeAAAAAQkwAAABQZogEqLAAAAAAQkQAAAAAWdCwAraJbARAAADAAEAAAMABI8SJqAAAAABaM4PyAAAAWWIggMoRigACT3HAAEOuOAAJfEnXXgAAAABCTAAAAFBmiASosA=";

/// Reserve a free localhost port for the proxy.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral")
        .local_addr()
        .expect("local addr")
        .port()
}

fn config(spool: &std::path::Path, bind: SocketAddr) -> SpoolerConfig {
    SpoolerConfig {
        bind,
        spool_dir: spool.to_path_buf(),
        datasets: vec![DatasetRoute {
            dataset: DatasetName::new("world").unwrap(),
            application_id_prefix: String::new(),
        }],
        segment_max_bytes: 64 * 1024 * 1024,
        segment_max_age_s: 3600,
        recording_idle_timeout_s: 3600,
        flush_interval_ms: 50,
        fsync_on_flush: true,
        live_queue_limit_bytes: 256 * 1024 * 1024,
    }
}

/// Spawn the proxy + spooler, run `body` (which pushes data), then stop and
/// return the segment tree row counts per recording on timeline `tick`.
async fn spool_run(
    spool_dir: &std::path::Path,
    n_messages: usize,
    recording: &str,
) -> std::collections::BTreeMap<String, u64> {
    let port = free_port();
    let bind: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let cfg = config(spool_dir, bind);

    let (signal, shutdown) = shutdown::shutdown();
    let options = ServerOptions {
        memory_limit: MemoryLimit::from_bytes(cfg.live_queue_limit_bytes),
        ..Default::default()
    };
    let (receiver, _handle) = re_grpc_server::spawn_with_recv(bind, options, shutdown);

    let stop = Arc::new(AtomicBool::new(false));
    let stop_drain = stop.clone();
    let drain = std::thread::spawn(move || {
        let spooler = Spooler::new(cfg).expect("spooler");
        run_blocking(
            spooler,
            receiver,
            stop_drain,
            Duration::from_millis(50),
            Duration::from_secs(3600),
        )
    });

    // Give the tonic server a moment to bind.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let proxy = format!("rerun+http://127.0.0.1:{port}/proxy");
    let stream = RecordingStreamBuilder::new("veoveo-sim-test")
        .recording_id(recording.to_string())
        .connect_grpc_opts(proxy)
        .expect("connect");
    for i in 0..n_messages {
        stream.set_time_sequence("tick", i as i64);
        stream
            .log("/world/sim/test", &Scalars::new([i as f64]))
            .expect("log");
    }
    stream.flush_blocking().expect("flush");
    drop(stream);

    // Let the messages arrive, then stop and freeze.
    tokio::time::sleep(Duration::from_millis(500)).await;
    stop.store(true, Ordering::SeqCst);
    signal.stop();
    let counters = drain.join().expect("join").expect("run_blocking");
    assert!(counters.messages > 0, "spooler saw messages");

    let result =
        veoveo_recording_hub::query_tree(spool_dir, "/**", "tick", 100_000).expect("query");
    result.rows_by_recording
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn roundtrip_counts_match() {
    let dir = tempfile::tempdir().expect("tempdir");
    let counts = spool_run(dir.path(), 25, "rec-round").await;
    assert_eq!(
        counts.get("rec-round").copied(),
        Some(25),
        "all 25 scalar rows are durable and queryable: {counts:?}"
    );

    // The world dataset directory exists and holds a segment.
    let segments = veoveo_recording_hub::collect_segments(dir.path()).expect("collect");
    assert!(!segments.is_empty(), "at least one segment written");
    assert!(
        segments
            .iter()
            .any(|p| p.to_string_lossy().contains("/world/")),
        "segment routed into the world dataset: {segments:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn restart_resumes_into_sibling_segment() {
    let dir = tempfile::tempdir().expect("tempdir");

    // First session: write, freeze, stop.
    let first = spool_run(dir.path(), 10, "rec-resume").await;
    assert_eq!(first.get("rec-resume").copied(), Some(10));

    // Second session with the SAME recording id: must not truncate the prior
    // file — it resumes into a `.rN` sibling, and the total is cumulative.
    let second = spool_run(dir.path(), 15, "rec-resume").await;
    assert_eq!(
        second.get("rec-resume").copied(),
        Some(25),
        "both sessions' rows are durable across restart: {second:?}"
    );

    // Two physical segments for one recording (base + .r1).
    let segments = veoveo_recording_hub::collect_segments(dir.path()).expect("collect");
    let resume_segments: Vec<_> = segments
        .iter()
        .filter(|p| p.to_string_lossy().contains("rec-resume"))
        .collect();
    assert_eq!(
        resume_segments.len(),
        2,
        "restart created a sibling, not a truncation: {resume_segments:?}"
    );
}

async fn spool_video_session(
    spool_dir: &std::path::Path,
    recording: &str,
    samples: &[(usize, Vec<u8>, bool)],
) {
    let port = free_port();
    let bind: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let cfg = config(spool_dir, bind);
    let (signal, shutdown) = shutdown::shutdown();
    let options = ServerOptions {
        memory_limit: MemoryLimit::from_bytes(cfg.live_queue_limit_bytes),
        ..Default::default()
    };
    let (receiver, _handle) = re_grpc_server::spawn_with_recv(bind, options, shutdown);
    let stop = Arc::new(AtomicBool::new(false));
    let stop_drain = stop.clone();
    let drain = std::thread::spawn(move || {
        run_blocking(
            Spooler::new(cfg).expect("spooler"),
            receiver,
            stop_drain,
            Duration::from_millis(25),
            Duration::from_secs(3600),
        )
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let stream = RecordingStreamBuilder::new("veoveo-video-test")
        .recording_id(recording.to_owned())
        .connect_grpc_opts(format!("rerun+http://127.0.0.1:{port}/proxy"))
        .expect("connect video producer");
    for (frame, bytes, keyframe) in samples {
        stream.set_duration_secs("sensor_time", *frame as f64 / 2.0);
        let mut video = VideoStream::new(VideoCodec::H264).with_sample(bytes.clone());
        if *keyframe {
            video = video.with_is_keyframe(true);
        }
        stream
            .log("/world/camera/front", &video)
            .expect("log video sample");
    }
    stream.flush_blocking().expect("flush video producer");
    drop(stream);
    tokio::time::sleep(Duration::from_millis(500)).await;
    stop.store(true, Ordering::SeqCst);
    signal.stop();
    drain.join().expect("join").expect("spool video");
}

fn fixture_access_units() -> Vec<Vec<u8>> {
    let bytes = BASE64_STANDARD
        .decode(H264_FIXTURE)
        .expect("fixture base64");
    let mut starts = Vec::new();
    for index in 0..bytes.len().saturating_sub(4) {
        if bytes[index..].starts_with(&[0, 0, 0, 1, 9]) {
            starts.push(index);
        }
    }
    starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = starts.get(index + 1).copied().unwrap_or(bytes.len());
            bytes[*start..end].to_vec()
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn h264_video_extracts_across_restart_segment_boundary() {
    let dir = tempfile::tempdir().expect("tempdir");
    let access_units = fixture_access_units();
    assert_eq!(access_units.len(), 4, "fixture contains I P I P frames");

    spool_video_session(
        dir.path(),
        "rec-video",
        &[
            (0, access_units[0].clone(), true),
            (1, access_units[1].clone(), false),
            (2, access_units[2].clone(), true),
        ],
    )
    .await;
    // The final P-frame lands in a new physical segment. Its decoder-reentrant
    // keyframe is intentionally in the prior segment.
    spool_video_session(
        dir.path(),
        "rec-video",
        &[(3, access_units[3].clone(), false)],
    )
    .await;

    let segments = veoveo_recording_hub::collect_segments(dir.path()).expect("segments");
    assert_eq!(segments.len(), 2, "restart produced two physical segments");
    let clip = extract_video_clip(
        &segments,
        &VideoClipRequest {
            application_id: "veoveo-video-test".to_owned(),
            recording_key: "rec-video".to_owned(),
            entity_path: "/world/camera/front".to_owned(),
            timeline: "sensor_time".to_owned(),
            start_index: 1_500_000_000,
            end_index: 1_500_000_000,
            max_samples: 10,
            max_encoded_bytes: 1_000_000,
        },
    )
    .expect("extract cross-segment video");
    assert_eq!(clip.decode_start_index, 1_000_000_000);
    assert_eq!(clip.samples.len(), 2, "IDR preroll plus requested P-frame");
    assert!(clip.samples[0].is_keyframe);
    assert!(!clip.samples[1].is_keyframe);

    let mp4 = remux_h264_mp4(&clip).expect("remux without re-encoding");
    let reader = mp4::Mp4Reader::read_header(std::io::Cursor::new(&mp4), mp4.len() as u64)
        .expect("read remuxed MP4");
    assert_eq!(reader.sample_count(1).expect("video track"), 2);

    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok()
    {
        let path = dir.path().join("extracted.mp4");
        std::fs::write(&path, &mp4).expect("write decode fixture");
        let status = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(path)
            .args(["-f", "null", "-"])
            .status()
            .expect("run ffmpeg decoder");
        assert!(status.success(), "remuxed MP4 decodes successfully");
    }
}
