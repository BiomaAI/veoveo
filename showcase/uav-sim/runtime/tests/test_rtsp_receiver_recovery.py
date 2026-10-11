from __future__ import annotations

import socket
import struct
import threading
import unittest
from contextlib import ExitStack
from unittest.mock import patch

from veoveo_uav_sim.rtsp_h264 import RtspEndpoint, RtspH264Receiver
from test_native_capture_clock import metadata_nal


def picture(frame: int, *, key: bool = True, malformed: bool = False, time_ns: int | None = None) -> bytes:
    nals = [b"\x67\x4d\x40\x20", b"\x68\x01"] if key else []
    nals += [b"\x06\x05\xff" if malformed else metadata_nal(frame * 1000 if time_ns is None else time_ns, frame)]
    nals += [b"\x65\x01" if key else b"\x41\x01"]
    payload = b"\x78" + b"".join(struct.pack("!H", len(nal)) + nal for nal in nals)
    rtp = struct.pack("!BBHII", 0x80, 0x80 | 96, frame, frame * 3000, 1) + payload
    return b"$\x00" + struct.pack("!H", len(rtp)) + rtp


class LoopbackRtsp:
    """Finite RTSP peer with real accepted sockets and interleaved RTP."""

    def __init__(self, scripts):
        self.scripts = scripts
        self.listener = socket.socket()
        self.listener.bind(("127.0.0.1", 0))
        self.listener.listen(4)
        self.listener.settimeout(0.1)
        self.endpoint = RtspEndpoint("127.0.0.1", self.listener.getsockname()[1])
        self.stop = threading.Event()
        self.handshake = threading.Event()
        self.played = threading.Event()
        self.closed = []
        self.connections = 0
        self.errors = []
        self.thread = threading.Thread(target=self.run, daemon=True)

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *_):
        self.stop.set()
        self.thread.join(2)
        self.listener.close()
        if self.thread.is_alive():
            raise AssertionError("loopback RTSP peer did not retire")
        if self.errors:
            raise AssertionError("loopback RTSP peer failed") from self.errors[0]

    def run(self):
        try:
            while not self.stop.is_set():
                try:
                    conn, _ = self.listener.accept()
                except socket.timeout:
                    continue
                index = self.connections
                self.connections += 1
                with conn:
                    conn.settimeout(0.1)
                    script = self.scripts[min(index, len(self.scripts) - 1)]
                    if script == "handshake_stall":
                        self.handshake.set()
                        self.await_close(conn, index)
                        continue
                    reader = conn.makefile("rb", buffering=0)
                    try:
                        for expected in ("OPTIONS", "DESCRIBE", "SETUP", "PLAY"):
                            line = reader.readline()
                            if not line:
                                break
                            if line.split(b" ", 1)[0].decode() != expected:
                                raise AssertionError("unexpected RTSP method")
                            headers = {}
                            while True:
                                line = reader.readline()
                                if line == b"\r\n":
                                    break
                                name, value = line.decode().split(":", 1)
                                headers[name.lower()] = value.strip()
                            body = b""
                            if expected == "DESCRIBE":
                                body = b"v=0\r\nm=video 0 RTP/AVP 96\r\na=rtpmap:96 H264/90000\r\na=control:track1\r\n"
                            conn.sendall((f"RTSP/1.0 200 OK\r\nCSeq: {headers['cseq']}\r\nSession: fixture\r\nContent-Length: {len(body)}\r\n\r\n").encode() + body)
                        else:
                            self.played.set()
                            if script == "eof":
                                continue
                            if script == "delta_then_key":
                                conn.sendall(picture(2, key=False) + picture(3))
                            elif script == "backwards":
                                conn.sendall(picture(2, time_ns=500))
                            elif script != "stall":
                                conn.sendall(picture(script if isinstance(script, int) else 1,
                                                     malformed=script == "malformed"))
                            self.await_close(conn, index)
                    finally:
                        reader.close()
        except BaseException as error:
            if not self.stop.is_set():
                self.errors.append(error)

    def await_close(self, conn, index):
        while not self.stop.is_set():
            try:
                if not conn.recv(1024):
                    self.closed.append(index)
                    return
            except socket.timeout:
                continue
            except ConnectionResetError:
                self.closed.append(index)
                return


class RtspReceiverRecoveryTests(unittest.TestCase):
    def receiver(self, peer, *, interrupted=None, operator=True):
        stack = ExitStack()
        for name in ("_RTSP_HANDSHAKE_SECONDS", "_RTSP_PICTURE_SILENCE_SECONDS"):
            stack.enter_context(patch("veoveo_uav_sim.rtsp_h264." + name, 0.25, create=True))
        stack.enter_context(patch("veoveo_uav_sim.rtsp_h264._RTSP_RECONNECT_LIMIT", 2, create=True))
        self.addCleanup(stack.close)
        frames, errors = [], []
        frame_event, error_event = threading.Event(), threading.Event()
        def frame(value):
            frames.append(value)
            frame_event.set()
        def error(value):
            errors.append(value)
            error_event.set()
        kwargs = {"on_transport_interrupted": interrupted or (lambda: None)} if operator else {}
        receiver = RtspH264Receiver(peer.endpoint, frame, error, **kwargs)
        self.addCleanup(receiver.close)
        receiver.start()
        return receiver, frames, errors, frame_event, error_event

    def test_default_caller_preserves_handshake_ready_and_terminal_transport_loss(self):
        with LoopbackRtsp(["stall"]) as peer:
            receiver, frames, errors, _, failed = self.receiver(peer, operator=False)
            try:
                self.assertTrue(peer.played.wait(2))
                self.assertTrue(receiver._ready.wait(2))
                self.assertTrue(receiver.ready)
                self.assertFalse(frames)
                self.assertTrue(failed.wait(2))
                self.assertEqual(peer.connections, 1)
                self.assertEqual(len(errors), 1)
                self.assertFalse(frames)
            finally:
                receiver.close()

    def test_default_caller_eof_is_terminal_without_replacement(self):
        with LoopbackRtsp(["eof"]) as peer:
            receiver, frames, errors, _, failed = self.receiver(peer, operator=False)
            try:
                self.assertTrue(failed.wait(2))
                self.assertEqual(peer.connections, 1)
                self.assertEqual(len(errors), 1)
                self.assertFalse(frames)
            finally:
                receiver.close()

    def test_silent_picture_session_and_eof_recover_with_old_session_closed(self):
        for initial in ("stall", "eof"):
            with self.subTest(initial=initial), LoopbackRtsp([initial, 2]) as peer:
                receiver, frames, errors, delivered, _ = self.receiver(peer)
                try:
                    self.assertTrue(delivered.wait(2), "replacement picture was not delivered")
                    self.assertEqual(len(frames), 1)
                    self.assertTrue(frames[0].is_keyframe)
                    self.assertEqual(peer.connections, 2)
                    if initial == "stall":
                        self.assertIn(0, peer.closed)
                    self.assertFalse(errors)
                finally:
                    receiver.close()

    def test_replacement_waits_for_fresh_decoder_reentrant_picture(self):
        with LoopbackRtsp(["eof", "delta_then_key"]) as peer:
            receiver, frames, errors, delivered, _ = self.receiver(peer)
            try:
                self.assertTrue(delivered.wait(2))
                self.assertEqual(len(frames), 1)
                self.assertTrue(frames[0].is_keyframe)
                self.assertEqual(peer.connections, 2)
                self.assertFalse(errors)
            finally:
                receiver.close()

    def test_eof_reconnect_budget_is_finite(self):
        with LoopbackRtsp(["eof"]) as peer:
            receiver, frames, errors, _, failed = self.receiver(peer)
            try:
                self.assertTrue(failed.wait(2), "EOF budget was not exhausted")
                self.assertEqual(peer.connections, 3)
                self.assertEqual(len(errors), 1)
                self.assertFalse(frames)
            finally:
                receiver.close()

    def test_malformed_metadata_is_terminal_without_reconnect(self):
        with LoopbackRtsp(["malformed"]) as peer:
            receiver, frames, errors, _, failed = self.receiver(peer)
            try:
                self.assertTrue(failed.wait(2))
                self.assertEqual(peer.connections, 1)
                self.assertEqual(len(errors), 1)
                self.assertFalse(frames)
            finally:
                receiver.close()

    def test_clock_identity_survives_transport_reconnect(self):
        for replacement in (1, "backwards"):
            with self.subTest(replacement=replacement), LoopbackRtsp([1, replacement]) as peer:
                receiver, frames, errors, delivered, failed = self.receiver(peer)
                try:
                    self.assertTrue(delivered.wait(2))
                    self.assertTrue(failed.wait(2), "changed capture identity was accepted")
                    self.assertEqual(len(frames), 1)
                    self.assertEqual(peer.connections, 2)
                    self.assertEqual(len(errors), 1)
                finally:
                    receiver.close()

    def test_close_interrupts_handshake_without_replacement(self):
        with LoopbackRtsp(["handshake_stall"]) as peer:
            receiver, frames, errors, _, _ = self.receiver(peer)
            self.assertTrue(peer.handshake.wait(2))
            receiver.close()
            self.assertEqual(peer.connections, 1)
            self.assertFalse(receiver._thread.is_alive())
            self.assertFalse(frames)
            self.assertFalse(errors)

    def test_stop_in_recovery_callback_prevents_replacement(self):
        with LoopbackRtsp(["eof"]) as peer:
            stopped = threading.Event()
            holder = {}
            def interrupted():
                # Owner cancellation can arrive as transport recovery invalidates
                # the old product. The callback runs on the reader's own thread.
                holder["receiver"]._stop.set()
                stopped.set()
            # Prevent the peer from reaching EOF before the holder is published.
            peer.scripts[0] = "stall"
            receiver, frames, errors, _, _ = self.receiver(peer, interrupted=interrupted)
            holder["receiver"] = receiver
            self.assertTrue(stopped.wait(2))
            receiver.close()
            self.assertEqual(peer.connections, 1)
            self.assertFalse(receiver._thread.is_alive())
            self.assertFalse(frames)
            self.assertFalse(errors)
