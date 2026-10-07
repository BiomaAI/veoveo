import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[4]))
from testing.python.protocol_schema import assert_peer_snapshot

"""Private decode tests; this suite performs no GPU inference."""
import unittest
import json
import copy
from hashlib import sha256
from contextlib import nullcontext
from tempfile import TemporaryDirectory
from types import SimpleNamespace
from unittest.mock import Mock, patch
from pydantic import ValidationError
from speech_runner.protocol import REQUEST_ADAPTER, EVENT_ADAPTER, PROTOCOL, Request, encode, provider_transcript
from pathlib import Path


def checkpoint_fixture(root):
    from speech_runner.cache_model import CheckpointFile, REVISION

    snapshot = root / "hub" / "models--moondream--parakeet-ultra" / "snapshots" / REVISION
    snapshot.mkdir(parents=True)
    files = []
    for name, payload in [
        ("config.json", b'{"fixture":true}'),
        ("tokenizer.json", b'{"tokens":[]}'),
        ("model.safetensors", b"isolated-checkpoint-bytes"),
    ]:
        (snapshot / name).write_bytes(payload)
        files.append(CheckpointFile(name, len(payload), sha256(payload).hexdigest()))
    return snapshot, tuple(files)


class CheckpointAdmissionTests(unittest.TestCase):
    def test_exact_snapshot_resolution_and_shared_blob_admission(self):
        from speech_runner import cache_model

        for layout in ("repository", "shared_xet_backed"):
            with self.subTest(layout=layout), TemporaryDirectory() as directory:
                snapshot, files = checkpoint_fixture(Path(directory))
                cache = snapshot.parents[2]
                blobs = snapshot.parent.parent / "blobs" if layout == "repository" else cache / "blobs" / "5d"
                blobs.mkdir(parents=True)
                weights = snapshot / "model.safetensors"
                weights.rename(blobs / "weights")
                weights.symlink_to(blobs / "weights")
                for offline in (False, True):
                    with patch.object(cache_model, "REQUIRED_FILES", files), patch(
                        "huggingface_hub.constants.HF_HUB_CACHE", str(cache)
                    ), patch("huggingface_hub.snapshot_download", return_value=str(snapshot)) as resolve:
                        self.assertEqual(cache_model.verified_checkpoint(local_files_only=offline), snapshot)
                        resolve.assert_called_once_with(
                            "moondream/parakeet-ultra",
                            revision="73175eb7aeb0d82f1e2a6b53b3aabc10a90bcd0b",
                            allow_patterns=["config.json", "tokenizer.json", "model.safetensors"],
                            local_files_only=offline,
                            cache_dir=cache,
                        )

    def test_every_required_file_refuses_missing_wrong_length_and_corrupt_bytes(self):
        from speech_runner import cache_model

        for name in ("config.json", "tokenizer.json", "model.safetensors"):
            for defect in ("missing", "length", "digest"):
                with self.subTest(name=name, defect=defect), TemporaryDirectory() as directory:
                    snapshot, files = checkpoint_fixture(Path(directory))
                    path = snapshot / name
                    blobs = snapshot.parents[2] / "blobs" / "5d"
                    blobs.mkdir(parents=True)
                    path.rename(blobs / "fixture-object")
                    path.symlink_to(blobs / "fixture-object")
                    if defect == "missing":
                        (blobs / "fixture-object").unlink()
                    elif defect == "length":
                        path.write_bytes(path.read_bytes() + b"extra")
                    else:
                        path.write_bytes(b"x" * path.stat().st_size)
                    with patch.object(cache_model, "REQUIRED_FILES", files), patch(
                        "huggingface_hub.constants.HF_HUB_CACHE", str(snapshot.parents[2])
                    ), patch(
                        "huggingface_hub.snapshot_download", return_value=str(snapshot)
                    ), self.assertRaisesRegex(RuntimeError, "^Speech checkpoint admission failed$"):
                        cache_model.verified_checkpoint(local_files_only=True)

    def test_snapshot_identity_blob_confinement_and_resolver_errors_are_redacted(self):
        from speech_runner import cache_model

        with TemporaryDirectory(prefix="sentinel-private-path-") as directory:
            snapshot, files = checkpoint_fixture(Path(directory))
            config = snapshot / "config.json"
            outside = Path(directory) / "outside-config"
            config.rename(outside)
            config.symlink_to(outside)
            wrong_revision = snapshot.with_name("unsupported-revision")
            wrong_revision.mkdir()
            wrong_repository = snapshot.parents[2] / "models--unrelated--model" / "snapshots" / cache_model.REVISION
            wrong_repository.mkdir(parents=True)
            for options in (
                {"return_value": str(snapshot)},
                {"return_value": str(wrong_revision)},
                {"return_value": str(wrong_repository)},
                {"side_effect": OSError("sentinel-private-path credential-value")},
            ):
                with patch.object(cache_model, "REQUIRED_FILES", files), patch(
                    "huggingface_hub.constants.HF_HUB_CACHE", str(snapshot.parents[2])
                ), patch(
                    "huggingface_hub.snapshot_download", **options
                ), self.assertRaises(RuntimeError) as raised:
                    cache_model.verified_checkpoint(local_files_only=True)
                self.assertEqual(str(raised.exception), "Speech checkpoint admission failed")
                self.assertTrue(raised.exception.__suppress_context__)


class ProtocolTests(unittest.TestCase):
    def test_invalid_discriminators_are_redacted_at_worker_boundaries(self):
        for action in [
            lambda: Request.parse(b'{"operation":"sentinel-private-value"}', Path("/tmp")),
            lambda: encode({"kind": "sentinel-private-value"}),
        ]:
            with self.assertRaises(ValueError) as raised:
                action()
            self.assertNotIn("sentinel-private-value", str(raised.exception))
            self.assertIn("union_tag_invalid", str(raised.exception))

    def test_worker_error_does_not_reflect_input(self):
        for adapter, value in [
            (REQUEST_ADAPTER, {"operation": "probe", "secret": "sentinel-private-value"}),
            (EVENT_ADAPTER, {"kind": "accepted", "secret": "sentinel-private-value"}),
            (REQUEST_ADAPTER, {"secret": "sentinel-private-value"}),
            (EVENT_ADAPTER, {"secret": "sentinel-private-value"}),
        ]:
            with self.assertRaises(ValidationError) as raised:
                adapter.validate_python(value)
            self.assertNotIn("sentinel-private-value", str(raised.exception))

    def test_empty_tagged_variants_are_closed(self):
        for adapter, value in [
            (REQUEST_ADAPTER, {"operation": "probe"}),
            (EVENT_ADAPTER, {"kind": "accepted"}),
        ]:
            adapter.validate_python(value)
            with self.assertRaises(ValidationError):
                adapter.validate_python({**value, "unexpected": True})

    def test_transcript_events_reject_nested_additions(self):
        value = {"kind": "transcript", "complete": True, "transcript": {
            "text": "hello", "durationSeconds": 1.0, "segments": [
                {"text": "hello", "start": 0.0, "end": 1.0, "words": [
                    {"word": "hello", "start": 0.0, "end": 1.0}
                ]}
            ]
        }}
        self.assertTrue(encode(value).endswith(b"\n"))
        value["transcript"]["segments"][0]["words"][0]["unexpected"] = True
        with self.assertRaises(ValueError):
            encode(value)

    def test_request_and_event_schemas_close_every_owned_object(self):
        for adapter in (REQUEST_ADAPTER, EVENT_ADAPTER):
            for definition in adapter.json_schema()["$defs"].values():
                if definition.get("type") == "object":
                    self.assertFalse(definition["additionalProperties"])


class PrivateProtocolSchemaTests(unittest.TestCase):
    def test_complete_reachable_private_protocol_schema(self):
        assert_peer_snapshot(
            Path(__file__).resolve().parents[2] / "testdata/private-protocol.schema.json",
            REQUEST_ADAPTER.json_schema(mode="validation"),
            EVENT_ADAPTER.json_schema(mode="serialization"),
        )


class NumericAdmissionTests(unittest.TestCase):
    def test_request_width_and_transcript_finiteness(self):
        for value in [-1, 2**32, True]:
            with self.assertRaises(ValidationError):
                REQUEST_ADAPTER.validate_python({"operation": "live", "sampleRate": value, "maxDurationSeconds": 1})
        for value in [float("nan"), float("inf"), -float("inf")]:
            with self.assertRaises(ValueError):
                encode({"kind": "transcript", "complete": True, "transcript": {
                    "text": "observed", "durationSeconds": value, "segments": []}})
            with self.assertRaises(ValueError):
                encode({"kind": "transcript", "complete": True, "transcript": {
                    "text": "observed", "durationSeconds": 1.0, "segments": [{
                        "text": "observed", "start": 0.0, "end": 1.0, "words": [{
                            "word": "observed", "start": value, "end": 1.0}]}]}})


class NamingAdmissionTests(unittest.TestCase):
    def test_current_requests_admit_one_spelling_on_both_paths(self):
        current = {"operation": "live", "sampleRate": 16000, "maxDurationSeconds": 1}
        for encoded in (False, True):
            validate = REQUEST_ADAPTER.validate_json if encoded else REQUEST_ADAPTER.validate_python
            wrap = json.dumps if encoded else lambda value: value
            self.assertEqual(validate(wrap(current)).sampleRate, 16000)
            for invalid in [
                {"operation": "live", "sample_rate": 16000, "max_duration_seconds": 1},
                {**current, "sample_rate": 16000},
                {**current, "max_duration_seconds": 1},
            ]:
                with self.assertRaises(ValidationError):
                    validate(wrap(invalid))
        self.assertEqual(Request.parse(json.dumps(current).encode(), Path("/tmp")).sample_rate, 16000)

    def test_provider_adapter_emits_current_owned_keys(self):
        upstream = {"text": "hello", "duration_seconds": 1.0, "segments": [], "diagnostic": "private"}
        product = provider_transcript(upstream, 120)
        self.assertEqual(product, {"text": "hello", "durationSeconds": 1.0, "segments": []})
        event = {"kind": "transcript", "complete": True, "transcript": product}
        self.assertEqual(json.loads(encode(event)), event)
        for encoded in (False, True):
            validate = EVENT_ADAPTER.validate_json if encoded else EVENT_ADAPTER.validate_python
            wrap = json.dumps if encoded else lambda value: value
            validate(wrap(event))
            for mixed in (False, True):
                retired = copy.deepcopy(event)
                retired["transcript"]["duration_seconds"] = 1.0
                if not mixed:
                    del retired["transcript"]["durationSeconds"]
                with self.assertRaises(ValidationError):
                    validate(wrap(retired))


class WorkerAdmissionTests(unittest.IsolatedAsyncioTestCase):
    async def test_verified_checkpoint_reaches_released_provider_model_path(self):
        from speech_runner import cache_model
        from speech_runner.main import serve

        with TemporaryDirectory() as directory:
            root = Path(directory)
            snapshot, files = checkpoint_fixture(root)
            model = SimpleNamespace(transcribe=Mock(side_effect=RuntimeError("fixture warmup stop")))
            photon = Mock(return_value=nullcontext(model))
            modules = {
                "numpy": SimpleNamespace(zeros=Mock(return_value=object()), float32=object()),
                "torch": SimpleNamespace(cuda=SimpleNamespace(is_available=lambda: True)),
                "moondream": SimpleNamespace(photon=photon),
            }
            args = SimpleNamespace(work_dir=root, socket=root / "worker.sock", capacity=2)
            with patch.dict(sys.modules, modules), patch.dict("os.environ", {}, clear=True), patch(
                "speech_runner.main.os.umask"
            ), patch.object(cache_model, "REQUIRED_FILES", files), patch(
                "huggingface_hub.constants.HF_HUB_CACHE", str(snapshot.parents[2])
            ), patch(
                "huggingface_hub.snapshot_download", return_value=str(snapshot)
            ):
                with self.assertRaisesRegex(RuntimeError, "^fixture warmup stop$"):
                    await serve(args)
                photon.assert_called_once_with(
                    "moondream/parakeet-ultra", device="cuda", model_path=snapshot,
                    single_pass_batch_capacity=2,
                )
                self.assertFalse(args.socket.exists())

    async def test_failed_checkpoint_admission_prevents_provider_load_and_socket_readiness(self):
        from speech_runner import cache_model
        from speech_runner.main import serve

        for defect in (
            "missing", "corrupt", "escaped", "blob_store_escaped",
            "optional_manifest", "optional_link", "optional_dangling",
            "undeclared_file", "undeclared_directory",
        ):
            with self.subTest(defect=defect), TemporaryDirectory() as directory:
                root = Path(directory)
                snapshot, files = checkpoint_fixture(root)
                weights = snapshot / "model.safetensors"
                blobs = snapshot.parents[2] / "blobs" / "5d"
                blobs.mkdir(parents=True)
                weights.rename(blobs / "fixture-object")
                weights.symlink_to(blobs / "fixture-object")
                if defect == "missing":
                    (blobs / "fixture-object").unlink()
                elif defect == "corrupt":
                    weights.write_bytes(b"x" * weights.stat().st_size)
                elif defect == "escaped":
                    outside = root / "outside-weights"
                    (blobs / "fixture-object").rename(outside)
                    weights.unlink()
                    weights.symlink_to(outside)
                elif defect == "blob_store_escaped":
                    outside = root / "outside-blobs"
                    blobs.parent.rename(outside)
                    blobs.parent.symlink_to(outside, target_is_directory=True)
                elif defect == "optional_manifest":
                    (snapshot / "ternary.json").write_text('{"sentinel-private-value":true}')
                elif defect == "optional_link":
                    outside = root / "outside-ternary.json"
                    outside.write_text('{"sentinel-private-value":true}')
                    (snapshot / "ternary.json").symlink_to(outside)
                elif defect == "optional_dangling":
                    (snapshot / "ternary.json").symlink_to(root / "missing-ternary.json")
                elif defect == "undeclared_file":
                    (snapshot / "unsupported-provider-input.json").write_text('{"sentinel-private-value":true}')
                else:
                    (snapshot / "unsupported-provider-inputs").mkdir()
                photon = Mock(side_effect=AssertionError("unadmitted checkpoint reached provider"))
                modules = {
                    "numpy": SimpleNamespace(),
                    "torch": SimpleNamespace(cuda=SimpleNamespace(is_available=lambda: True)),
                    "moondream": SimpleNamespace(photon=photon),
                }
                args = SimpleNamespace(work_dir=root, socket=root / "worker.sock", capacity=1)
                with patch.dict(sys.modules, modules), patch.dict("os.environ", {}, clear=True), patch(
                    "speech_runner.main.os.umask"
                ), patch.object(
                    cache_model, "REQUIRED_FILES", files
                ), patch("huggingface_hub.constants.HF_HUB_CACHE", str(snapshot.parents[2])), patch(
                    "huggingface_hub.snapshot_download", return_value=str(snapshot)
                ) as resolve:
                    with self.assertRaisesRegex(RuntimeError, "^Speech checkpoint admission failed$"):
                        await serve(args)
                    self.assertTrue(resolve.call_args.kwargs["local_files_only"])
                    photon.assert_not_called()
                    self.assertFalse(args.socket.exists())

    async def test_retired_and_mixed_names_refuse_before_worker_effects(self):
        from speech_runner.main import Worker

        class Reader:
            def __init__(self, value):
                self.value = json.dumps(value).encode() + b"\n"
            async def readline(self):
                return self.value

        class Writer:
            def __init__(self):
                self.events = []
            def write(self, value):
                self.events.append(json.loads(value))
            async def drain(self):
                pass
            def close(self):
                pass
            async def wait_closed(self):
                pass

        class Model:
            async def atranscribe(self, **options):
                raise AssertionError("unadmitted request reached inference")

        worker = Worker(Model(), Path("/tmp"), "cuda:NVIDIA fixture", 1)
        current = {"operation": "live", "sampleRate": 16000, "maxDurationSeconds": 1}
        for value in [
            {"operation": "live", "sample_rate": 16000, "max_duration_seconds": 1},
            {**current, "sample_rate": 16000},
            {**current, "unknown": "private"},
        ]:
            writer = Writer()
            await worker.handle(Reader(value), writer)
            self.assertEqual(writer.events, [{"kind": "error", "code": "invalid_input"}])
            self.assertEqual(worker.active, 0)
        writer = Writer()
        await worker.handle(Reader({"operation": "probe"}), writer)
        self.assertEqual(writer.events[0]["protocol"], PROTOCOL)
        self.assertEqual(writer.events[0]["kind"], "ready")
