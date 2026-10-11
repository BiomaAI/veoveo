from __future__ import annotations

import json
import unittest

from veoveo_uav_sim.native_rtsp import CaptureMetadata, capture_simulation_time
from veoveo_uav_sim.rtsp_h264 import H264RtpDepacketizer, RtpPacket


def metadata_nal(time_ns: int = 123456789, frame: int = 1, **extra: object) -> bytes:
    payload = bytes.fromhex("aa71e48f07115d80a247cd31ca6fa49c") + json.dumps({
        "publish_sim_time_ns": time_ns, "timestamp_iso8601": "2026-10-10T00:00:00Z",
        "timestamp": 1791590400000000000, "frame_num": frame, **extra,
    }).encode()
    size = b"\xff" * (len(payload) // 255) + bytes([len(payload) % 255])
    rbsp = b"\x05" + size + payload + b"\x80"
    escaped = bytearray()
    zeros = 0
    for value in rbsp:
        if zeros >= 2 and value <= 3:
            escaped.append(3)
            zeros = 0
        escaped.append(value)
        zeros = zeros + 1 if value == 0 else 0
    return b"\x06" + bytes(escaped)


def sample(nal: bytes) -> bytes:
    return b"\x00\x00\x00\x01" + nal + b"\x00\x00\x00\x01\x65\x01"


class NativeCaptureClockTests(unittest.TestCase):
    def test_stock_metadata_is_integer_precise_and_missing_malformed_refused(self) -> None:
        self.assertEqual(capture_simulation_time(sample(metadata_nal())).publish_sim_time_ns, 123456789)
        self.assertEqual(capture_simulation_time(sample(metadata_nal(0, 2))).frame_num, 2)
        for value in (True, -1, 1.5, "123"):
            with self.subTest(value=value), self.assertRaises(RuntimeError):
                capture_simulation_time(sample(metadata_nal(value)))
        with self.assertRaisesRegex(RuntimeError, "missing"):
            capture_simulation_time(sample(b"\x09\x10"))
        with self.assertRaisesRegex(RuntimeError, "repeated"):
            capture_simulation_time(b"\x00\x00\x00\x01" + metadata_nal() + sample(metadata_nal()))

    def test_metadata_only_marker_correlates_only_same_rtp_picture(self) -> None:
        decoder = H264RtpDepacketizer(96, sequence_parameter_set=b"\x67\x01",
                                      picture_parameter_set=b"\x68\x02")
        metadata = metadata_nal()
        self.assertIsNone(decoder.push(RtpPacket(1, 100, True, 96, metadata)))
        picture = decoder.push(RtpPacket(2, 100, True, 96, b"\x65\x01"))
        self.assertIsNotNone(picture)
        self.assertTrue(picture.is_decoder_reentrant)
        self.assertEqual(capture_simulation_time(picture.sample).publish_sim_time_ns, 123456789)
        self.assertIsNone(decoder.push(RtpPacket(3, 200, True, 96, metadata_nal(frame=2))))
        with self.assertRaisesRegex(RuntimeError, "no picture"):
            decoder.push(RtpPacket(4, 201, True, 96, b"\x41\x01"))

    def test_invalid_escape_and_closed_metadata_shape_refused(self) -> None:
        for nal in (b"\x06\x05\x03\x00\x00\x03",
                    b"\x06\x05\x04\x00\x00\x03\x04\x80"):
            with self.subTest(nal=nal), self.assertRaisesRegex(RuntimeError, "emulation prevention"):
                capture_simulation_time(sample(nal))
        with self.assertRaisesRegex(RuntimeError, "unsupported shape"):
            capture_simulation_time(sample(metadata_nal(unknown=True)))
        with self.assertRaisesRegex(RuntimeError, "size bound"):
            capture_simulation_time(sample(metadata_nal(timestamp_iso8601="x" * 5000)))

    def test_receiver_close_retains_thread_until_bounded_retirement(self) -> None:
        from unittest.mock import Mock, patch
        import threading
        from veoveo_uav_sim.rtsp_h264 import RtspH264Receiver
        receiver = RtspH264Receiver.__new__(RtspH264Receiver)
        receiver._stop = threading.Event()
        receiver._session_lock = threading.RLock()
        receiver._session = Mock()
        receiver._thread = Mock(ident=123)
        receiver._thread.is_alive.side_effect = [True, False]
        with self.assertRaisesRegex(RuntimeError, "within 5 seconds"):
            receiver.close()
        retained = receiver._thread
        receiver.close()
        self.assertIs(receiver._thread, retained)
        self.assertEqual(retained.join.call_count, 2)
        retained.join.assert_called_with(timeout=5.0)
        with patch("threading.current_thread", return_value=retained):
            with self.assertRaisesRegex(RuntimeError, "owning thread"):
                receiver.close()
        self.assertEqual(retained.join.call_count, 2)

    def test_physical_receiver_play_ready_precedes_picture_and_loss_is_terminal(self) -> None:
        from unittest.mock import Mock, patch
        from veoveo_uav_sim.rtsp_h264 import RtspEndpoint, RtspH264Receiver
        failures = []
        receiver = RtspH264Receiver(RtspEndpoint("127.0.0.1", 8554),
                                    lambda _frame: self.fail("unexpected picture"), failures.append)
        session = Mock()
        receiver._session = session
        def interrupted(*_args):
            # Physical drawable callbacks may enqueue their matching pose now,
            # before the first encoded picture is available.
            self.assertTrue(receiver.ready)
            raise EOFError("fixture transport closed")
        session.receive_interleaved.side_effect = interrupted
        with patch("veoveo_uav_sim.rtsp_h264._RtspSession") as replacement:
            receiver._run()
        replacement.assert_not_called()
        session.connect.assert_called_once()
        self.assertEqual(len(failures), 1)
        self.assertIsInstance(failures[0], EOFError)
        self.assertFalse(receiver.ready)

    def test_receiver_admits_pause_but_refuses_same_transport_clock_reset(self) -> None:
        import struct
        from unittest.mock import Mock
        from veoveo_uav_sim.rtsp_h264 import RtspEndpoint, RtspH264Receiver
        def packet(sequence: int, time_ns: int, frame_num: int) -> bytes:
            nals = (metadata_nal(time_ns, frame_num), b"\x65\x01")
            payload = b"\x78" + b"".join(struct.pack("!H", len(nal)) + nal for nal in nals)
            return struct.pack("!BBHII", 0x80, 0xE0, sequence, sequence * 100, 1) + payload
        for second_time, second_frame, accepted in ((100, 2, True), (99, 2, False), (100, 1, False)):
            with self.subTest(time=second_time, frame=second_frame):
                frames = []
                failures = []
                receiver = RtspH264Receiver(RtspEndpoint("127.0.0.1", 8554),
                                           lambda frame: frames.append(frame), failures.append)
                session = Mock()
                session.connect.return_value = H264RtpDepacketizer(
                    96, sequence_parameter_set=b"\x67\x01", picture_parameter_set=b"\x68\x02")
                packets = iter((packet(1, 100, 1), packet(2, second_time, second_frame)))
                def receive(*_args):
                    try:
                        return next(packets)
                    except StopIteration:
                        receiver._stop.set()
                        return b""
                session.receive_interleaved.side_effect = receive
                receiver._session = session
                receiver._run()
                self.assertEqual(len(frames), 2 if accepted else 1)
                self.assertEqual(len(failures), 0 if accepted else 1)
                if failures:
                    self.assertIn("generation without reconnect", str(failures[0]))
                session.close.assert_called_once()


class NativeWriterAcquisitionTests(unittest.TestCase):
    def test_attach_failure_retains_exact_writer_when_detach_cannot_finish(self) -> None:
        from contextlib import nullcontext
        from types import SimpleNamespace
        from unittest.mock import Mock, patch
        from veoveo_uav_sim.native_rtsp import attach_native_rtsp_writer
        from veoveo_uav_sim.hydra_camera import RetainedProductAcquisitionError
        writer = Mock()
        writer.attach.side_effect = RuntimeError("attach failed after acquisition")
        writer.detach.side_effect = [RuntimeError("detach unresolved"), None]
        stage = Mock()
        factory = Mock(return_value=writer)
        usd = SimpleNamespace(get_context=lambda: SimpleNamespace(get_stage=lambda: stage))
        modules = {
            "omni": SimpleNamespace(usd=usd), "omni.usd": usd,
            "isaacsim.streaming.rtsp": SimpleNamespace(RTSPStreamWriter=factory),
            "isaacsim.streaming.rtsp.impl.render_var_utils": SimpleNamespace(
                ensure_render_var_on_product=lambda *args: (True, None)),
            "pxr": SimpleNamespace(Usd=SimpleNamespace(EditContext=lambda *args: nullcontext())),
        }
        with patch.dict("sys.modules", modules):
            with self.assertRaises(RetainedProductAcquisitionError) as raised:
                attach_native_rtsp_writer("/Render/owned", port=8554, width=32, height=32)
        owner = raised.exception.owner
        self.assertFalse(owner.cleanup_complete)
        with self.assertRaisesRegex(RuntimeError, "detach unresolved"):
            owner.close()
        self.assertTrue(owner.cleanup_complete)
        self.assertEqual(writer.detach.call_count, 2)
        factory.assert_called_once()
        self.assertEqual(factory.call_args.kwargs["encoding"], "h264")
