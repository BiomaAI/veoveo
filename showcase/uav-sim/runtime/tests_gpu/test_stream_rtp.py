"""Production RTP publisher and Stream NVDEC/TensorRT runner, without a cluster.

Requires Docker/NVIDIA Container Toolkit, ffprobe, Python 3.13 and these inputs:
VEOVEO_TEST_STREAM_IMAGE (locally cached immutable image), VEOVEO_TEST_STREAM_ENGINE,
VEOVEO_TEST_STREAM_INFERENCE (its matching nvinfer config), VEOVEO_TEST_STREAM_H264,
and VEOVEO_TEST_NVIDIA_GPU_UUID. Config must use /models/primary-detector.engine.
Run this file with a 180-second outer timeout. Each case owns its container, socket
and temporary files; source model and image caches are read-only and preserved.
"""
from __future__ import annotations

import base64
from dataclasses import dataclass, field
import hashlib
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import tempfile
import threading
import time
import unittest
import uuid

from veoveo_uav_sim.config import StreamPublicationConfig
from veoveo_uav_sim.h264 import annex_b_nals
from veoveo_uav_sim.stream_output import RtpH264Publisher

ROOT = Path(__file__).resolve().parents[4]
MAX_EVENT_BYTES = 4 * 1024 * 1024
SENT_FRAMES = 180


@dataclass
class Observation:
    sequences: list[int] = field(default_factory=list)
    timestamps_us: list[int] = field(default_factory=list)
    inference_frames: int = 0
    failures: list[str] = field(default_factory=list)


def receive_events(connection: socket.socket, observed: Observation) -> None:
    try:
        with connection.makefile("rb") as events:
            for _ in range(SENT_FRAMES * 2 + 1):
                line = events.readline(MAX_EVENT_BYTES + 1)
                if not line:
                    return
                if len(line) > MAX_EVENT_BYTES:
                    raise ValueError("native event exceeds its declared byte limit")
                event = json.loads(line)
                match event["schema"]:
                    case "veoveo.stream-live-video-chunk/v1":
                        chunk = event["chunk"]
                        annex_b_nals(base64.b64decode(chunk["data_base64"], validate=True))
                        observed.sequences.append(chunk["sequence"])
                        observed.timestamps_us.append(chunk["timestamp_us"])
                    case "veoveo.stream-live-frame/v1":
                        if event["frame"]["index"] != observed.inference_frames:
                            raise ValueError("GPU inference sequence is not contiguous")
                        observed.inference_frames += 1
                    case _:
                        raise ValueError("unknown native Stream event")
            raise ValueError("native runner exceeded the fixture's event budget")
    except Exception as error:
        observed.failures.append(str(error))


class StreamRtpTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.image = os.environ["VEOVEO_TEST_STREAM_IMAGE"]
        if not re.fullmatch(r"[^\s]+@sha256:[0-9a-f]{64}", cls.image):
            raise ValueError("Stream GPU qualification requires an immutable image digest")
        cls.engine = Path(os.environ["VEOVEO_TEST_STREAM_ENGINE"]).resolve(strict=True)
        cls.inference = Path(os.environ["VEOVEO_TEST_STREAM_INFERENCE"]).resolve(strict=True)
        cls.sample = Path(os.environ["VEOVEO_TEST_STREAM_H264"]).resolve(strict=True)
        cls.gpu = os.environ["VEOVEO_TEST_NVIDIA_GPU_UUID"]
        identity = subprocess.run(
            ["nvidia-smi", "--id", cls.gpu, "--query-gpu=name,uuid,driver_version",
             "--format=csv,noheader"],
            capture_output=True, text=True, timeout=10, check=True,
        ).stdout.strip()
        if "NVIDIA" not in identity or cls.gpu not in identity or "\n" in identity:
            raise ValueError("qualification requires one accessible NVIDIA hardware GPU")
        probe = json.loads(subprocess.run(
            ["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
             "packet=pos,size:stream=width,height", "-of", "json", str(cls.sample)],
            capture_output=True, text=True, timeout=10, check=True,
        ).stdout)
        raw = cls.sample.read_bytes()
        cls.frames = [raw[int(p["pos"]):int(p["pos"]) + int(p["size"])]
                      for p in probe["packets"]]
        if not cls.frames or 5 not in {nal[0] & 31 for nal in annex_b_nals(cls.frames[0])}:
            raise ValueError("the H.264 fixture must start at an IDR")
        cls.width = probe["streams"][0]["width"]
        cls.height = probe["streams"][0]["height"]
        catalog = json.loads((ROOT / "configs/stream/catalog.example.json").read_text())
        cls.pipeline = next(p for p in catalog["pipelines"] if p["id"] == "detect-objects")
        print(json.dumps({
            "suite": "uav-stream-rtp", "image": cls.image, "gpu": identity,
            "engine_sha256": hashlib.sha256(cls.engine.read_bytes()).hexdigest(),
            "sample_sha256": hashlib.sha256(raw).hexdigest(),
        }), flush=True)

    def run_case(self, stalled: bool) -> None:
        name = "veoveo-stream-rtp-test-" + uuid.uuid4().hex
        with tempfile.TemporaryDirectory(prefix="veoveo-stream-rtp-") as directory:
            work = Path(directory)
            # Select a local test port without changing the production packetizer.
            with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as reservation:
                reservation.bind(("127.0.0.1", 0))
                port = reservation.getsockname()[1]
            graph = dict(self.pipeline["live"]["graph"])
            graph["launch"] = graph["launch"].replace("port=9000", f"port={port}")
            request = {
                "schema": "veoveo.stream-live-runner-request/v1",
                "session_id": "019ffdb2-0596-7c91-ac83-0a45b82d7952",
                "input_width": self.width, "input_height": self.height,
                "pipeline": {"pipeline_id": self.pipeline["id"], "graph": graph,
                             "profile": {k: v for k, v in self.pipeline["profile"].items()
                                         if k != "model_id"}},
                "model": {"model_id": "primary-detector", "format": "tensor_rt_engine",
                          "model_path": "/models/primary-detector.engine"},
                "max_detections_per_frame": 10000, "max_event_bytes": MAX_EVENT_BYTES,
                "max_video_chunk_bytes": 1024 * 1024,
            }
            (work / "request.json").write_text(json.dumps(request))
            listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            listener.bind(str(work / "events.sock"))
            listener.listen(1)
            listener.settimeout(30)
            connection = None
            reader = None
            child = None
            observed = Observation()
            log_path = work / "runner.log"
            publisher = RtpH264Publisher(StreamPublicationConfig(
                "127.0.0.1", port, 96, "fixture", 64,
            ))
            try:
                with log_path.open("w") as log:
                    child = subprocess.Popen([
                        "docker", "run", "--rm", "--pull=never", "--name", name,
                        "--gpus", f"device={self.gpu}", "--network", "host",
                        "--user", f"{os.getuid()}:{os.getgid()}", "--memory", "4g",
                        "--cpus", "2", "-v", f"{work}:/work",
                        "-v", f"{self.engine}:/models/primary-detector.engine:ro",
                        "-v", f"{self.inference}:/etc/veoveo/stream/primary-detector.txt:ro",
                        "--entrypoint", "/usr/local/bin/stream-gst-runner", self.image,
                        "--request-json", "/work/request.json",
                        "--event-socket", "/work/events.sock",
                    ], stdout=log, stderr=log)
                    connection, _ = listener.accept()
                    connection.settimeout(15)
                    reader = threading.Thread(
                        target=receive_events, args=(connection, observed), daemon=True,
                    )
                    reader.start()
                    # The private socket connects before the GPU graph starts PLAYING.
                    time.sleep(1)
                    source_time = 600.0
                    for index in range(SENT_FRAMES):
                        self.assertIsNone(child.poll(), "native Stream runner exited")
                        # Delayed delivery followed by physics catch-up reproduces receiver
                        # clock correction without changing or re-encoding the H.264 bytes.
                        wall, step = 1 / 30, 1 / 30
                        if stalled and 30 <= index < 50:
                            wall = 0.2
                        elif stalled and 50 <= index < 70:
                            wall, step = 0.003, 0.2
                        publisher.publish(self.frames[index % len(self.frames)], source_time)
                        source_time += step
                        time.sleep(wall)
                    time.sleep(1)
                    self.assertIsNone(child.poll(), "native Stream runner exited")
                    self.assertFalse(observed.failures, str(observed.failures))
                    self.assertGreaterEqual(len(observed.timestamps_us), 170)
                    self.assertGreaterEqual(observed.inference_frames, 170)
                    self.assertEqual(len(set(observed.timestamps_us)), len(observed.timestamps_us))
                    self.assertEqual(observed.sequences, list(range(len(observed.sequences))))
                    print(json.dumps({
                        "case": "stalls-and-catch-up" if stalled else "steady",
                        "sent": SENT_FRAMES, "preview": len(observed.timestamps_us),
                        "inference": observed.inference_frames, "passed": True,
                    }), flush=True)
            except BaseException:
                if log_path.exists():
                    print(log_path.read_text(errors="replace")[-8192:], flush=True)
                raise
            finally:
                publisher.close()
                subprocess.run(["docker", "rm", "-f", name], capture_output=True, timeout=15)
                if child is not None:
                    child.wait(timeout=15)
                listener.close()
                if connection is not None:
                    connection.close()
                if reader is not None:
                    reader.join(timeout=2)
                    if reader.is_alive():
                        raise RuntimeError("native event reader did not stop after container cleanup")

    def test_steady_rtp_produces_gpu_results_and_unique_preview_timestamps(self) -> None:
        self.run_case(stalled=False)

    def test_stalls_and_catch_up_preserve_gpu_results_and_preview_timestamps(self) -> None:
        self.run_case(stalled=True)


if __name__ == "__main__":
    result = unittest.TextTestRunner(verbosity=2).run(
        unittest.defaultTestLoader.loadTestsFromTestCase(StreamRtpTests)
    )
    print(json.dumps({
        "suite": "uav-stream-rtp", "passed": result.wasSuccessful(),
        "tests": result.testsRun, "failures": len(result.failures), "errors": len(result.errors),
    }))
    raise SystemExit(0 if result.wasSuccessful() else 1)
