from __future__ import annotations

import logging
import math
import queue
import secrets
import socket
import struct
import threading
from dataclasses import dataclass

from .config import StreamPublicationConfig
from .event_queue import NonBlockingEventQueue
from .h264 import NativeH264AccessUnit, annex_b_nals


LOGGER = logging.getLogger("veoveo.uav_sim.stream_output")
_RTP_CLOCK_RATE = 90_000
_MAX_RTP_PAYLOAD_BYTES = 1_200
_FU_A_TYPE = 28


@dataclass(frozen=True, slots=True)
class StreamPublicationStatus:
    lifecycle: str
    queued_access_units: int
    dropped_access_units: int
    published_access_units: int
    last_error: str | None


@dataclass(frozen=True, slots=True)
class _AccessUnitEvent:
    access_unit: NativeH264AccessUnit
    simulation_time_s: float


@dataclass(frozen=True, slots=True)
class _StopEvent:
    pass


type _PublicationEvent = _AccessUnitEvent | _StopEvent


class RtpH264Publisher:
    """Publishes existing Annex B access units without re-encoding them."""

    def __init__(self, config: StreamPublicationConfig) -> None:
        addresses = socket.getaddrinfo(
            config.host,
            config.port,
            type=socket.SOCK_DGRAM,
        )
        if not addresses:
            raise RuntimeError("Stream RTP destination did not resolve")
        family, socket_type, protocol, _, destination = addresses[0]
        self._socket = socket.socket(family, socket_type, protocol)
        self._destination = destination
        self._payload_type = config.payload_type
        self._sequence = secrets.randbits(16)
        self._ssrc = secrets.randbits(32)
        self._timestamp_offset = secrets.randbits(32)
        self._last_timestamp: int | None = None

    def publish(self, access_unit: bytes, simulation_time_s: float) -> None:
        timestamp = _rtp_timestamp(self._timestamp_offset, simulation_time_s)
        if self._last_timestamp is not None:
            delta = (timestamp - self._last_timestamp) & 0xFFFF_FFFF
            if delta == 0 or delta >= 0x8000_0000:
                raise RuntimeError("Stream RTP timestamp did not advance")
        self._last_timestamp = timestamp
        nals = annex_b_nals(access_unit)
        payloads = [
            payload
            for nal in nals
            for payload in _packetize_nal(nal, _MAX_RTP_PAYLOAD_BYTES)
        ]
        for index, payload in enumerate(payloads):
            marker = index == len(payloads) - 1
            header = struct.pack(
                "!BBHII",
                0x80,
                (0x80 if marker else 0) | self._payload_type,
                self._sequence,
                timestamp,
                self._ssrc,
            )
            written = self._socket.sendto(header + payload, self._destination)
            if written != len(header) + len(payload):
                raise RuntimeError("Stream RTP datagram was truncated")
            self._sequence = (self._sequence + 1) & 0xFFFF

    def close(self) -> None:
        self._socket.close()


class StreamPublicationWorker:
    """Nonblocking fan-out consumer for the optional live RTP transport."""

    def __init__(self, config: StreamPublicationConfig) -> None:
        self._config = config
        self._events = NonBlockingEventQueue[_PublicationEvent](
            config.queue_capacity
        )
        self._closed = threading.Event()
        self._status_lock = threading.Lock()
        self._lifecycle = "connecting"
        self._published_access_units = 0
        self._last_error: str | None = None
        self._worker = threading.Thread(
            target=self._run,
            name="uav-native-sensor-rtp",
            daemon=True,
        )
        self._worker.start()

    def offer(
        self,
        access_unit: NativeH264AccessUnit,
        simulation_time_s: float,
    ) -> None:
        if self._closed.is_set():
            return
        self._events.offer(_AccessUnitEvent(access_unit, simulation_time_s))

    def status(self) -> StreamPublicationStatus:
        with self._status_lock:
            return StreamPublicationStatus(
                lifecycle=self._lifecycle,
                queued_access_units=self._events.depth(),
                dropped_access_units=self._events.dropped(),
                published_access_units=self._published_access_units,
                last_error=self._last_error,
            )

    def close(self) -> None:
        if self._closed.is_set():
            return
        self._closed.set()
        self._events.offer(_StopEvent())
        self._worker.join(timeout=5.0)

    def _run(self) -> None:
        publisher: RtpH264Publisher | None = None
        try:
            while True:
                try:
                    event = self._events.take(0.5)
                except queue.Empty:
                    if self._closed.is_set():
                        return
                    continue
                if isinstance(event, _StopEvent):
                    self._set_status("stopped", None)
                    return
                try:
                    if publisher is None:
                        publisher = RtpH264Publisher(self._config)
                    publisher.publish(
                        event.access_unit.sample,
                        event.simulation_time_s,
                    )
                    self._mark_published()
                except Exception as error:
                    if publisher is not None:
                        publisher.close()
                        publisher = None
                    self._set_status("degraded", _bounded_diagnostic(error))
        finally:
            if publisher is not None:
                publisher.close()

    def _set_status(self, lifecycle: str, error: str | None) -> None:
        with self._status_lock:
            previous_error = self._last_error
            self._lifecycle = lifecycle
            self._last_error = error
        if lifecycle == "degraded" and error != previous_error:
            LOGGER.error(
                "native H.264 RTP publication degraded; simulation continues: %s",
                error,
            )

    def _mark_published(self) -> None:
        with self._status_lock:
            previous_lifecycle = self._lifecycle
            self._lifecycle = "ready"
            self._published_access_units += 1
            self._last_error = None
        if previous_lifecycle == "degraded":
            LOGGER.info("native H.264 RTP publication recovered")


def _bounded_diagnostic(error: BaseException) -> str:
    return f"{type(error).__name__}: {error}"[:512]


def _rtp_timestamp(timestamp_offset: int, simulation_time_s: float) -> int:
    if not 0 <= timestamp_offset <= 0xFFFF_FFFF:
        raise ValueError("RTP timestamp offset must be an unsigned 32-bit integer")
    if not math.isfinite(simulation_time_s) or simulation_time_s < 0.0:
        raise ValueError("RTP simulation time must be finite and non-negative")
    return (timestamp_offset + round(simulation_time_s * _RTP_CLOCK_RATE)) & 0xFFFF_FFFF


def _packetize_nal(nal: bytes, maximum_payload: int) -> list[bytes]:
    if not nal:
        raise ValueError("H.264 NAL must not be empty")
    if len(nal) <= maximum_payload:
        return [nal]
    if maximum_payload < 3:
        raise ValueError("maximum RTP payload is too small for FU-A")
    nal_header = nal[0]
    fu_indicator = (nal_header & 0xE0) | _FU_A_TYPE
    nal_type = nal_header & 0x1F
    body = nal[1:]
    fragment_size = maximum_payload - 2
    fragments: list[bytes] = []
    for offset in range(0, len(body), fragment_size):
        fragment = body[offset : offset + fragment_size]
        start = offset == 0
        end = offset + len(fragment) == len(body)
        fu_header = nal_type | (0x80 if start else 0) | (0x40 if end else 0)
        fragments.append(bytes((fu_indicator, fu_header)) + fragment)
    return fragments
