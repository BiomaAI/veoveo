from __future__ import annotations

import secrets
import socket
import struct

from .config import StreamPublicationConfig


_RTP_CLOCK_RATE = 90_000
_MAX_RTP_PAYLOAD_BYTES = 1_200
_FU_A_TYPE = 28


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
        self._last_timestamp: int | None = None

    def publish(self, access_unit: bytes, simulation_time_s: float) -> None:
        timestamp = round(simulation_time_s * _RTP_CLOCK_RATE) & 0xFFFF_FFFF
        if self._last_timestamp is not None:
            delta = (timestamp - self._last_timestamp) & 0xFFFF_FFFF
            if delta == 0 or delta >= 0x8000_0000:
                raise RuntimeError("Stream RTP timestamp did not advance")
        self._last_timestamp = timestamp
        nals = _annex_b_nals(access_unit)
        if not nals:
            raise RuntimeError("encoded camera access unit contains no Annex B NAL")
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


def _annex_b_nals(access_unit: bytes) -> list[bytes]:
    starts: list[tuple[int, int]] = []
    index = 0
    while index + 3 <= len(access_unit):
        if access_unit[index : index + 4] == b"\x00\x00\x00\x01":
            starts.append((index, 4))
            index += 4
        elif access_unit[index : index + 3] == b"\x00\x00\x01":
            starts.append((index, 3))
            index += 3
        else:
            index += 1
    output: list[bytes] = []
    for position, (start, prefix_length) in enumerate(starts):
        end = starts[position + 1][0] if position + 1 < len(starts) else len(access_unit)
        nal = access_unit[start + prefix_length : end]
        if nal:
            output.append(nal)
    return output


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
