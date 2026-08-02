from __future__ import annotations

import hashlib
import math
import mmap
import struct
import time
from dataclasses import dataclass
from pathlib import Path

from .contracts import ContractError, PoseSourceBinding, identity


POSE_MAGIC = b"VVPOSE01"
SHARED_MAGIC = b"VVPSHM01"
POSE_HEADER_BYTES = 116
SHARED_HEADER_BYTES = 64


@dataclass(frozen=True, slots=True)
class EntityPose:
    entity_id: str
    position_enu_m: tuple[float, float, float]
    orientation_xyzw: tuple[float, float, float, float]
    active: bool
    visible: bool


@dataclass(frozen=True, slots=True)
class PoseSnapshot:
    session_id: str
    epoch_id: str
    sequence: int
    simulation_timestamp_ns: int
    frame_uri: str
    frame_digest: str
    entity_table_revision: int
    entity_table_digest: str
    entities: tuple[EntityPose, ...]


class SharedPoseReader:
    def __init__(self, path: Path, maximum_message_bytes: int) -> None:
        self._file = path.open("rb")
        length = path.stat().st_size
        if (
            length < SHARED_HEADER_BYTES
            or (length - SHARED_HEADER_BYTES) % 2 != 0
        ):
            raise ContractError("shared pose file has an invalid length")
        self._map = mmap.mmap(
            self._file.fileno(), length, access=mmap.ACCESS_READ
        )
        if self._map[:8] != SHARED_MAGIC:
            self.close()
            raise ContractError("shared pose file magic is invalid")
        version = int.from_bytes(self._map[8:10], byteorder="little")
        slot_capacity = (length - SHARED_HEADER_BYTES) // 2
        declared = int.from_bytes(self._map[48:56], byteorder="little")
        if (
            version != 1
            or slot_capacity < 1
            or slot_capacity != declared
            or slot_capacity > maximum_message_bytes
        ):
            self.close()
            raise ContractError("shared pose slot declaration is invalid")
        self._slot_capacity = slot_capacity

    def latest(self) -> tuple[int, bytes] | None:
        for _ in range(3):
            generation = self._native_u64(16)
            if generation == 0:
                return None
            if generation & 1:
                continue
            active = self._native_u64(24)
            if active not in (0, 1):
                raise ContractError("shared pose active slot is invalid")
            length = self._native_u64(32 if active == 0 else 40)
            if length < 1 or length > self._slot_capacity:
                raise ContractError("shared pose payload length is invalid")
            start = SHARED_HEADER_BYTES + active * self._slot_capacity
            payload = self._map[start : start + length]
            if (
                generation == self._native_u64(16)
                and active == self._native_u64(24)
            ):
                return generation // 2, payload
        return None

    def close(self) -> None:
        if hasattr(self, "_map"):
            self._map.close()
        self._file.close()

    def _native_u64(self, offset: int) -> int:
        # The canonical simulation runtime is linux/amd64, matching the Rust
        # shared-memory publisher's native-endian atomic control words.
        return struct.unpack_from("=Q", self._map, offset)[0]


class PoseMirror:
    def __init__(self, directory: Path) -> None:
        self._directory = directory
        self._binding: PoseSourceBinding | None = None
        self._reader: SharedPoseReader | None = None
        self._generation = 0
        self._latest: PoseSnapshot | None = None
        self._accepted_at = 0.0

    @property
    def latest(self) -> PoseSnapshot | None:
        return self._latest

    @property
    def stale(self) -> bool:
        binding = self._binding
        return (
            binding is None
            or self._latest is None
            or (time.monotonic() - self._accepted_at) * 1000.0
            > binding.stale_after_ms
        )

    def bind(self, binding: PoseSourceBinding) -> None:
        self.close()
        path = self._directory / f"{binding.session_id}.pose"
        self._reader = SharedPoseReader(
            path, binding.maximum_message_bytes
        )
        self._binding = binding

    def renew(self, binding: PoseSourceBinding) -> None:
        current = self._binding
        if current is None or not _same_pose_data(current, binding):
            raise ContractError("pose renewal changed immutable source identity")
        if binding.authorization_revision <= current.authorization_revision:
            if binding == current:
                return
            raise ContractError("pose authorization revision is stale")
        self._binding = binding

    def revoke(self) -> None:
        self.close()

    def poll(self) -> PoseSnapshot | None:
        if self._reader is None or self._binding is None:
            return None
        value = self._reader.latest()
        if value is None or value[0] <= self._generation:
            return None
        snapshot = decode_snapshot(value[1], self._binding)
        if (
            self._latest is not None
            and snapshot.sequence <= self._latest.sequence
        ):
            self._generation = value[0]
            return None
        self._generation = value[0]
        self._latest = snapshot
        self._accepted_at = time.monotonic()
        return snapshot

    def close(self) -> None:
        if self._reader is not None:
            self._reader.close()
        self._binding = None
        self._reader = None
        self._generation = 0
        self._latest = None
        self._accepted_at = 0.0


def _same_pose_data(
    left: PoseSourceBinding, right: PoseSourceBinding
) -> bool:
    return (
        left.session_id == right.session_id
        and left.epoch_id == right.epoch_id
        and left.frame_uri == right.frame_uri
        and left.frame_digest == right.frame_digest
        and left.entity_table_revision == right.entity_table_revision
        and left.entity_table_digest == right.entity_table_digest
        and left.maximum_entities == right.maximum_entities
        and left.maximum_message_bytes == right.maximum_message_bytes
        and left.stale_after_ms == right.stale_after_ms
        and left.producer_id == right.producer_id
        and left.producer_spiffe_id == right.producer_spiffe_id
    )


class Reader:
    def __init__(self, value: bytes) -> None:
        self._value = value
        self._offset = 0

    @property
    def remaining(self) -> int:
        return len(self._value) - self._offset

    def take(self, count: int) -> bytes:
        if count < 0 or self.remaining < count:
            raise ContractError("pose snapshot is truncated")
        result = self._value[self._offset : self._offset + count]
        self._offset += count
        return result

    def unpack(self, shape: str) -> tuple[object, ...]:
        size = struct.calcsize(shape)
        result = struct.unpack(shape, self.take(size))
        return result

    def text(self, count: int, label: str) -> str:
        try:
            value = self.take(count).decode("utf-8")
        except UnicodeDecodeError as error:
            raise ContractError(f"{label} is not UTF-8") from error
        return value


def decode_snapshot(
    value: bytes, binding: PoseSourceBinding
) -> PoseSnapshot:
    if (
        len(value) < POSE_HEADER_BYTES
        or len(value) > binding.maximum_message_bytes
    ):
        raise ContractError("pose snapshot size is invalid")
    reader = Reader(value)
    if reader.take(8) != POSE_MAGIC:
        raise ContractError("pose snapshot magic is invalid")
    version, flags, declared_length = reader.unpack(">HHI")
    if version != 1 or flags != 0 or declared_length != len(value):
        raise ContractError("pose snapshot header is invalid")
    (
        sequence,
        simulation_timestamp_ns,
        entity_table_revision,
        entity_count,
        session_length,
        epoch_length,
        frame_length,
        convention,
    ) = reader.unpack(">QqQIHHHH")
    frame_digest = f"sha256:{reader.take(32).hex()}"
    entity_table_digest = f"sha256:{reader.take(32).hex()}"
    session_id = identity(
        "sessionId", reader.text(session_length, "sessionId")
    )
    epoch_id = identity("epochId", reader.text(epoch_length, "epochId"))
    frame_uri = reader.text(frame_length, "frameRevision")
    if (
        sequence < 1
        or simulation_timestamp_ns < 0
        or convention != 1
        or entity_count < 1
        or entity_count > binding.maximum_entities
        or session_id != binding.session_id
        or epoch_id != binding.epoch_id
        or frame_uri != binding.frame_uri
        or frame_digest != binding.frame_digest
        or entity_table_revision != binding.entity_table_revision
        or entity_table_digest != binding.entity_table_digest
    ):
        raise ContractError("pose snapshot does not match its binding")

    entities: list[EntityPose] = []
    previous = ""
    identity_hasher = hashlib.sha256()
    identity_hasher.update(struct.pack(">Q", entity_table_revision))
    for _ in range(entity_count):
        entity_length, entity_flags, reserved = reader.unpack(">HBB")
        if entity_flags & ~0x0F or reserved != 0:
            raise ContractError("pose entity flags are invalid")
        values = reader.unpack(">7d")
        if not all(math.isfinite(component) for component in values):
            raise ContractError("pose entity transform is non-finite")
        quaternion = tuple(float(component) for component in values[3:])
        if (
            abs(
                sum(component * component for component in quaternion) - 1.0
            )
            > 1e-3
        ):
            raise ContractError("pose entity quaternion is not normalized")
        if entity_flags & 0x04:
            velocity = reader.unpack(">6f")
            if not all(math.isfinite(component) for component in velocity):
                raise ContractError("pose entity velocity is non-finite")
        if entity_flags & 0x08:
            reader.take(4)
            reader.unpack(">H")
        entity_id = identity(
            "entityId", reader.text(entity_length, "entityId")
        )
        if entity_id <= previous:
            raise ContractError(
                "pose entity identities are not strictly ordered"
            )
        previous = entity_id
        encoded_identity = entity_id.encode("utf-8")
        identity_hasher.update(struct.pack(">H", len(encoded_identity)))
        identity_hasher.update(encoded_identity)
        entities.append(
            EntityPose(
                entity_id=entity_id,
                position_enu_m=tuple(
                    float(component) for component in values[:3]
                ),
                orientation_xyzw=quaternion,
                active=bool(entity_flags & 0x01),
                visible=bool(entity_flags & 0x02),
            )
        )
    if reader.remaining:
        raise ContractError("pose snapshot has trailing bytes")
    if (
        f"sha256:{identity_hasher.hexdigest()}"
        != binding.entity_table_digest
    ):
        raise ContractError("pose entity table digest is invalid")
    return PoseSnapshot(
        session_id=session_id,
        epoch_id=epoch_id,
        sequence=sequence,
        simulation_timestamp_ns=simulation_timestamp_ns,
        frame_uri=frame_uri,
        frame_digest=frame_digest,
        entity_table_revision=entity_table_revision,
        entity_table_digest=entity_table_digest,
        entities=tuple(entities),
    )
