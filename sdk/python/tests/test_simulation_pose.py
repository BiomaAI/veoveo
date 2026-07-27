import hashlib
import struct
from pathlib import Path

import pytest

from veoveo_mcp.simulation_pose import (
    CoordinateConvention,
    EntityId,
    EntityPose,
    EnuPosition,
    EpochId,
    FluVelocity,
    FrameRevision,
    PoseLimits,
    PoseProtocolError,
    PoseSnapshot,
    PoseTlsConfig,
    QuaternionXyzw,
    Rgba8,
    SemanticDisplayState,
    SessionId,
    Sha256Digest,
    encode_snapshot,
    encode_stream_frame,
    entity_table_digest,
)
def _snapshot(sequence: int = 7) -> PoseSnapshot:
    entities = [
        EntityPose(
            entity_id=EntityId("entity-b"),
            position=EnuPosition(-1.0, 0.0, 2.0),
            orientation=QuaternionXyzw(0.0, 0.0, 1.0, 0.0),
            active=False,
            visible=False,
        ),
        EntityPose(
            entity_id=EntityId("entity-a"),
            position=EnuPosition(1.0, 2.0, 3.0),
            orientation=QuaternionXyzw(0.0, 0.0, 0.0, 1.0),
            velocity=FluVelocity(5.0, 0.0, 0.1, 0.0, 0.0, 0.2),
            display=SemanticDisplayState(Rgba8(1, 2, 3, 255), 7),
        ),
    ]
    return PoseSnapshot.build(
        session_id=SessionId("session-a"),
        epoch_id=EpochId("epoch-a"),
        sequence=sequence,
        simulation_timestamp_ns=70_000_000,
        frame_revision=FrameRevision(
            "frames://world/world-a/revision/rev-a",
            Sha256Digest("sha256:" + "11" * 32),
        ),
        entity_table_revision=3,
        entities=entities,
    )


def test_binary_matches_rust_protocol_fixture() -> None:
    encoded = encode_snapshot(
        _snapshot(),
        PoseLimits(max_entities=4, max_message_bytes=4096),
    )
    assert encoded[:8] == b"VVPOSE01"
    assert struct.unpack("!I", encoded[12:16])[0] == len(encoded)
    assert len(encoded) == 335
    assert hashlib.sha256(encoded).hexdigest() == (
        "cf08001e5d89c7a72918ac42b1a63a954df66bb3e5ae91c117fe2981c5327407"
    )
    assert encode_snapshot(_snapshot()) == encoded

    framed = encode_stream_frame(_snapshot())
    assert struct.unpack("!I", framed[:4])[0] == len(encoded)
    assert framed[4:] == encoded


def test_builder_sorts_entities_and_hashes_only_ordered_identity_table() -> None:
    snapshot = _snapshot()
    assert [str(entity.entity_id) for entity in snapshot.entities] == [
        "entity-a",
        "entity-b",
    ]
    assert snapshot.entity_table_digest == entity_table_digest(
        3, [EntityId("entity-a"), EntityId("entity-b")]
    )


def test_invalid_snapshot_shapes_fail_before_encoding() -> None:
    snapshot = _snapshot()
    with pytest.raises(PoseProtocolError, match="normalized"):
        EntityPose(
            entity_id=EntityId("entity-c"),
            position=EnuPosition(0.0, 0.0, 0.0),
            orientation=QuaternionXyzw(0.0, 0.0, 0.0, 0.0),
        ).validate()
    with pytest.raises(PoseProtocolError, match="strictly ordered"):
        PoseSnapshot(
            session_id=snapshot.session_id,
            epoch_id=snapshot.epoch_id,
            sequence=snapshot.sequence,
            simulation_timestamp_ns=snapshot.simulation_timestamp_ns,
            frame_revision=snapshot.frame_revision,
            entity_table_revision=snapshot.entity_table_revision,
            entity_table_digest=snapshot.entity_table_digest,
            entities=tuple(reversed(snapshot.entities)),
            coordinate_convention=CoordinateConvention.ENU_METERS_FLU_XYZW,
        ).validate()
    with pytest.raises(PoseProtocolError, match="maximum is 1"):
        encode_snapshot(snapshot, PoseLimits(max_entities=1))


def test_strong_identities_and_digests_reject_ambiguous_values() -> None:
    with pytest.raises(PoseProtocolError):
        SessionId("../session")
    with pytest.raises(PoseProtocolError):
        EntityId("entity/one")
    with pytest.raises(PoseProtocolError):
        Sha256Digest("sha256:" + "AA" * 32)
    with pytest.raises(PoseProtocolError):
        FrameRevision(
            "https://example.test/world",
            Sha256Digest("sha256:" + "00" * 32),
        )


def test_tls_configuration_is_fail_closed() -> None:
    config = PoseTlsConfig(
        host="simulation-view-pose",
        port=7443,
        server_hostname="simulation-view-pose.veoveo.svc",
        ca_certificate=Path("/var/run/veoveo/pose/ca.crt"),
        client_certificate=Path("/var/run/veoveo/pose/tls.crt"),
        client_private_key=Path("/var/run/veoveo/pose/tls.key"),
    )
    assert config.port == 7443
    with pytest.raises(PoseProtocolError, match="absolute"):
        PoseTlsConfig(
            host="simulation-view-pose",
            port=7443,
            server_hostname="simulation-view-pose.veoveo.svc",
            ca_certificate=Path("ca.crt"),
            client_certificate=Path("/cert.crt"),
            client_private_key=Path("/key.pem"),
        )
