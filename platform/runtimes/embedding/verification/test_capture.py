"""Pure receiver controls using the current Rust-produced collector fixture."""
import copy
import argparse
from dataclasses import asdict, replace
from contextlib import contextmanager
import hashlib
import importlib
import sys
import types
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import MagicMock, patch, sentinel

from pydantic import TypeAdapter, ValidationError

from capture import Capture, Space, compact, read_capture

URI_FIXTURE = Path(__file__).resolve().parents[4] / "servers/knowledge-mcp/testdata/embedding-collector-uri-cases.json"

FIXTURE = Path(__file__).resolve().parents[4] / "servers/knowledge-mcp/testdata/embedding-candidate-capture.json"


@contextmanager
def mocked_reference():
    torch = MagicMock()
    attention = types.ModuleType("torch.nn.attention")
    attention.SDPBackend = MagicMock()
    attention.sdpa_kernel = MagicMock()
    transformers = types.ModuleType("transformers")
    transformers.AutoModel = MagicMock()
    transformers.AutoTokenizer = MagicMock()
    modules = {"torch": torch, "torch.nn": MagicMock(), "torch.nn.functional": MagicMock(),
               "torch.nn.attention": attention, "transformers": transformers}
    try:
        with patch.dict(sys.modules, modules):
            yield importlib.import_module("reference"), torch, transformers
    finally:
        sys.modules.pop("reference", None)


class CaptureAdmission(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.raw = FIXTURE.read_bytes()
        cls.current = json.loads(cls.raw)

    def receivers(self):
        adapter = TypeAdapter(Capture)
        return (
            Capture.model_validate,
            lambda value: Capture.model_validate_json(json.dumps(value, allow_nan=True)),
            adapter.validate_python,
            lambda value: adapter.validate_json(json.dumps(value, allow_nan=True)),
        )

    def refuse(self, value, branch):
        for index, decode in enumerate(self.receivers()):
            with self.subTest(branch=branch, receiver=index):
                with self.assertRaises(ValidationError):
                    decode(value)

    def test_current_rust_capture_admitted_by_all_receivers_and_raw_bytes_retained(self):
        for decode in self.receivers():
            admitted = decode(self.current)
            self.assertEqual(len(admitted.chunks[0].values), admitted.profile.contents.space.dimension)
        raw, admitted = read_capture(FIXTURE)
        self.assertEqual(raw, self.raw)
        self.assertEqual(admitted.profile.id, self.current["profile"]["id"])

    def test_complete_known_graph_refuses_unknown_and_retired_mixed_keys(self):
        def objects(value, path=()):
            if isinstance(value, dict):
                yield path, value
                for key, child in value.items():
                    yield from objects(child, path + (key,))
            elif isinstance(value, list):
                for index, child in enumerate(value):
                    yield from objects(child, path + (index,))
        for path, node in objects(self.current):
            for key in [None, *[key for key in node if any(c.isupper() for c in key)]]:
                for mixed in (False, True) if key else (True,):
                    bad = copy.deepcopy(self.current)
                    target = bad
                    for part in path:
                        target = target[part]
                    if key is None:
                        target["unexpected"] = 1
                    else:
                        old = "".join("_" + c.lower() if c.isupper() else c for c in key)
                        target[old] = target[key] if mixed else target.pop(key)
                    self.refuse(bad, (path, key, mixed))

    def test_selection_dimension_and_scalar_negatives_precede_collection(self):
        mutations = (
            ("chunk parent", lambda value: value["chunks"][0].update(member=len(value["members"]))),
            ("bool parent", lambda value: value["chunks"][0].update(member=True)),
            ("dimension", lambda value: value["chunks"][0]["values"].pop()),
            ("nonfinite", lambda value: value["chunks"][0]["values"].__setitem__(0, float("nan"))),
            ("normalization", lambda value: value["chunks"][0]["values"].__setitem__(0, .25)),
            ("duplicate selected", lambda value: value["queries"][0]["selectedMembers"].append(value["queries"][0]["selectedMembers"][0])),
            ("foreign relevant", lambda value: value["queries"][0].update(relevantMembers=[len(value["members"])])),
            ("bool relevant", lambda value: value["queries"][0].update(relevantMembers=[True])),
            ("repeated member", lambda value: value["members"].append(copy.deepcopy(value["members"][0]))),
            ("repeated query", lambda value: value["queries"].append(copy.deepcopy(value["queries"][0]))),
            ("profile identity", lambda value: value["profile"].update(id="sha256:" + "0" * 64)),
            ("task identity", lambda value: value.update(queryTaskRevision="sha256:" + "0" * 64)),
            ("chunk identity", lambda value: value.update(chunkingRevision="sha256:" + "0" * 64)),
            ("format", lambda value: value.update(format="veoveo.ai/embedding-candidate-capture/v2")),
        )
        for name, mutate in mutations:
            bad = copy.deepcopy(self.current)
            mutate(bad)
            self.refuse(bad, name)


    def test_actual_collector_rejects_invalid_capture_before_cuda_or_outputs(self):
        torch = MagicMock()
        attention = types.ModuleType("torch.nn.attention")
        attention.SDPBackend = MagicMock()
        attention.sdpa_kernel = MagicMock()
        transformers = types.ModuleType("transformers")
        transformers.AutoModel = MagicMock()
        transformers.AutoTokenizer = MagicMock()
        modules = {"torch": torch, "torch.nn": MagicMock(), "torch.nn.functional": MagicMock(),
                   "torch.nn.attention": attention, "transformers": transformers}
        with patch.dict(sys.modules, modules):
            reference = importlib.import_module("reference")
            with tempfile.TemporaryDirectory() as directory:
                directory = Path(directory)
                capture = directory / "capture.json"
                args = argparse.Namespace(candidate_capture=capture, manifest=directory / "missing-manifest",
                                          checkpoint=directory / "missing-checkpoint", output=directory / "vectors.json",
                                          retrieval_report=directory / "retrieval.json")
                from capture import Space
                space = Space.model_validate(self.current["profile"]["contents"]["space"])
                for branch in ("foreign parent", "mixed field"):
                    bad = copy.deepcopy(self.current)
                    if branch == "foreign parent":
                        bad["chunks"][0]["member"] = len(bad["members"])
                    else:
                        bad["queries"][0]["selected_members"] = bad["queries"][0]["selectedMembers"]
                    capture.write_text(json.dumps(bad))
                    with self.assertRaisesRegex(RuntimeError, "invalid current candidate capture"):
                        reference.measure_candidate(args, space)
                    self.assertFalse(args.output.exists())
                    self.assertFalse(args.retrieval_report.exists())
                torch.cuda.is_available.assert_not_called()
                transformers.AutoModel.from_pretrained.assert_not_called()
                transformers.AutoTokenizer.from_pretrained.assert_not_called()
        sys.modules.pop("reference", None)


    def test_vector_diagnostics_are_bounded_redacted_and_hash_actual_inputs(self):
        torch = MagicMock()
        attention = types.ModuleType("torch.nn.attention")
        attention.SDPBackend = MagicMock()
        attention.sdpa_kernel = MagicMock()
        transformers = types.ModuleType("transformers")
        transformers.AutoModel = MagicMock()
        transformers.AutoTokenizer = MagicMock()
        modules = {"torch": torch, "torch.nn": MagicMock(), "torch.nn.functional": MagicMock(),
                   "torch.nn.attention": attention, "transformers": transformers}
        try:
            with patch.dict(sys.modules, modules):
                reference = importlib.import_module("reference")
                capture = Capture.model_validate(self.current)
                indices = [0, len(capture.chunks)]
                result = reference.vector_diagnostics(capture, indices, [.997611, 1.0], [11, 23], [32, 48])
                document, query = result
                self.assertEqual(document.inputTextSha256, "sha256:" + hashlib.sha256(capture.chunks[0].text.encode()).hexdigest())
                self.assertEqual(document.memberIndex, capture.chunks[0].member)
                self.assertEqual(document.collection, capture.members[document.memberIndex].member.collection)
                self.assertIsNone(document.queryId)
                self.assertEqual(query.kindIndex, 0)
                self.assertEqual(query.queryId, capture.queries[0].id)
                model_input = f"Instruct: {capture.queryTask}\nQuery:{capture.queries[0].text}"
                self.assertEqual(query.inputTextSha256, "sha256:" + hashlib.sha256(model_input.encode()).hexdigest())
                self.assertIsNone(query.memberIndex)
                self.assertEqual((query.tokenLength, query.paddingWidth, query.paddingCount), (23, 48, 25))
                encoded = json.dumps([asdict(item) for item in result], allow_nan=False)
                for body in [model_input, capture.queryTask, capture.chunks[0].text,
                             capture.queries[0].text, capture.members[0].member.uri]:
                    self.assertNotIn(body, encoded)
                for forbidden in ['"values"', '"text"', '"tokens"', '"uri"']:
                    self.assertNotIn(forbidden, encoded)
                self.assertLess(len(encoded), 2048)
                self.assertEqual(len(reference.vector_diagnostics(capture, [0] * 8, [1.] * 8, [1] * 8, [16] * 8)), 8)
                for bad_indices, cosines, lengths, widths in [
                    ([0] * 9, [1.] * 9, [1] * 9, [16] * 9),
                    ([0], [], [1], [16]),
                    ([-1], [1.], [1], [16]),
                    ([len(capture.chunks) + len(capture.queries)], [1.], [1], [16]),
                    ([True], [1.], [1], [16]),
                    ([0], [float("nan")], [1], [16]),
                    ([0], [1.], [17], [16]),
                ]:
                    with self.subTest(indices=bad_indices, lengths=lengths):
                        with self.assertRaisesRegex(RuntimeError, "diagnostic"):
                            reference.vector_diagnostics(capture, bad_indices, cosines, lengths, widths)
                torch.__version__ = "reference-torch-version"
                torch.version.cuda = "reference-cuda-version"
                torch.backends.cudnn.version.return_value = 91300
                torch.cuda.get_device_name.return_value = "NVIDIA reference GPU"
                with patch.object(reference.importlib.metadata, "version", side_effect={
                    "transformers": "reference-transformers-version", "vllm": "reference-vllm-version",
                }.__getitem__):
                    runtime = reference.runtime_diagnostic(capture)
                self.assertEqual(runtime.torch, "reference-torch-version")
                self.assertEqual(runtime.transformers, "reference-transformers-version")
                self.assertEqual(runtime.vllm, "reference-vllm-version")
                self.assertEqual(runtime.cuda, "reference-cuda-version")
                self.assertEqual(runtime.cudnn, 91300)
                self.assertEqual(runtime.candidateRuntimeImage, capture.profile.contents.runtimeImage)
                self.assertEqual(runtime.candidateVllm, capture.profile.contents.vllmVersion)
                self.assertEqual(runtime.candidateDriver, capture.profile.contents.environment.driver)
                torch.cuda.is_available.assert_not_called()
                transformers.AutoModel.from_pretrained.assert_not_called()
                transformers.AutoTokenizer.from_pretrained.assert_not_called()
        finally:
            sys.modules.pop("reference", None)

    def test_selected_document_batch_diagnostic_bounds_format_and_redaction(self):
        with mocked_reference() as (reference, torch, transformers):
            value = copy.deepcopy(self.current)
            contents = value["profile"]["contents"]
            contents["space"]["precision"] = "bfloat16"
            contents["serving"]["precision"] = "bfloat16"
            domain = b"veoveo.ai/embedding-execution-profile/v1"
            body = compact(contents)
            framed = len(domain).to_bytes(8, "big") + domain + len(body).to_bytes(8, "big") + body
            value["profile"]["id"] = "sha256:" + hashlib.sha256(framed).hexdigest()
            capture = Capture.model_validate(value)
            self.assertEqual(reference.selected_document_indices(capture, [15, 16]), (15, 16))
            last = len(capture.chunks) - 1
            tail_start = last // 16 * 16
            for index, expected_start, expected_count in [
                (15, 0, 16), (16, 16, 16), (last, tail_start, len(capture.chunks) - tail_start),
            ]:
                start, rows = reference.original_document_batch(capture, index)
                self.assertEqual((start, len(rows)), (expected_start, expected_count))
                self.assertEqual(rows[index - start], capture.chunks[index])
                self.assertEqual(rows, tuple(capture.chunks[expected_start:expected_start + expected_count]))
            for indices in [None, [], [0, 1, 2], [0, 0], [-1], [len(capture.chunks)], [True]]:
                with self.subTest(indices=indices):
                    with self.assertRaisesRegex(RuntimeError, "distinct admitted document indices"):
                        reference.selected_document_indices(capture, indices)
            runtime = reference.RuntimeDiagnostic("python-fixture", "torch-fixture", "transformers-fixture",
                "vllm-fixture", "cuda-fixture", 1, "NVIDIA fixture", capture.profile.contents.runtimeImage,
                "candidate-vllm-fixture", "driver-fixture", "candidate-cuda-fixture")
            row = reference.DocumentBatchDiagnostic(15,
                "sha256:" + hashlib.sha256(capture.chunks[15].text.encode()).hexdigest(),
                capture.chunks[15].member, capture.members[capture.chunks[15].member].member.collection,
                0, 16, reference.TokenShape(80, 162, 82), reference.TokenShape(80, 80, 0),
                reference.BatchCosines(.997611, .9999, .998), None, None)
            record = reference.BatchDiagnostic("veoveo.ai/embedding-batch-diagnostic/v1",
                "selected_document_batch_comparison", "sha256:" + hashlib.sha256(self.raw).hexdigest(),
                capture.profile.id, capture.profile.contents.checkpointManifest, capture.profile.contents.space,
                "bfloat16", "cudnn_attention", "model_default", runtime, (row,))
            document = reference.batch_diagnostic_document(record)
            self.assertEqual(document["format"], "veoveo.ai/embedding-batch-diagnostic/v1")
            self.assertEqual(document["scope"], "selected_document_batch_comparison")
            self.assertEqual(document["positions"], "model_default")
            self.assertEqual(document["referencePrecision"], document["space"]["precision"])
            fp16_space = record.space.model_copy(update={"precision": "float16"})
            fp16 = reference.batch_diagnostic_document(replace(record, space=fp16_space, referencePrecision="float16"))
            self.assertEqual(fp16["referencePrecision"], "float16")
            self.assertEqual(fp16["space"]["precision"], "float16")
            with self.assertRaisesRegex(RuntimeError, "precision differs"):
                reference.batch_diagnostic_document(replace(record, referencePrecision="float16"))
            self.assertNotIn("passed", document)
            self.assertNotIn("measurement", document)
            self.assertNotIn("minimumCosineMillionths", document)
            encoded = json.dumps(document, allow_nan=False)
            for body in [capture.queryTask, capture.chunks[15].text, capture.members[0].member.uri]:
                self.assertNotIn(body, encoded)
            for forbidden in ['"values"', '"text"', '"tokens"', '"uri"']:
                self.assertNotIn(forbidden, encoded)
            self.assertLess(len(encoded), 4096)
            repeated_row = replace(row, repeatSingleton=reference.TokenShape(80, 80, 0),
                repeatCosines=reference.RepeatCosines(.9999, .998, 1.0))
            repeated = reference.batch_diagnostic_document(replace(record, documents=(repeated_row,)))
            self.assertEqual(repeated["documents"][0]["repeatCosines"]["singletonVsRepeat"], 1.0)
            for rows in [(), (row, row), (row, replace(row, vectorIndex=16), replace(row, vectorIndex=17))]:
                with self.assertRaisesRegex(RuntimeError, "bounded batch diagnostic"):
                    reference.batch_diagnostic_document(replace(record, documents=rows))
            with tempfile.TemporaryDirectory() as directory:
                output = Path(directory) / "diagnostic.json"
                reference.write_new(output, document)
                original = output.read_bytes()
                with self.assertRaises(FileExistsError):
                    reference.write_new(output, repeated)
                self.assertEqual(output.read_bytes(), original)
            torch.cuda.is_available.assert_not_called()
            transformers.AutoModel.from_pretrained.assert_not_called()
            transformers.AutoTokenizer.from_pretrained.assert_not_called()

    def test_document_diagnostic_admission_refuses_before_cuda_or_output(self):
        with mocked_reference() as (reference, torch, transformers), tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            capture_path, manifest = directory / "capture.json", directory / "manifest"
            manifest.write_text("")
            value = copy.deepcopy(self.current)
            contents = value["profile"]["contents"]
            contents["space"].update(model="qwen3-embedding-0.6b", revision=reference.REVISION, dimension=1024, precision="bfloat16")
            contents["serving"]["precision"] = "bfloat16"
            for row in [*value["chunks"], *value["queries"]]:
                row["values"].extend([0.0] * (1024 - len(row["values"])))
            contents["checkpointManifest"] = "sha256:" + hashlib.sha256(manifest.read_bytes()).hexdigest()

            def write_capture(candidate):
                contents = candidate["profile"]["contents"]
                domain = b"veoveo.ai/embedding-execution-profile/v1"
                body = compact(contents)
                framed = len(domain).to_bytes(8, "big") + domain + len(body).to_bytes(8, "big") + body
                candidate["profile"]["id"] = "sha256:" + hashlib.sha256(framed).hexdigest()
                capture_path.write_text(json.dumps(candidate))

            write_capture(value)
            space = Space.model_validate(contents["space"])
            args = argparse.Namespace(candidate_capture=capture_path, manifest=manifest,
                checkpoint=directory / "checkpoint", output=directory / "diagnostic.json",
                retrieval_report=None, document_indices=[0], repeat_singleton=False)
            read_capture(capture_path)  # The pure receiver fixture itself is admitted.
            for indices in [None, [], [0, 1, 2], [0, 0], [-1], [len(value["chunks"])], [True]]:
                args.document_indices = indices
                with self.assertRaisesRegex(RuntimeError, "distinct admitted document indices"):
                    reference.diagnose_documents(args, space)
                self.assertFalse(args.output.exists())
            args.document_indices = [0]
            args.output.write_bytes(b"existing diagnostic")
            with self.assertRaisesRegex(RuntimeError, "diagnostic output already exists"):
                reference.diagnose_documents(args, space)
            self.assertEqual(args.output.read_bytes(), b"existing diagnostic")
            args.output.unlink()
            args.retrieval_report = directory / "acceptance.json"
            with self.assertRaisesRegex(RuntimeError, "no retrieval report"):
                reference.diagnose_documents(args, space)
            self.assertFalse(args.retrieval_report.exists())
            args.retrieval_report = None
            bad = copy.deepcopy(value)
            bad["chunks"][0]["unexpected"] = "private body sentinel"
            write_capture(bad)
            with self.assertRaisesRegex(RuntimeError, "invalid current candidate capture"):
                reference.diagnose_documents(args, space)
            write_capture(value)
            with self.assertRaisesRegex(RuntimeError, "unsupported reference target space"):
                reference.diagnose_documents(args, space.model_copy(update={"revision": "unsupported"}))
            bad = copy.deepcopy(value)
            bad["profile"]["contents"]["checkpointManifest"] = "sha256:" + "0" * 64
            write_capture(bad)
            with self.assertRaisesRegex(RuntimeError, "space/checkpoint"):
                reference.diagnose_documents(args, space)
            # A matching manifest identity cannot conceal mismatching local bytes.
            args.checkpoint.mkdir()
            (args.checkpoint / "weights.bin").write_bytes(b"synthetic checkpoint")
            manifest.write_text("0" * 64 + "  weights.bin\n")
            value["profile"]["contents"]["checkpointManifest"] = "sha256:" + hashlib.sha256(manifest.read_bytes()).hexdigest()
            write_capture(value)
            with self.assertRaisesRegex(RuntimeError, "checkpoint digest mismatch"):
                reference.diagnose_documents(args, space)
            self.assertFalse(args.output.exists())
            torch.cuda.is_available.assert_not_called()
            transformers.AutoModel.from_pretrained.assert_not_called()
            transformers.AutoTokenizer.from_pretrained.assert_not_called()

    def test_reference_supported_precision_selects_actual_dtype_and_closed_metadata(self):
        with mocked_reference() as (reference, torch, transformers):
            torch.bfloat16, torch.float16, torch.float32 = sentinel.bfloat16, sentinel.float16, sentinel.float32
            args = argparse.Namespace(checkpoint=Path("synthetic-checkpoint"))
            for precision, expected in [("bfloat16", sentinel.bfloat16), ("float16", sentinel.float16)]:
                with self.subTest(precision=precision):
                    space = Space.model_validate({**self.current["profile"]["contents"]["space"],
                        "model": "qwen3-embedding-0.6b", "revision": reference.REVISION,
                        "dimension": 1024, "precision": precision})
                    reference.admit_reference_space(space)
                    self.assertEqual(reference.reference_precision(space), precision)
                    self.assertIs(reference.reference_dtype(space), expected)
                    model = MagicMock()
                    model.dtype = expected
                    model.eval.return_value = model
                    model.cuda.return_value = model
                    transformers.AutoModel.from_pretrained.return_value = model
                    _, loaded, metadata_precision = reference.candidate_model(args, space)
                    self.assertIs(loaded, model)
                    self.assertIs(loaded.dtype, expected)
                    self.assertIs(transformers.AutoModel.from_pretrained.call_args.kwargs["dtype"], expected)
                    self.assertEqual(transformers.AutoModel.from_pretrained.call_args.kwargs["attn_implementation"], "sdpa")
                    self.assertEqual(metadata_precision, precision)
                    self.assertEqual(reference.require_model_dtype(loaded, space), metadata_precision)
                    model.assert_not_called()
            torch.cuda.is_available.assert_not_called()

    def test_unsupported_reference_precisions_refuse_before_cuda_model_or_reports(self):
        with mocked_reference() as (reference, torch, transformers), tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            for precision in ["float32", "int8", "int4"]:
                with self.subTest(precision=precision):
                    space = Space.model_validate({**self.current["profile"]["contents"]["space"],
                        "model": "qwen3-embedding-0.6b", "revision": reference.REVISION,
                        "dimension": 1024, "precision": precision})
                    args = argparse.Namespace(checkpoint=directory / "checkpoint", manifest=directory / "absent-manifest",
                        output=directory / "reference.json", retrieval_report=directory / "retrieval.json",
                        candidate_capture=directory / "capture.json", document_indices=[0], repeat_singleton=False)
                    with self.assertRaisesRegex(RuntimeError, "unsupported reference precision"):
                        reference.candidate_model(args, space)
                    with patch.object(reference, "read_capture", return_value=(self.raw, Capture.model_validate(self.current))):
                        with self.assertRaisesRegex(RuntimeError, "unsupported reference precision"):
                            reference.measure_candidate(args, space)
                    with self.assertRaisesRegex(RuntimeError, "unsupported reference precision"):
                        reference.diagnose_documents(args, space)
                    space_path = directory / "space.json"
                    space_path.write_text(space.model_dump_json())
                    argv = ["reference.py", "--checkpoint", str(args.checkpoint), "--manifest", str(args.manifest),
                        "--output", str(args.output), "--space-file", str(space_path)]
                    with patch.object(sys, "argv", argv):
                        with self.assertRaisesRegex(RuntimeError, "unsupported reference precision"):
                            reference.main()
                    self.assertFalse(args.output.exists())
                    self.assertFalse(args.retrieval_report.exists())
            torch.cuda.is_available.assert_not_called()
            transformers.AutoModel.from_pretrained.assert_not_called()
            transformers.AutoTokenizer.from_pretrained.assert_not_called()

    def test_loaded_reference_dtype_mismatch_refuses_before_inference_or_output(self):
        with mocked_reference() as (reference, torch, transformers), tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            torch.bfloat16, torch.float16, torch.float32 = sentinel.bfloat16, sentinel.float16, sentinel.float32
            torch.cuda.is_available.return_value = True
            manifest = directory / "manifest"
            manifest.write_text("")
            for precision in ["bfloat16", "float16"]:
                with self.subTest(precision=precision):
                    space = Space.model_validate({**self.current["profile"]["contents"]["space"],
                        "model": "qwen3-embedding-0.6b", "revision": reference.REVISION,
                        "dimension": 1024, "precision": precision})
                    model = MagicMock()
                    model.dtype = sentinel.float32
                    model.eval.return_value = model
                    model.cuda.return_value = model
                    transformers.AutoModel.from_pretrained.return_value = model
                    args = argparse.Namespace(checkpoint=directory / "checkpoint")
                    with self.assertRaisesRegex(RuntimeError, "model dtype differs"):
                        reference.candidate_model(args, space)
                    space_path, output = directory / "space.json", directory / "reference.json"
                    space_path.write_text(space.model_dump_json())
                    argv = ["reference.py", "--checkpoint", str(args.checkpoint), "--manifest", str(manifest),
                        "--output", str(output), "--space-file", str(space_path)]
                    with patch.object(sys, "argv", argv):
                        with self.assertRaisesRegex(RuntimeError, "model dtype differs"):
                            reference.main()
                    model.assert_not_called()
                    transformers.AutoTokenizer.from_pretrained.return_value.assert_not_called()
                    self.assertFalse(output.exists())

    def test_foundation_produced_uri_cases_match_all_receivers_without_repair(self):
        cases = json.loads(URI_FIXTURE.read_bytes())
        self.assertGreaterEqual(len(cases), 28)
        for case in cases:
            value = copy.deepcopy(self.current)
            value["members"][0]["member"]["uri"] = case["uri"]
            if case["admitted"]:
                for index, decode in enumerate(self.receivers()):
                    with self.subTest(uri=case["uri"], receiver=index):
                        admitted = decode(value)
                        self.assertEqual(admitted.members[0].member.uri, case["uri"])
            else:
                self.refuse(value, ("URI", case["uri"]))

    def test_read_bound_is_checked_on_actual_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "capture.json"
            path.write_bytes(self.raw)
            with patch("capture.MAX_CAPTURE_BYTES", len(self.raw) - 1):
                with self.assertRaisesRegex(RuntimeError, "exceeds"):
                    read_capture(path)
            with patch("capture.MAX_CAPTURE_BYTES", len(self.raw)):
                raw, _ = read_capture(path)
                self.assertEqual(raw, self.raw)


if __name__ == "__main__":
    unittest.main()
