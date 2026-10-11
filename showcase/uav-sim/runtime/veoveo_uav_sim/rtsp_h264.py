from __future__ import annotations

import base64
import logging
import socket
import struct
import threading
import time
from collections.abc import Callable
from dataclasses import dataclass
from typing import BinaryIO
from urllib.parse import urljoin

from .h264 import (
    NativeH264AccessUnit,
    make_decoder_reentrant,
    parse_native_h264_access_unit,
)

_RTP_VERSION = 2
_STAP_A = 24
_FU_A = 28
_SEQUENCE_PARAMETER_SET = 7
_PICTURE_PARAMETER_SET = 8
_RTSP_PREPARE_ATTEMPTS = 6
_RTSP_HANDSHAKE_SECONDS = 5.0
_RTSP_PICTURE_SILENCE_SECONDS = 5.0
_RTSP_RECONNECT_LIMIT = 2


@dataclass(frozen=True, slots=True)
class RtspEndpoint:
    host: str
    port: int
    path: str = "/stream"

    def __post_init__(self) -> None:
        if not self.host:
            raise ValueError("RTSP host is required")
        if not 1 <= self.port <= 65_535:
            raise ValueError("RTSP port must be between 1 and 65535")
        if not self.path.startswith("/"):
            raise ValueError("RTSP path must begin with a slash")

    @property
    def uri(self) -> str:
        return f"rtsp://{self.host}:{self.port}{self.path}"


@dataclass(frozen=True, slots=True)
class RtpPacket:
    sequence: int
    timestamp: int
    marker: bool
    payload_type: int
    payload: bytes


def parse_rtp_packet(packet: bytes) -> RtpPacket:
    if len(packet) < 12:
        raise ValueError("RTP packet is shorter than its fixed header")
    first, second, sequence, timestamp, _ssrc = struct.unpack_from("!BBHII", packet)
    if first >> 6 != _RTP_VERSION:
        raise ValueError("unsupported RTP version")
    padding = bool(first & 0x20)
    extension = bool(first & 0x10)
    contributing_sources = first & 0x0F
    cursor = 12 + contributing_sources * 4
    if cursor > len(packet):
        raise ValueError("RTP contributing-source list is truncated")
    if extension:
        if cursor + 4 > len(packet):
            raise ValueError("RTP extension header is truncated")
        extension_words = struct.unpack_from("!H", packet, cursor + 2)[0]
        cursor += 4 + extension_words * 4
        if cursor > len(packet):
            raise ValueError("RTP extension body is truncated")
    end = len(packet)
    if padding:
        padding_bytes = packet[-1]
        if padding_bytes < 1 or padding_bytes > end - cursor:
            raise ValueError("RTP padding is invalid")
        end -= padding_bytes
    payload = packet[cursor:end]
    if not payload:
        raise ValueError("RTP packet has no payload")
    return RtpPacket(
        sequence=sequence,
        timestamp=timestamp,
        marker=bool(second & 0x80),
        payload_type=second & 0x7F,
        payload=payload,
    )


class H264RtpDepacketizer:
    """Reassemble RFC 6184 RTP payloads into native Annex B access units."""

    def __init__(
        self,
        payload_type: int,
        *,
        sequence_parameter_set: bytes | None = None,
        picture_parameter_set: bytes | None = None,
    ) -> None:
        if not 0 <= payload_type <= 127:
            raise ValueError("RTP payload type must be between 0 and 127")
        self._payload_type = payload_type
        self._sequence_parameter_set = sequence_parameter_set
        self._picture_parameter_set = picture_parameter_set
        self._last_sequence: int | None = None
        self._timestamp: int | None = None
        self._nals: list[bytes] = []
        self._fragment: bytearray | None = None
        self._decoder_ready = False
        self._prefix_nals: tuple[bytes, ...] = ()
        self._prefix_timestamp: int | None = None

    def push(self, packet: RtpPacket) -> NativeH264AccessUnit | None:
        if packet.payload_type != self._payload_type:
            return None
        if self._prefix_nals and packet.timestamp != self._prefix_timestamp:
            raise RuntimeError("H.264 metadata-only unit has no picture at its RTP timestamp")
        if self._last_sequence is not None:
            expected = (self._last_sequence + 1) & 0xFFFF
            if packet.sequence != expected:
                raise RuntimeError(
                    f"H.264 RTP sequence discontinuity: expected {expected}, "
                    f"received {packet.sequence}"
                )
        self._last_sequence = packet.sequence
        if self._timestamp is None:
            self._timestamp = packet.timestamp
        elif packet.timestamp != self._timestamp:
            if self._nals or self._fragment is not None:
                raise RuntimeError(
                    "H.264 RTP timestamp changed before access-unit marker"
                )
            self._timestamp = packet.timestamp

        self._append_payload(packet.payload)
        if not packet.marker:
            return None
        if self._fragment is not None:
            raise RuntimeError("H.264 fragmented NAL ended without an FU-A end bit")
        nals = tuple(self._nals)
        self._nals.clear()
        self._timestamp = None
        if not nals:
            raise RuntimeError("H.264 RTP marker completed an empty access unit")

        for nal in nals:
            nal_type = nal[0] & 0x1F
            if nal_type == _SEQUENCE_PARAMETER_SET:
                self._sequence_parameter_set = nal
            elif nal_type == _PICTURE_PARAMETER_SET:
                self._picture_parameter_set = nal
        if not any(1 <= (nal[0] & 0x1F) <= 5 for nal in nals):
            if any((nal[0] & 0x1F) == 6 for nal in nals):
                if self._prefix_nals:
                    raise RuntimeError("H.264 repeated metadata-only unit before picture")
                self._prefix_nals = nals
                self._prefix_timestamp = packet.timestamp
            return None

        nals = self._prefix_nals + nals
        self._prefix_nals = ()
        self._prefix_timestamp = None
        sample = b"".join(b"\x00\x00\x00\x01" + nal for nal in nals)
        access_unit = parse_native_h264_access_unit(sample)
        if access_unit.is_keyframe:
            if (
                self._sequence_parameter_set is None
                or self._picture_parameter_set is None
            ):
                raise RuntimeError(
                    "native RTSP stream emitted an IDR without H.264 parameter sets"
                )
            access_unit = make_decoder_reentrant(
                access_unit,
                self._sequence_parameter_set,
                self._picture_parameter_set,
            )
            self._decoder_ready = True
        if not self._decoder_ready:
            return None
        return access_unit

    def _append_payload(self, payload: bytes) -> None:
        nal_type = payload[0] & 0x1F
        if 1 <= nal_type <= 23:
            if self._fragment is not None:
                raise RuntimeError("single H.264 NAL arrived during FU-A assembly")
            self._nals.append(payload)
            return
        if nal_type == _STAP_A:
            if self._fragment is not None:
                raise RuntimeError("H.264 STAP-A arrived during FU-A assembly")
            cursor = 1
            while cursor < len(payload):
                if cursor + 2 > len(payload):
                    raise ValueError("H.264 STAP-A NAL length is truncated")
                length = struct.unpack_from("!H", payload, cursor)[0]
                cursor += 2
                if length < 1 or cursor + length > len(payload):
                    raise ValueError("H.264 STAP-A NAL body is truncated")
                self._nals.append(payload[cursor : cursor + length])
                cursor += length
            return
        if nal_type == _FU_A:
            if len(payload) < 3:
                raise ValueError("H.264 FU-A payload is truncated")
            indicator, header = payload[0], payload[1]
            start = bool(header & 0x80)
            end = bool(header & 0x40)
            reconstructed = bytes(((indicator & 0xE0) | (header & 0x1F),))
            if start:
                if self._fragment is not None:
                    raise RuntimeError("nested H.264 FU-A start")
                self._fragment = bytearray(reconstructed)
            elif self._fragment is None:
                raise RuntimeError("H.264 FU-A continuation has no start")
            assert self._fragment is not None
            self._fragment.extend(payload[2:])
            if end:
                self._nals.append(bytes(self._fragment))
                self._fragment = None
            return
        raise ValueError(f"unsupported H.264 RTP packetization type {nal_type}")


@dataclass(frozen=True, slots=True)
class _RtspResponse:
    status: int
    headers: dict[str, str]
    body: bytes


class RtspResponseError(RuntimeError):
    def __init__(self, method: str, status: int) -> None:
        self.method = method
        self.status = status
        super().__init__(f"RTSP {method} failed with status {status}")


class _RtspSession:
    def __init__(self, endpoint: RtspEndpoint) -> None:
        self._endpoint = endpoint
        self._socket: socket.socket | None = None
        self._reader: BinaryIO | None = None
        self._cseq = 0
        self._ownership = threading.Lock()
        self._closed = False
        self._deadline = 0.0

    def close(self) -> None:
        with self._ownership:
            self._closed = True
            sock = self._socket
            reader = self._reader
            self._socket = None
            self._reader = None
        if sock is not None:
            try:
                sock.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            sock.close()
        if reader is not None:
            reader.close()

    def connect(self, deadline: float) -> H264RtpDepacketizer:
        self._deadline = deadline
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("native RTSP handshake deadline expired")
        sock = socket.create_connection(
            (self._endpoint.host, self._endpoint.port), timeout=remaining
        )
        with self._ownership:
            if self._closed:
                sock.close()
                raise RuntimeError("native RTSP session was retired during connection")
            self._socket = sock
            self._reader = sock.makefile("rb", buffering=0)
        self._request("OPTIONS", self._endpoint.uri)
        describe = self._request(
            "DESCRIBE",
            self._endpoint.uri,
            {"Accept": "application/sdp"},
        )
        payload_type, control, sequence, picture = _parse_h264_sdp(describe.body)
        base = describe.headers.get("content-base", self._endpoint.uri + "/")
        control_uri = urljoin(base, control)
        setup = self._request(
            "SETUP",
            control_uri,
            {"Transport": "RTP/AVP/TCP;unicast;interleaved=0-1"},
        )
        session = setup.headers.get("session", "").split(";", 1)[0].strip()
        if not session:
            raise RuntimeError("RTSP SETUP response omitted the session identity")
        self._request("PLAY", self._endpoint.uri, {"Session": session})
        return H264RtpDepacketizer(
            payload_type,
            sequence_parameter_set=sequence,
            picture_parameter_set=picture,
        )

    def _read(self, length: int) -> bytes:
        output = bytearray()
        while len(output) < length:
            remaining = self._deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("native RTSP read deadline expired")
            sock, reader = self._socket, self._reader
            if sock is None or reader is None:
                raise EOFError("native RTSP session is closed")
            sock.settimeout(remaining)
            chunk = reader.read(length - len(output))
            if not chunk:
                raise EOFError("native RTSP stream closed")
            output.extend(chunk)
        return bytes(output)

    def _line(self, limit: int) -> bytes:
        result = bytearray()
        while len(result) < limit:
            result.extend(self._read(1))
            if result[-1] == 10:
                return bytes(result)
        raise RuntimeError("native RTSP control line exceeds its bound")

    def receive_interleaved(self, deadline: float) -> bytes:
        self._deadline = deadline
        reader = self._reader
        if reader is None:
            raise RuntimeError("RTSP session is not connected")
        marker = self._read(1)
        if marker != b"$":
            line = marker + self._line(512)
            raise RuntimeError(
                "native RTSP stream sent an unexpected control message: "
                + line.decode("ascii", "replace").strip()
            )
        channel = self._read(1)[0]
        length = struct.unpack("!H", self._read(2))[0]
        payload = self._read(length)
        return payload if channel == 0 else b""

    def _request(
        self,
        method: str,
        uri: str,
        headers: dict[str, str] | None = None,
    ) -> _RtspResponse:
        sock = self._socket
        reader = self._reader
        if sock is None or reader is None:
            raise RuntimeError("RTSP session is not connected")
        self._cseq += 1
        fields = {
            "CSeq": str(self._cseq),
            "User-Agent": "veoveo-native-sensor/1",
            **(headers or {}),
        }
        request = (
            f"{method} {uri} RTSP/1.0\r\n"
            + "".join(f"{name}: {value}\r\n" for name, value in fields.items())
            + "\r\n"
        )
        remaining = self._deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("native RTSP handshake deadline expired")
        sock.settimeout(remaining)
        sock.sendall(request.encode("ascii"))
        status_line = self._line(4_096)
        if not status_line:
            raise EOFError(f"RTSP server closed during {method}")
        parts = status_line.decode("ascii", "replace").strip().split(" ", 2)
        if len(parts) < 2 or not parts[1].isdigit():
            raise RuntimeError(f"invalid RTSP status line during {method}")
        response_headers: dict[str, str] = {}
        while True:
            line = self._line(16_384)
            if line in (b"\r\n", b"\n"):
                break
            if not line:
                raise EOFError(f"RTSP headers ended during {method}")
            name, separator, value = line.decode("ascii", "replace").partition(":")
            if not separator:
                raise RuntimeError(f"invalid RTSP header during {method}")
            response_headers[name.strip().lower()] = value.strip()
        content_length = int(response_headers.get("content-length", "0"))
        if not 0 <= content_length <= 1024 * 1024:
            raise RuntimeError("native RTSP control body exceeds its bound")
        body = self._read(content_length) if content_length else b""
        response = _RtspResponse(int(parts[1]), response_headers, body)
        if response.status != 200:
            raise RtspResponseError(method, response.status)
        return response


class RtspH264Receiver:
    """Receive native NVENC pictures; an invalidation callback opts into recovery.

    Physical recording callers omit the callback so their pending pose/video pairing
    queues cannot survive an unannounced transport replacement. Readiness means PLAY
    succeeded, allowing those callers to enqueue render metadata before a picture.
    """

    def __init__(
        self,
        endpoint: RtspEndpoint,
        on_access_unit: Callable[[NativeH264AccessUnit], None],
        on_error: Callable[[BaseException], None],
        *,
        on_transport_interrupted: Callable[[], None] | None = None,
    ) -> None:
        self._endpoint = endpoint
        self._on_access_unit = on_access_unit
        self._on_error = on_error
        self._on_transport_interrupted = on_transport_interrupted
        self._stop = threading.Event()
        self._ready = threading.Event()
        self._session_lock = threading.RLock()
        self._session = _RtspSession(endpoint)
        self._thread = threading.Thread(
            target=self._run, name="uav-native-sensor-rtsp", daemon=True,
        )

    @property
    def ready(self) -> bool:
        return self._ready.is_set()

    def start(self) -> None:
        self._thread.start()

    def close(self) -> None:
        with self._session_lock:
            self._stop.set()
            session = self._session
        session.close()
        if self._thread.ident is not None:
            if threading.current_thread() is self._thread:
                raise RuntimeError("native RTSP receiver retirement requires its owning thread")
            self._thread.join(timeout=5.0)
            if self._thread.is_alive():
                raise RuntimeError("native RTSP receiver did not retire within 5 seconds")

    def _replace_session(self) -> bool:
        with self._session_lock:
            if self._stop.is_set():
                return False
            self._session = _RtspSession(self._endpoint)
            return True

    def _connect(self) -> H264RtpDepacketizer:
        deadline = time.monotonic() + _RTSP_HANDSHAKE_SECONDS
        for attempt in range(_RTSP_PREPARE_ATTEMPTS):
            if self._stop.is_set():
                raise EOFError("native RTSP receiver stopped")
            try:
                return self._session.connect(deadline)
            except RtspResponseError as error:
                if (error.method, error.status) != ("DESCRIBE", 503):
                    raise
                self._session.close()
                if attempt == _RTSP_PREPARE_ATTEMPTS - 1:
                    raise
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError("native RTSP preparation deadline expired")
                if self._stop.wait(min(0.5, remaining)) or not self._replace_session():
                    raise EOFError("native RTSP receiver stopped")
        raise RuntimeError("native RTSP preparation exhausted")

    def _run(self) -> None:
        last_capture = None
        reconnects = 0
        try:
            while not self._stop.is_set():
                try:
                    depacketizer = self._connect()
                    with self._session_lock:
                        if self._stop.is_set():
                            return
                        self._ready.set()
                    picture_deadline = time.monotonic() + _RTSP_PICTURE_SILENCE_SECONDS
                    while not self._stop.is_set():
                        packet = self._session.receive_interleaved(picture_deadline)
                        if not packet:
                            continue
                        access_unit = depacketizer.push(parse_rtp_packet(packet))
                        if access_unit is None:
                            continue
                        from .native_rtsp import capture_simulation_time
                        capture = capture_simulation_time(access_unit.sample)
                        if last_capture is not None and (
                            capture.publish_sim_time_ns < last_capture.publish_sim_time_ns
                            or capture.frame_num <= last_capture.frame_num
                        ):
                            raise RuntimeError("native capture clock changed generation without reconnect")
                        if last_capture is None:
                            logging.getLogger("veoveo.uav_sim.native_capture").info(
                                "native capture admitted rtsp_port=%d sim_time_ns=%d "
                                "frame_num=%d timestamp_ns=%d timestamp_iso8601=%s",
                                self._endpoint.port, capture.publish_sim_time_ns,
                                capture.frame_num, capture.timestamp, capture.timestamp_iso8601,
                            )
                        with self._session_lock:
                            if self._stop.is_set():
                                return
                            last_capture = capture
                            self._on_access_unit(access_unit)
                        picture_deadline = time.monotonic() + _RTSP_PICTURE_SILENCE_SECONDS
                except (EOFError, TimeoutError, ConnectionResetError, BrokenPipeError):
                    # Retire the old transport before admitting another same-writer session.
                    self._session.close()
                    with self._session_lock:
                        self._ready.clear()
                        if self._stop.is_set():
                            return
                        if self._on_transport_interrupted is None:
                            raise
                        self._on_transport_interrupted()
                    if reconnects >= _RTSP_RECONNECT_LIMIT:
                        raise RuntimeError("native RTSP transport recovery exhausted")
                    reconnects += 1
                    if not self._replace_session():
                        return
        except BaseException as error:
            with self._session_lock:
                if not self._stop.is_set():
                    self._on_error(error)
        finally:
            self._session.close()


def _parse_h264_sdp(body: bytes) -> tuple[int, str, bytes | None, bytes | None]:
    text = body.decode("utf-8", "strict")
    media_lines: list[str] = []
    in_video = False
    for raw_line in text.replace("\r\n", "\n").split("\n"):
        line = raw_line.strip()
        if line.startswith("m="):
            in_video = line.startswith("m=video ")
        if in_video and line:
            media_lines.append(line)
    if not media_lines:
        raise RuntimeError("RTSP description contains no video media")
    media = media_lines[0].split()
    if len(media) < 4 or not media[3].isdigit():
        raise RuntimeError("RTSP video media has no RTP payload type")
    payload_type = int(media[3])
    control = next(
        (
            line.removeprefix("a=control:")
            for line in media_lines
            if line.startswith("a=control:")
        ),
        "",
    )
    if not control:
        raise RuntimeError("RTSP video media has no control URI")
    sequence: bytes | None = None
    picture: bytes | None = None
    for line in media_lines:
        prefix = f"a=fmtp:{payload_type} "
        if not line.startswith(prefix):
            continue
        parameters = {
            name.strip(): value.strip()
            for item in line[len(prefix) :].split(";")
            if "=" in item
            for name, value in [item.split("=", 1)]
        }
        encoded = parameters.get("sprop-parameter-sets")
        if encoded:
            values = encoded.split(",")
            if len(values) >= 2:
                sequence = base64.b64decode(values[0], validate=True)
                picture = base64.b64decode(values[1], validate=True)
        break
    return payload_type, control, sequence, picture


def _read_exact(reader: BinaryIO, length: int) -> bytes:
    if length < 0:
        raise ValueError("negative stream read length")
    output = bytearray()
    while len(output) < length:
        chunk = reader.read(length - len(output))
        if not chunk:
            raise EOFError("stream ended before the declared payload length")
        output.extend(chunk)
    return bytes(output)
