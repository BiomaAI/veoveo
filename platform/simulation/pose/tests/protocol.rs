use std::time::Duration;

use veoveo_simulation_pose::{
    CoordinateConvention, EntityId, EntityPose, EnuPosition, EpochId, FluVelocity, FrameRevision,
    LatestPoseStore, POSE_PROTOCOL_VERSION, PoseBinding, PoseError, PoseLimits, PoseSnapshot,
    PoseStreamDecoder, PublishDisposition, QuaternionXyzw, Rgba8, SemanticDisplayState, SessionId,
    Sha256Digest, SharedPoseReader, SharedPoseWriter, decode_snapshot, encode_snapshot,
    encode_stream_frame, entity_table_digest,
};

fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new(format!("sha256:{}", format!("{value:02x}").repeat(32))).unwrap()
}

fn limits() -> PoseLimits {
    PoseLimits {
        max_entities: 4,
        max_message_bytes: 4096,
        max_cadence_hz: 100,
        stale_after: Duration::from_millis(100),
    }
}

fn snapshot(sequence: u64, timestamp_ns: i64, epoch: &str) -> PoseSnapshot {
    let entities = vec![
        EntityPose {
            entity_id: EntityId::new("entity-a").unwrap(),
            position: EnuPosition {
                east_m: 1.0,
                north_m: 2.0,
                up_m: 3.0,
            },
            orientation: QuaternionXyzw {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 1.0,
            },
            active: true,
            visible: true,
            velocity: Some(FluVelocity {
                forward_mps: 5.0,
                left_mps: 0.0,
                up_mps: 0.1,
                roll_rps: 0.0,
                pitch_rps: 0.0,
                yaw_rps: 0.2,
            }),
            display: Some(SemanticDisplayState {
                color: Rgba8 {
                    red: 1,
                    green: 2,
                    blue: 3,
                    alpha: 255,
                },
                status_code: 7,
            }),
        },
        EntityPose {
            entity_id: EntityId::new("entity-b").unwrap(),
            position: EnuPosition {
                east_m: -1.0,
                north_m: 0.0,
                up_m: 2.0,
            },
            orientation: QuaternionXyzw {
                x: 0.0,
                y: 0.0,
                z: 1.0,
                w: 0.0,
            },
            active: false,
            visible: false,
            velocity: None,
            display: None,
        },
    ];
    PoseSnapshot {
        protocol_version: POSE_PROTOCOL_VERSION,
        session_id: SessionId::new("session-a").unwrap(),
        epoch_id: EpochId::new(epoch).unwrap(),
        sequence,
        simulation_timestamp_ns: timestamp_ns,
        frame_revision: FrameRevision {
            uri: "frames://world/world-a/revision/rev-a".to_owned(),
            digest: digest(0x11),
        },
        coordinate_convention: CoordinateConvention::EnuMetersFluXyzw,
        entity_table_revision: 3,
        entity_table_digest: entity_table_digest(3, &entities),
        entities,
    }
}

fn binding(epoch: &str) -> PoseBinding {
    let snapshot = snapshot(1, 10_000_000, epoch);
    PoseBinding {
        session_id: snapshot.session_id,
        epoch_id: snapshot.epoch_id,
        frame_revision: snapshot.frame_revision,
        entity_table_revision: snapshot.entity_table_revision,
        entity_table_digest: snapshot.entity_table_digest,
    }
}

#[test]
fn binary_round_trip_and_fragmented_stream_are_deterministic() {
    let snapshot = snapshot(7, 70_000_000, "epoch-a");
    let encoded = encode_snapshot(&snapshot, &limits()).unwrap();
    assert_eq!(decode_snapshot(&encoded, &limits()).unwrap(), snapshot);
    assert_eq!(encode_snapshot(&snapshot, &limits()).unwrap(), encoded);

    let frame = encode_stream_frame(&encoded).unwrap();
    let mut decoder = PoseStreamDecoder::new(limits());
    assert!(decoder.push(&frame[..3]).unwrap().is_empty());
    assert_eq!(decoder.push(&frame[3..]).unwrap(), vec![snapshot]);
}

#[test]
fn latest_store_drops_old_sequences_and_reset_invalidates_old_epoch() {
    let store = LatestPoseStore::new(binding("epoch-a"), limits()).unwrap();
    assert_eq!(
        store.publish(snapshot(1, 10_000_000, "epoch-a")).unwrap(),
        PublishDisposition::Accepted
    );
    assert_eq!(
        store.publish(snapshot(1, 20_000_000, "epoch-a")).unwrap(),
        PublishDisposition::DroppedStale
    );
    store.reset_epoch(binding("epoch-b")).unwrap();
    assert!(store.latest().is_none());
    assert!(matches!(
        store.publish(snapshot(2, 30_000_000, "epoch-a")),
        Err(PoseError::BindingMismatch { field: "epoch_id" })
    ));
    assert_eq!(
        store.publish(snapshot(1, 10_000_000, "epoch-b")).unwrap(),
        PublishDisposition::Accepted
    );
}

#[test]
fn shared_memory_swaps_complete_snapshots() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("latest.pose");
    let mut writer = SharedPoseWriter::create(&path, 4096).unwrap();
    let reader = SharedPoseReader::open(&path).unwrap();
    assert!(reader.latest().unwrap().is_none());

    let first = encode_snapshot(&snapshot(1, 10_000_000, "epoch-a"), &limits()).unwrap();
    let second = encode_snapshot(&snapshot(2, 20_000_000, "epoch-a"), &limits()).unwrap();
    assert_eq!(writer.publish(&first).unwrap(), 1);
    assert_eq!(reader.latest().unwrap().unwrap(), (1, first));
    assert_eq!(writer.publish(&second).unwrap(), 2);
    assert_eq!(reader.latest().unwrap().unwrap(), (2, second));
}
