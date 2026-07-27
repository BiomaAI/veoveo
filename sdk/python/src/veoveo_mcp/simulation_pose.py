"""Provider-neutral Simulation View latest-pose producer SDK.

The binary layout in this module is the Python implementation of
``veoveo.io/simulation-view-pose/v1``. A producer offers complete snapshots to
``LatestPosePublisher`` without waiting for DNS, TLS, or network I/O. The
publisher keeps only the newest offered snapshot and performs TLS 1.3 mutual
authentication on its own worker thread.
"""

from __future__ import annotations

import hashlib
import math
import socket
import ssl
import struct
import threading
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from types import TracebackType
from typing import Final, Self


POSE_PROTOCOL_VERSION: Final = 1
POSE_PROTOCOL_SCHEMA: Final = "veoveo.io/simulation-view-pose/v1"
_MAGIC: Final = b"VVPOSE01"
_HEADER_BYTES: Final = 116
_IDENTITY_CHARACTERS: Final = frozenset(
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_."
)


class PoseProtocolError(ValueError):
    """A pose snapshot or producer configuration violates the public contract."""


def _validate_identifier(kind: str, value: str) -> str:
    if (
        not isinstance(value, str)
        or not 1 <= len(value) <= 128
        or any(character not in _IDENTITY_CHARACTERS for character in value)
    ):
        raise PoseProtocolError(
            f"{kind} must contain 1-128 ASCII letters, digits, dashes, "
            "underscores, or periods"
        )
    return value


@dataclass(frozen=True, slots=True)
class SessionId:
    value: str

    def __post_init__(self) -> None:
        _validate_identifier("session_id", self.value)

    def __str__(self) -> str:
        return self.value


@dataclass(frozen=True, slots=True)
class EpochId:
    value: str

    def __post_init__(self) -> None:
        _validate_identifier("epoch_id", self.value)

    def __str__(self) -> str:
        return self.value


@dataclass(frozen=True, order=True, slots=True)
class EntityId:
    value: str

    def __post_init__(self) -> None:
        _validate_identifier("entity_id", self.value)

    def __str__(self) -> str:
        return self.value


@dataclass(frozen=True, slots=True)
class Sha256Digest:
    value: str

    def __post_init__(self) -> None:
        value = self.value
        if (
            not isinstance(value, str)
            or len(value) != 71
            or not value.startswith("sha256:")
            or any(character not in "0123456789abcdef" for character in value[7:])
        ):
            raise PoseProtocolError(
                "SHA-256 digest must be lowercase sha256:<64 hexadecimal characters>"
            )

    @classmethod
    def from_bytes(cls, value: bytes) -> Self:
        if len(value) != 32:
            raise PoseProtocolError("a SHA-256 digest must contain exactly 32 bytes")
        return cls(f"sha256:{value.hex()}")

    @property
    def bytes(self) -> bytes:
        return bytes.fromhex(self.value[7:])

    def __str__(self) -> str:
        return self.value


@dataclass(frozen=True, slots=True)
class FrameRevision:
    uri: str
    digest: Sha256Digest

    def __post_init__(self) -> None:
        if not isinstance(self.uri, str):
            raise PoseProtocolError("frame revision URI must be a string")
        encoded = _encoded_text("frame revision URI", self.uri)
        if not self.uri.startswith("frames://world/") or len(encoded) > 512:
            raise PoseProtocolError(
                "frame revision must be a frames://world/ URI of at most 512 bytes"
            )
        if not isinstance(self.digest, Sha256Digest):
            raise PoseProtocolError("frame revision digest must be a Sha256Digest")


class CoordinateConvention(str, Enum):
    ENU_METERS_FLU_XYZW = "enu_meters_flu_xyzw"


@dataclass(frozen=True, slots=True)
class EnuPosition:
    east_m: float
    north_m: float
    up_m: float

    def validate(self, entity_id: EntityId) -> None:
        if not all(
            math.isfinite(value) for value in (self.east_m, self.north_m, self.up_m)
        ):
            raise PoseProtocolError(f"entity {entity_id} has non-finite position")


@dataclass(frozen=True, slots=True)
class QuaternionXyzw:
    x: float
    y: float
    z: float
    w: float

    def validate(self, entity_id: EntityId) -> None:
        components = (self.x, self.y, self.z, self.w)
        if not all(math.isfinite(value) for value in components):
            raise PoseProtocolError(f"entity {entity_id} has non-finite orientation")
        norm_squared = sum(value * value for value in components)
        if abs(norm_squared - 1.0) > 1.0e-3:
            raise PoseProtocolError(
                f"entity {entity_id} orientation quaternion is not normalized"
            )


@dataclass(frozen=True, slots=True)
class FluVelocity:
    forward_mps: float
    left_mps: float
    up_mps: float
    roll_rps: float
    pitch_rps: float
    yaw_rps: float

    def validate(self, entity_id: EntityId) -> None:
        values = (
            self.forward_mps,
            self.left_mps,
            self.up_mps,
            self.roll_rps,
            self.pitch_rps,
            self.yaw_rps,
        )
        if not all(math.isfinite(value) for value in values):
            raise PoseProtocolError(f"entity {entity_id} has non-finite velocity")
        try:
            struct.pack("!6f", *values)
        except (OverflowError, struct.error) as error:
            raise PoseProtocolError(
                f"entity {entity_id} velocity exceeds the f32 wire range"
            ) from error


@dataclass(frozen=True, slots=True)
class Rgba8:
    red: int
    green: int
    blue: int
    alpha: int

    def __post_init__(self) -> None:
        for name, value in (
            ("red", self.red),
            ("green", self.green),
            ("blue", self.blue),
            ("alpha", self.alpha),
        ):
            if (
                isinstance(value, bool)
                or not isinstance(value, int)
                or not 0 <= value <= 255
            ):
                raise PoseProtocolError(f"{name} must be an unsigned 8-bit integer")


@dataclass(frozen=True, slots=True)
class SemanticDisplayState:
    color: Rgba8
    status_code: int

    def __post_init__(self) -> None:
        if (
            isinstance(self.status_code, bool)
            or not isinstance(self.status_code, int)
            or not 0 <= self.status_code <= 65_535
        ):
            raise PoseProtocolError("status_code must be an unsigned 16-bit integer")


@dataclass(frozen=True, slots=True)
class EntityPose:
    entity_id: EntityId
    position: EnuPosition
    orientation: QuaternionXyzw
    active: bool = True
    visible: bool = True
    velocity: FluVelocity | None = None
    display: SemanticDisplayState | None = None

    def validate(self) -> None:
        if not isinstance(self.entity_id, EntityId):
            raise PoseProtocolError("entity_id must be an EntityId")
        if not isinstance(self.position, EnuPosition):
            raise PoseProtocolError("position must be an EnuPosition")
        if not isinstance(self.orientation, QuaternionXyzw):
            raise PoseProtocolError("orientation must be a QuaternionXyzw")
        if self.velocity is not None and not isinstance(self.velocity, FluVelocity):
            raise PoseProtocolError("velocity must be a FluVelocity")
        if self.display is not None and not isinstance(
            self.display, SemanticDisplayState
        ):
            raise PoseProtocolError("display must be a SemanticDisplayState")
        if not isinstance(self.active, bool) or not isinstance(self.visible, bool):
            raise PoseProtocolError("active and visible must be booleans")
        self.position.validate(self.entity_id)
        self.orientation.validate(self.entity_id)
        if self.velocity is not None:
            self.velocity.validate(self.entity_id)


@dataclass(frozen=True, slots=True)
class PoseLimits:
    max_entities: int = 10_000
    max_message_bytes: int = 4 * 1024 * 1024

    def __post_init__(self) -> None:
        if not 1 <= self.max_entities <= 1_000_000:
            raise PoseProtocolError("max_entities must be between 1 and 1,000,000")
        if not _HEADER_BYTES < self.max_message_bytes <= 2**32 - 1:
            raise PoseProtocolError(
                "max_message_bytes must fit u32 and exceed the protocol header"
            )


@dataclass(frozen=True, slots=True)
class PoseSnapshot:
    session_id: SessionId
    epoch_id: EpochId
    sequence: int
    simulation_timestamp_ns: int
    frame_revision: FrameRevision
    entity_table_revision: int
    entity_table_digest: Sha256Digest
    entities: tuple[EntityPose, ...]
    protocol_version: int = POSE_PROTOCOL_VERSION
    coordinate_convention: CoordinateConvention = (
        CoordinateConvention.ENU_METERS_FLU_XYZW
    )

    def __post_init__(self) -> None:
        if not isinstance(self.session_id, SessionId):
            raise PoseProtocolError("session_id must be a SessionId")
        if not isinstance(self.epoch_id, EpochId):
            raise PoseProtocolError("epoch_id must be an EpochId")
        if not isinstance(self.frame_revision, FrameRevision):
            raise PoseProtocolError("frame_revision must be a FrameRevision")
        if not isinstance(self.entity_table_digest, Sha256Digest):
            raise PoseProtocolError("entity_table_digest must be a Sha256Digest")
        if not isinstance(self.entities, tuple) or any(
            not isinstance(entity, EntityPose) for entity in self.entities
        ):
            raise PoseProtocolError("entities must be an immutable tuple of EntityPose")

    @classmethod
    def build(
        cls,
        *,
        session_id: SessionId,
        epoch_id: EpochId,
        sequence: int,
        simulation_timestamp_ns: int,
        frame_revision: FrameRevision,
        entity_table_revision: int,
        entities: tuple[EntityPose, ...] | list[EntityPose],
    ) -> Self:
        ordered = tuple(sorted(entities, key=lambda entity: entity.entity_id))
        return cls(
            session_id=session_id,
            epoch_id=epoch_id,
            sequence=sequence,
            simulation_timestamp_ns=simulation_timestamp_ns,
            frame_revision=frame_revision,
            entity_table_revision=entity_table_revision,
            entity_table_digest=entity_table_digest(
                entity_table_revision, tuple(entity.entity_id for entity in ordered)
            ),
            entities=ordered,
        )

    def validate(self, limits: PoseLimits = PoseLimits()) -> None:
        if self.protocol_version != POSE_PROTOCOL_VERSION:
            raise PoseProtocolError(
                f"unsupported pose protocol version {self.protocol_version}"
            )
        if (
            isinstance(self.sequence, bool)
            or not isinstance(self.sequence, int)
            or not 1 <= self.sequence <= 2**64 - 1
        ):
            raise PoseProtocolError("sequence must be a nonzero unsigned 64-bit integer")
        if (
            isinstance(self.simulation_timestamp_ns, bool)
            or not isinstance(self.simulation_timestamp_ns, int)
            or not 0 <= self.simulation_timestamp_ns <= 2**63 - 1
        ):
            raise PoseProtocolError(
                "simulation_timestamp_ns must be a nonnegative signed 64-bit integer"
            )
        if (
            isinstance(self.entity_table_revision, bool)
            or not isinstance(self.entity_table_revision, int)
            or not 0 <= self.entity_table_revision <= 2**64 - 1
        ):
            raise PoseProtocolError(
                "entity_table_revision must be an unsigned 64-bit integer"
            )
        if self.coordinate_convention is not CoordinateConvention.ENU_METERS_FLU_XYZW:
            raise PoseProtocolError("unsupported coordinate convention")
        if not 1 <= len(self.entities) <= limits.max_entities:
            raise PoseProtocolError(
                f"snapshot has {len(self.entities)} entities; "
                f"maximum is {limits.max_entities}"
            )
        previous: EntityId | None = None
        for entity in self.entities:
            if previous is not None and previous >= entity.entity_id:
                raise PoseProtocolError(
                    "entity identities must be strictly ordered and unique"
                )
            entity.validate()
            previous = entity.entity_id
        expected = entity_table_digest(
            self.entity_table_revision,
            tuple(entity.entity_id for entity in self.entities),
        )
        if self.entity_table_digest != expected:
            raise PoseProtocolError(
                "entity table digest does not match the ordered identities"
            )


def entity_table_digest(
    revision: int, entity_ids: tuple[EntityId, ...] | list[EntityId]
) -> Sha256Digest:
    if (
        isinstance(revision, bool)
        or not isinstance(revision, int)
        or not 0 <= revision <= 2**64 - 1
    ):
        raise PoseProtocolError("entity table revision must be an unsigned 64-bit integer")
    hasher = hashlib.sha256()
    hasher.update(struct.pack("!Q", revision))
    for entity_id in entity_ids:
        if not isinstance(entity_id, EntityId):
            raise PoseProtocolError("entity table entries must be EntityId values")
        encoded = _encoded_text("entity_id", entity_id.value)
        hasher.update(struct.pack("!H", len(encoded)))
        hasher.update(encoded)
    return Sha256Digest.from_bytes(hasher.digest())


def encode_snapshot(
    snapshot: PoseSnapshot, limits: PoseLimits = PoseLimits()
) -> bytes:
    """Validate and deterministically encode one complete latest-pose snapshot."""

    snapshot.validate(limits)
    session = _encoded_text("session_id", snapshot.session_id.value)
    epoch = _encoded_text("epoch_id", snapshot.epoch_id.value)
    frame_uri = _encoded_text("frame revision URI", snapshot.frame_revision.uri)
    output = bytearray()
    output.extend(_MAGIC)
    output.extend(
        struct.pack(
            "!HHIQqQIHHHH",
            snapshot.protocol_version,
            0,
            0,
            snapshot.sequence,
            snapshot.simulation_timestamp_ns,
            snapshot.entity_table_revision,
            len(snapshot.entities),
            len(session),
            len(epoch),
            len(frame_uri),
            1,
        )
    )
    output.extend(snapshot.frame_revision.digest.bytes)
    output.extend(snapshot.entity_table_digest.bytes)
    output.extend(session)
    output.extend(epoch)
    output.extend(frame_uri)

    for entity in snapshot.entities:
        entity_id = _encoded_text("entity_id", entity.entity_id.value)
        flags = (
            int(entity.active)
            | (int(entity.visible) << 1)
            | (int(entity.velocity is not None) << 2)
            | (int(entity.display is not None) << 3)
        )
        output.extend(struct.pack("!HBB", len(entity_id), flags, 0))
        output.extend(
            struct.pack(
                "!7d",
                entity.position.east_m,
                entity.position.north_m,
                entity.position.up_m,
                entity.orientation.x,
                entity.orientation.y,
                entity.orientation.z,
                entity.orientation.w,
            )
        )
        if entity.velocity is not None:
            output.extend(
                struct.pack(
                    "!6f",
                    entity.velocity.forward_mps,
                    entity.velocity.left_mps,
                    entity.velocity.up_mps,
                    entity.velocity.roll_rps,
                    entity.velocity.pitch_rps,
                    entity.velocity.yaw_rps,
                )
            )
        if entity.display is not None:
            color = entity.display.color
            output.extend(
                struct.pack(
                    "!4BH",
                    color.red,
                    color.green,
                    color.blue,
                    color.alpha,
                    entity.display.status_code,
                )
            )
        output.extend(entity_id)

    if len(output) > limits.max_message_bytes:
        raise PoseProtocolError(
            f"pose message is {len(output)} bytes; maximum is "
            f"{limits.max_message_bytes}"
        )
    output[12:16] = struct.pack("!I", len(output))
    return bytes(output)


def encode_stream_frame(
    snapshot: PoseSnapshot, limits: PoseLimits = PoseLimits()
) -> bytes:
    message = encode_snapshot(snapshot, limits)
    return struct.pack("!I", len(message)) + message


def _encoded_text(kind: str, value: str) -> bytes:
    try:
        encoded = value.encode("utf-8")
    except UnicodeEncodeError as error:
        raise PoseProtocolError(f"{kind} is not valid UTF-8") from error
    if len(encoded) > 65_535:
        raise PoseProtocolError(f"{kind} exceeds the unsigned 16-bit wire length")
    return encoded


@dataclass(frozen=True, slots=True)
class PoseTlsConfig:
    host: str
    port: int
    server_hostname: str
    ca_certificate: Path
    client_certificate: Path
    client_private_key: Path
    connect_timeout_seconds: float = 2.0
    send_timeout_seconds: float = 2.0
    reconnect_initial_seconds: float = 0.1
    reconnect_maximum_seconds: float = 2.0

    def __post_init__(self) -> None:
        if (
            not self.host
            or any(character.isspace() for character in self.host)
            or "/" in self.host
        ):
            raise PoseProtocolError("pose TLS host must be a nonempty DNS name or IP")
        if not 1 <= self.port <= 65_535:
            raise PoseProtocolError("pose TLS port must be between 1 and 65535")
        if (
            not self.server_hostname
            or any(character.isspace() for character in self.server_hostname)
            or "/" in self.server_hostname
        ):
            raise PoseProtocolError("pose TLS server_hostname must be a DNS name")
        for name, path in (
            ("CA certificate", self.ca_certificate),
            ("client certificate", self.client_certificate),
            ("client private key", self.client_private_key),
        ):
            if (
                not isinstance(path, Path)
                or not path.is_absolute()
                or ".." in path.parts
            ):
                raise PoseProtocolError(f"{name} path must be absolute and normalized")
        if not 0.05 <= self.connect_timeout_seconds <= 30.0:
            raise PoseProtocolError(
                "connect_timeout_seconds must be between 0.05 and 30"
            )
        if not 0.05 <= self.send_timeout_seconds <= 30.0:
            raise PoseProtocolError("send_timeout_seconds must be between 0.05 and 30")
        if not 0.01 <= self.reconnect_initial_seconds <= 30.0:
            raise PoseProtocolError(
                "reconnect_initial_seconds must be between 0.01 and 30"
            )
        if not (
            self.reconnect_initial_seconds
            <= self.reconnect_maximum_seconds
            <= 60.0
        ):
            raise PoseProtocolError(
                "reconnect_maximum_seconds must be at least the initial delay "
                "and no greater than 60"
            )

    def create_context(self) -> ssl.SSLContext:
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
        context.minimum_version = ssl.TLSVersion.TLSv1_3
        context.maximum_version = ssl.TLSVersion.TLSv1_3
        context.check_hostname = True
        context.verify_mode = ssl.CERT_REQUIRED
        context.load_verify_locations(cafile=str(self.ca_certificate))
        context.load_cert_chain(
            certfile=str(self.client_certificate),
            keyfile=str(self.client_private_key),
        )
        return context


@dataclass(frozen=True, slots=True)
class PosePublisherStatus:
    running: bool
    connected: bool
    offered_snapshots: int
    sent_snapshots: int
    replaced_snapshots: int
    last_sent_sequence: int | None
    last_error: str | None


class LatestPosePublisher:
    """Bounded newest-value publisher for an mTLS Simulation View pose ingress."""

    def __init__(
        self,
        config: PoseTlsConfig,
        *,
        limits: PoseLimits = PoseLimits(),
        thread_name: str = "veoveo-simulation-pose",
    ) -> None:
        self._config = config
        self._limits = limits
        self._condition = threading.Condition()
        self._pending: bytes | None = None
        self._pending_sequence: int | None = None
        self._stopping = False
        self._connected = False
        self._offered_snapshots = 0
        self._sent_snapshots = 0
        self._replaced_snapshots = 0
        self._last_sent_sequence: int | None = None
        self._last_error: str | None = None
        self._socket: ssl.SSLSocket | None = None
        self._thread = threading.Thread(
            target=self._run,
            name=thread_name,
            daemon=True,
        )
        self._thread.start()

    def offer(self, snapshot: PoseSnapshot) -> None:
        """Replace any unsent snapshot and return without performing network I/O."""

        frame = encode_stream_frame(snapshot, self._limits)
        with self._condition:
            if self._stopping:
                raise RuntimeError("pose publisher is closed")
            self._offered_snapshots += 1
            if self._pending is not None:
                self._replaced_snapshots += 1
            self._pending = frame
            self._pending_sequence = snapshot.sequence
            self._condition.notify()

    def status(self) -> PosePublisherStatus:
        with self._condition:
            return PosePublisherStatus(
                running=not self._stopping and self._thread.is_alive(),
                connected=self._connected,
                offered_snapshots=self._offered_snapshots,
                sent_snapshots=self._sent_snapshots,
                replaced_snapshots=self._replaced_snapshots,
                last_sent_sequence=self._last_sent_sequence,
                last_error=self._last_error,
            )

    def close(self, timeout_seconds: float = 5.0) -> None:
        if not 0.0 <= timeout_seconds <= 30.0:
            raise ValueError("timeout_seconds must be between 0 and 30")
        with self._condition:
            if self._stopping:
                return
            self._stopping = True
            active_socket = self._socket
            self._condition.notify_all()
        if active_socket is not None:
            try:
                active_socket.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
        self._thread.join(timeout_seconds)
        if self._thread.is_alive():
            raise TimeoutError("pose publisher did not stop before the deadline")

    def __enter__(self) -> Self:
        return self

    def __exit__(
        self,
        _exception_type: type[BaseException] | None,
        _exception: BaseException | None,
        _traceback: TracebackType | None,
    ) -> None:
        self.close()

    def _run(self) -> None:
        context: ssl.SSLContext | None = None
        reconnect_delay = self._config.reconnect_initial_seconds
        while True:
            pending = self._await_pending()
            if pending is None:
                return
            frame, sequence = pending
            connection: ssl.SSLSocket | None = None
            try:
                if context is None:
                    context = self._config.create_context()
                connection = self._connect(context)
                reconnect_delay = self._config.reconnect_initial_seconds
                while True:
                    connection.sendall(frame)
                    with self._condition:
                        self._sent_snapshots += 1
                        self._last_sent_sequence = sequence
                        self._last_error = None
                    pending = self._await_pending()
                    if pending is None:
                        return
                    frame, sequence = pending
            except (OSError, ssl.SSLError) as error:
                with self._condition:
                    self._connected = False
                    self._last_error = f"{type(error).__name__}: {error}"
                    if self._pending is None:
                        self._pending = frame
                        self._pending_sequence = sequence
                if self._wait_or_stop(reconnect_delay):
                    return
                reconnect_delay = min(
                    reconnect_delay * 2.0,
                    self._config.reconnect_maximum_seconds,
                )
            finally:
                with self._condition:
                    self._socket = None
                    self._connected = False
                if connection is not None:
                    try:
                        connection.close()
                    except OSError:
                        pass

    def _await_pending(self) -> tuple[bytes, int] | None:
        with self._condition:
            self._condition.wait_for(
                lambda: self._pending is not None or self._stopping
            )
            if self._stopping:
                return None
            frame = self._pending
            sequence = self._pending_sequence
            self._pending = None
            self._pending_sequence = None
            assert frame is not None and sequence is not None
            return frame, sequence

    def _connect(self, context: ssl.SSLContext) -> ssl.SSLSocket:
        plain = socket.create_connection(
            (self._config.host, self._config.port),
            timeout=self._config.connect_timeout_seconds,
        )
        try:
            plain.settimeout(self._config.connect_timeout_seconds)
            secured = context.wrap_socket(
                plain,
                server_hostname=self._config.server_hostname,
            )
            secured.settimeout(self._config.send_timeout_seconds)
        except BaseException:
            plain.close()
            raise
        with self._condition:
            if self._stopping:
                secured.close()
                raise OSError("pose publisher is stopping")
            self._socket = secured
            self._connected = True
        return secured

    def _wait_or_stop(self, duration_seconds: float) -> bool:
        with self._condition:
            self._condition.wait_for(lambda: self._stopping, duration_seconds)
            return self._stopping
