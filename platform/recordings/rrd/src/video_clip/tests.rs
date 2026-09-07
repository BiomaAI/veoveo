use super::*;
use re_sdk::RecordingStreamBuilder;

fn fixture_access_units() -> Vec<Vec<u8>> {
    let bytes = include_bytes!("../../tests/fixtures/video.h264");
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

#[test]
fn live_video_extraction_derives_keyframes_from_annex_b_samples() {
    let access_units = fixture_access_units();
    let (recording, storage) = RecordingStreamBuilder::new("veoveo-video-test")
        .recording_id("rec-video-live")
        .memory()
        .expect("memory recording");
    recording
        .log_static("/world/camera/front", &VideoStream::new(VideoCodec::H264))
        .expect("log video codec");
    for (index, sample) in access_units.iter().enumerate() {
        recording.set_duration_secs("sensor_time", index as f64 / 2.0);
        recording
            .log(
                "/world/camera/front",
                &VideoStream::update_fields().with_sample(sample.clone()),
            )
            .expect("log live video sample without keyframe metadata");
    }

    let clip = extract_video_clip_from_messages(
        storage.take(),
        &VideoClipRequest {
            application_id: "veoveo-video-test".to_owned(),
            recording_key: "rec-video-live".to_owned(),
            entity_path: "/world/camera/front".to_owned(),
            timeline: "sensor_time".to_owned(),
            start_index: 500_000_000,
            end_index: 1_500_000_000,
            max_samples: 10,
            max_encoded_bytes: 1_000_000,
        },
    )
    .expect("extract live video without a keyframe column");
    assert_eq!(clip.decode_start_index, 0);
    assert_eq!(clip.samples.len(), 4);
    assert!(clip.samples[0].is_keyframe);
    assert!(!clip.samples[1].is_keyframe);
    assert!(clip.samples[2].is_keyframe);
    assert!(!clip.samples[3].is_keyframe);
    let mp4 = remux_h264_mp4(&clip).expect("remux existing encoded access units");
    let mut reader = mp4::Mp4Reader::read_header(std::io::Cursor::new(&mp4), mp4.len() as u64)
        .expect("read MP4 sample table");
    assert_eq!(reader.sample_count(1).unwrap(), 4);
    assert_eq!(reader.tracks().get(&1).unwrap().timescale(), 90_000);
    assert!(reader.read_sample(1, 1).unwrap().unwrap().is_sync);
    assert!(!reader.read_sample(1, 2).unwrap().unwrap().is_sync);
}
