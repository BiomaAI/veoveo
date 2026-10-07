import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[4]))
from testing.python.protocol_schema import assert_peer_snapshot

import json
from pathlib import Path
import tempfile
import unittest
import zipfile
from unittest.mock import patch

from map_data.contract import ContractError, NormalizeCommand, NormalizeResult
from map_data.adapters.gtfs import normalize_gtfs
from map_data.adapters.osm import _OSM_CONFIG
from map_data.adapters.raster import raster_metadata
from map_data.terrain import corridor_sample_positions


class NormalizeCommandTests(unittest.TestCase):
    def gtfs_command(self, root: str, source: Path, maximum_output_bytes: int) -> NormalizeCommand:
        return NormalizeCommand.parse(
            {
                "schemaVersion": 2,
                "acquisitionId": "acquisition-019f5cda-8c2d-7283-88c8-a72f4a138a5e",
                "adapterKind": "gtfs_schedule",
                "sourcePath": str(source),
                "outputDir": str(Path(root) / "output"),
                "maximumElapsedSeconds": 10,
                "maximumOutputBytes": maximum_output_bytes,
            }
        )

    def test_helper_model_decoders_refuse_retired_mixed_keys_and_missing_revision(self):
        cases = [
            (NormalizeCommand, {"schemaVersion": 2, "acquisitionId": "acquisition-test",
             "adapterKind": "authority_vector", "sourcePath": "/tmp/source",
             "outputDir": "/tmp/output", "maximumElapsedSeconds": 10,
             "maximumOutputBytes": 1024}),
            (NormalizeResult, {"schemaVersion": 2, "acquisitionId": "acquisition-test",
             "sourceDigestSha256": "a" * 64, "versionLabel": "test", "normalizedPaths": [],
             "qualityReportPath": "/tmp/report.json", "routingBuildPath": None})]
        for model, current in cases:
            for decode in [model.model_validate, lambda value: model.model_validate_json(json.dumps(value))]:
                decode(current)
                missing = {key: value for key, value in current.items() if key != "schemaVersion"}
                with self.assertRaises(ValueError):
                    decode(missing)
                for retired, field in model.model_fields.items():
                    canonical = field.alias or retired
                    if canonical == retired:
                        continue
                    for mixed in [False, True]:
                        bad = {**current, retired: current[canonical]}
                        if not mixed:
                            del bad[canonical]
                        with self.assertRaises(ValueError):
                            decode(bad)

    def test_raster_protocol_refuses_retired_fields_before_directory_or_gdal_effects(self):
        from map_data.raster_ops import run

        with tempfile.TemporaryDirectory() as root:
            source = Path(root) / "source.tif"
            source.write_bytes(b"not-opened-by-negative-controls")
            output = Path(root) / "output"
            current = {"schemaVersion": 2, "sourcePath": str(source),
                       "outputDir": str(output), "maximumOutputBytes": 1024,
                       "operation": {"kind": "corridor_maximum", "band": 1,
                                     "corridor": {"coordinates": [{"longitudeDeg": 0, "latitudeDeg": 0}, {"longitudeDeg": 0.001, "latitudeDeg": 0.001}]},
                                     "sampleSpacing": 10, "halfWidth": 2,
                                     "crossTrackSamples": 3}}
            for canonical, retired in [("schemaVersion", "schema_version"),
                                       ("sourcePath", "source_path"),
                                       ("outputDir", "output_dir"),
                                       ("maximumOutputBytes", "maximum_output_bytes")]:
                for mixed in [False, True]:
                    bad = {**current, retired: current[canonical]}
                    if not mixed:
                        del bad[canonical]
                    with self.assertRaises(ContractError):
                        run(bad)
                    self.assertFalse(output.exists())
            for canonical, retired in [("sampleSpacing", "sample_spacing"),
                                       ("halfWidth", "half_width"),
                                       ("crossTrackSamples", "cross_track_samples")]:
                for mixed in [False, True]:
                    operation = {**current["operation"], retired: current["operation"][canonical]}
                    if not mixed:
                        del operation[canonical]
                    with self.assertRaises(ContractError):
                        run({**current, "operation": operation})
                    self.assertFalse(output.exists())
            for version in [1, 3]:
                with self.assertRaises(ContractError):
                    run({**current, "schemaVersion": version})
                self.assertFalse(output.exists())

    def test_nested_raster_admission_precedes_directory_and_gdal(self):
        from copy import deepcopy
        from map_data.raster_ops import run, admit_operation
        position = {"longitudeDeg": 0, "latitudeDeg": 0, "ellipsoidalHeightM": 10}
        cases = [
            ({"kind": "sample", "band": 1, "positions": [position]}, ["positions", 0]),
            ({"kind": "corridor_maximum", "band": 1,
              "corridor": {"coordinates": [position, {"longitudeDeg": .001, "latitudeDeg": .001}]},
              "sampleSpacing": 10, "halfWidth": 2, "crossTrackSamples": 3}, ["corridor", "coordinates", 0]),
            ({"kind": "window", "bounds": {"west": 0, "south": 0, "east": 1, "north": 1}, "width": 10, "height": 10}, ["bounds"]),
        ]
        with tempfile.TemporaryDirectory() as root:
            source = Path(root) / "source.tif"
            source.write_bytes(b"negative-admission-never-opens-this")
            output = Path(root) / "output"
            for current, path in cases:
                admit_operation(current, set(current) - {"kind"})
                mutations = []
                nested = current
                for key in path:
                    nested = nested[key]
                for key in nested:
                    mutations.append((key, None, "missing"))
                    if key in {"longitudeDeg", "latitudeDeg", "ellipsoidalHeightM"}:
                        old = {"longitudeDeg": "longitude_deg", "latitudeDeg": "latitude_deg", "ellipsoidalHeightM": "ellipsoidal_height_m"}[key]
                        mutations.extend([(key, old, "replacement"), (key, old, "mixed")])
                mutations.append((None, "unknown", "mixed"))
                for key, retired, mode in mutations:
                    if key == "ellipsoidalHeightM" and mode == "missing":
                        continue
                    bad = deepcopy(current)
                    target = bad
                    for segment in path:
                        target = target[segment]
                    if retired is not None:
                        target[retired] = target[key] if key is not None else True
                    if key is not None and mode in {"missing", "replacement"}:
                        del target[key]
                    with patch("map_data.raster_ops.gdal.Open") as opened:
                        with self.assertRaises(ContractError):
                            run({"schemaVersion": 2, "sourcePath": str(source), "outputDir": str(output), "maximumOutputBytes": 1024, "operation": bad})
                        opened.assert_not_called()
                    self.assertFalse(output.exists())
            bad = deepcopy(cases[1][0]); bad["corridor"]["unknown"] = True
            with patch("map_data.raster_ops.gdal.Open") as opened:
                with self.assertRaises(ContractError):
                    run({"schemaVersion": 2, "sourcePath": str(source), "outputDir": str(output), "maximumOutputBytes": 1024, "operation": bad})
                opened.assert_not_called()
            self.assertFalse(output.exists())

    def test_actual_quality_writer_and_reader_refuse_retired_and_malformed_objects(self):
        from copy import deepcopy
        from pydantic import ValidationError
        from map_data.adapters.common import write_quality_report
        from map_data.contract import QualityReport
        from map_data.main import run
        with tempfile.TemporaryDirectory() as root:
            source = Path(root) / "source.zip"; source.write_bytes(b"controlled-source")
            command = self.gtfs_command(root, source, 1024)
            report = write_quality_report(command, adapter="gtfs_schedule", checks=[{"name": "required_files_present", "passed": True}])
            current = json.loads(report.read_text())
            QualityReport.model_validate_json(report.read_text())
            mutations = []
            for key, retired in [("schemaVersion", "schema_version"), ("acquisitionId", "acquisition_id")]:
                for mixed in [False, True]:
                    bad = deepcopy(current); bad[retired] = bad[key]
                    if not mixed: del bad[key]
                    mutations.append(bad)
            for path, key, value in [(None, "unknown", True), ("checks", "unknown", True), (None, "schemaVersion", 1), (None, "passed", 1), ("checks", "passed", "true"), ("checks", "name", 12)]:
                bad = deepcopy(current)
                target = bad["checks"][0] if path else bad
                target[key] = value; mutations.append(bad)
            for bad in mutations:
                for decode in [QualityReport.model_validate, lambda value: QualityReport.model_validate_json(json.dumps(value))]:
                    with self.assertRaises(ValidationError): decode(bad)
                report.write_text(json.dumps(bad))
                with patch.dict("map_data.main.ADAPTERS", {"gtfs_schedule": lambda _: ((source,), report, None)}), patch("map_data.main.enforce_output_limit") as publish:
                    with self.assertRaises(ValidationError): run(command.model_dump(mode="json", by_alias=True))
                    publish.assert_not_called()
            report.write_text(json.dumps(current))
            with patch.dict("map_data.main.ADAPTERS", {"gtfs_schedule": lambda _: ((source,), report, None)}):
                self.assertEqual(run(command.model_dump(mode="json", by_alias=True)).acquisition_id, command.acquisition_id)
            report.write_text(json.dumps({**current, "passed": False, "checks": [{"name": "required_files_present", "passed": False}]}))
            with patch.dict("map_data.main.ADAPTERS", {"gtfs_schedule": lambda _: ((source,), report, None)}):
                with self.assertRaises(ContractError): run(command.model_dump(mode="json", by_alias=True))

    def test_current_command_refuses_retired_keys_and_revision_before_directory_effect(self):
        with tempfile.TemporaryDirectory() as root:
            source = Path(root) / "source.json"
            source.write_text("{}")
            output = Path(root) / "output"
            current = {"schemaVersion":2, "acquisitionId":"acquisition-fixture",
                "adapterKind":"authority_vector", "sourcePath":str(source),
                "outputDir":str(output), "maximumElapsedSeconds":1, "maximumOutputBytes":1}
            for old, key in [("schema_version","schemaVersion"), ("acquisition_id","acquisitionId"),
                ("adapter_kind","adapterKind"), ("source_path","sourcePath"),
                ("output_dir","outputDir"), ("maximum_elapsed_seconds","maximumElapsedSeconds"),
                ("maximum_output_bytes","maximumOutputBytes")]:
                for mixed in (False, True):
                    invalid = {**current, old:current[key]}
                    if not mixed:
                        del invalid[key]
                    with self.assertRaises(ContractError):
                        NormalizeCommand.parse(invalid)
                    self.assertFalse(output.exists())
            for revision in (1, 3):
                with self.assertRaises(ContractError):
                    NormalizeCommand.parse({**current, "schemaVersion":revision})
                self.assertFalse(output.exists())
            self.assertEqual(NormalizeCommand.parse(current).schema_version, 2)
            self.assertTrue(output.is_dir())

    def test_helper_error_does_not_reflect_unknown_field_value(self):
        with self.assertRaises(ContractError) as raised:
            NormalizeCommand.parse({
                "schemaVersion": 2, "acquisitionId": "acquisition-test",
                "adapterKind": "gtfs_schedule", "sourcePath": "/tmp/source",
                "outputDir": "/tmp/output", "maximumElapsedSeconds": 1,
                "maximumOutputBytes": 1, "secret": "sentinel-private-value",
            })
        self.assertNotIn("sentinel-private-value", str(raised.exception))

    def test_helper_models_close_owned_wire_fields(self):
        from pydantic import ValidationError
        self.assertFalse(NormalizeCommand.model_json_schema()["additionalProperties"])
        self.assertFalse(NormalizeResult.model_json_schema()["additionalProperties"])
        with self.assertRaises(ValidationError):
            NormalizeResult(
                schemaVersion=2,
                acquisitionId="acquisition-test", sourceDigestSha256="a" * 64,
                versionLabel="fixture", normalizedPaths=(),
                qualityReportPath=Path("/tmp/quality.json"), routingBuildPath=None,
                unexpected=True,
            )

    def test_rejects_relative_and_missing_source_paths(self):
        with self.assertRaises(ContractError):
            NormalizeCommand.parse(
                {
                    "schemaVersion": 2,
                    "acquisitionId": "acquisition-test",
                    "adapterKind": "authority_vector",
                    "sourcePath": "relative.json",
                    "outputDir": "/tmp/output",
                    "maximumElapsedSeconds": 10,
                    "maximumOutputBytes": 1024,
                }
            )

    def test_accepts_confined_absolute_paths(self):
        with tempfile.TemporaryDirectory() as root:
            source = Path(root) / "input.geojson"
            source.write_text(json.dumps({"type": "FeatureCollection", "features": []}))
            command = NormalizeCommand.parse(
                {
                    "schemaVersion": 2,
                    "acquisitionId": "acquisition-019f5cda-8c2d-7283-88c8-a72f4a138a5e",
                    "adapterKind": "authority_vector",
                    "sourcePath": str(source),
                    "outputDir": str(Path(root) / "output"),
                    "maximumElapsedSeconds": 10,
                    "maximumOutputBytes": 1024,
                }
            )
            self.assertEqual(command.source_path, source.resolve())

    def test_gtfs_rejects_archive_traversal(self):
        with tempfile.TemporaryDirectory() as root:
            source = Path(root) / "feed.zip"
            with zipfile.ZipFile(source, "w") as archive:
                archive.writestr("../agency.txt", "bad")
                for name in ["routes.txt", "stops.txt", "trips.txt", "stop_times.txt"]:
                    archive.writestr(name, "x")
            command = self.gtfs_command(root, source, 1024 * 1024)
            with self.assertRaises(ContractError):
                normalize_gtfs(command)

    def test_gtfs_normalizes_a_bounded_feed_and_records_validation_status(self):
        with tempfile.TemporaryDirectory() as root:
            source = Path(root) / "feed.zip"
            with zipfile.ZipFile(source, "w") as archive:
                for name in ["agency.txt", "routes.txt", "stops.txt", "trips.txt", "stop_times.txt"]:
                    archive.writestr(name, "header\n")
            command = self.gtfs_command(root, source, 1024 * 1024)

            with (
                patch.dict(
                    "os.environ", {"MAP_GTFS_VALIDATOR_JAR": "/validator.jar"}, clear=True
                ),
                patch("map_data.adapters.gtfs.run_tool") as validator,
            ):
                normalized, report_path, routing = normalize_gtfs(command)

            self.assertEqual(len(normalized), 1)
            self.assertEqual(normalized[0].read_bytes(), source.read_bytes())
            self.assertIsNone(routing)
            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertTrue(report["passed"])
            self.assertEqual(report["adapter"], "gtfs_schedule")
            self.assertTrue(
                next(
                    check["passed"]
                    for check in report["checks"]
                    if check["name"] == "canonical_validator_ran"
                )
            )
            validator.assert_called_once()

    def test_gtfs_rejects_excessive_expansion_before_extracting(self):
        with tempfile.TemporaryDirectory() as root:
            source = Path(root) / "feed.zip"
            with zipfile.ZipFile(source, "w", compression=zipfile.ZIP_DEFLATED) as archive:
                archive.writestr("agency.txt", "x" * 20_000)
                for name in ["routes.txt", "stops.txt", "trips.txt", "stop_times.txt"]:
                    archive.writestr(name, "header\n")
            command = self.gtfs_command(root, source, 1024)

            with self.assertRaisesRegex(ContractError, "expansion limits"):
                normalize_gtfs(command)

    def test_typed_result_emits_only_contract_fields(self):
        result = NormalizeResult(
            schemaVersion=2,
            acquisitionId="acquisition-test",
            sourceDigestSha256="a" * 64,
            versionLabel="sha256:" + "a" * 64,
            normalizedPaths=(Path("/tmp/normalized.parquet"),),
            qualityReportPath=Path("/tmp/quality.json"),
            routingBuildPath=None,
        )
        payload = json.loads(result.to_json())
        self.assertEqual(payload["schemaVersion"], 2)
        self.assertEqual(payload["acquisitionId"], "acquisition-test")
        self.assertIsNone(payload["routingBuildPath"])

    def test_osm_profile_preserves_element_versions_and_complete_json_tags(self):
        profile = _OSM_CONFIG.read_text(encoding="utf-8")
        lines = profile.splitlines()
        self.assertEqual(lines.count("osm_version=yes"), 5)
        self.assertEqual(lines.count("all_tags=yes"), 5)
        self.assertIn("report_all_tags=yes", profile)
        self.assertIn("tags_format=json", profile)

    def test_raster_metadata_preserves_transform_bands_units_and_nodata(self):
        with tempfile.TemporaryDirectory() as root:
            raster = Path(root) / "raster.tif"
            raster.write_bytes(b"bounded-raster")
            metadata = raster_metadata(
                {
                    "driverShortName": "GTiff",
                    "size": [4, 3],
                    "geoTransform": [100.0, 2.0, 0.0, 200.0, 0.0, -2.0],
                    "metadata": {"IMAGE_STRUCTURE": {"LAYOUT": "COG"}},
                    "coordinateSystem": {"wkt": 'PROJCRS["example"]'},
                    "cornerCoordinates": {
                        "upperLeft": [100.0, 200.0],
                        "lowerLeft": [100.0, 194.0],
                        "lowerRight": [108.0, 194.0],
                        "upperRight": [108.0, 200.0],
                    },
                    "bands": [
                        {
                            "type": "Float32",
                            "description": "elevation",
                            "unit": "m",
                            "noDataValue": -9999.0,
                            "colorInterpretation": "Gray",
                            "metadata": {
                                "": {
                                    "VEOVEO_VALUE_INTERPRETATION": "continuous"
                                }
                            },
                        }
                    ],
                },
                raster,
            )
            self.assertEqual(metadata["resolution"], [2.0, 2.0])
            self.assertEqual(metadata["bands"][0]["unit"], "m")
            self.assertEqual(metadata["bands"][0]["nodata"], -9999.0)
            self.assertEqual(len(metadata["checksumSha256"]), 64)

    def test_corridor_sampling_bounds_spacing_and_cross_track_offsets(self):
        samples = corridor_sample_positions(
            [(0.0, 0.0), (0.001, 0.0)],
            spacing=50.0,
            half_width=10.0,
            cross_track_samples=3,
        )
        self.assertEqual(len(samples), 12)
        self.assertEqual(
            sorted({round(sample[1], 6) for sample in samples}),
            [-10.0, 0.0, 10.0],
        )
        self.assertLessEqual(max(sample[0] for sample in samples), 112.0)


if __name__ == "__main__":
    unittest.main()


class PrivateProtocolSchemaTests(unittest.TestCase):
    def test_complete_reachable_private_protocol_schema(self):
        assert_peer_snapshot(
            Path(__file__).resolve().parents[2] / "testdata/private-protocol.schema.json",
            NormalizeCommand.model_json_schema(mode="validation"),
            NormalizeResult.model_json_schema(mode="serialization"),
        )


class NumericAdmissionTests(unittest.TestCase):
    def test_command_model_bounds_and_adapter_vocabulary(self):
        from pydantic import ValidationError
        valid = {"schemaVersion": 2, "acquisitionId": "acquisition-fixture",
                 "adapterKind": "gtfs_schedule", "sourcePath": "/tmp/source",
                 "outputDir": "/tmp/output", "maximumElapsedSeconds": 1,
                 "maximumOutputBytes": 2**64 - 1}
        NormalizeCommand.model_validate(valid)
        for field, value in [("maximumOutputBytes", 2**64), ("maximumElapsedSeconds", -1), ("maximumElapsedSeconds", True), ("adapterKind", "uncontrolled"), ("schemaVersion", 2**32)]:
            with self.assertRaises(ValidationError):
                NormalizeCommand.model_validate({**valid, field: value})
