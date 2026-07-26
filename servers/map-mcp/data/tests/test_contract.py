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
                "schema_version": 1,
                "acquisition_id": "acquisition-019f5cda-8c2d-7283-88c8-a72f4a138a5e",
                "adapter_kind": "gtfs_schedule",
                "source_path": str(source),
                "output_dir": str(Path(root) / "output"),
                "maximum_elapsed_seconds": 10,
                "maximum_output_bytes": maximum_output_bytes,
            }
        )

    def test_rejects_relative_and_missing_source_paths(self):
        with self.assertRaises(ContractError):
            NormalizeCommand.parse(
                {
                    "schema_version": 1,
                    "acquisition_id": "acquisition-test",
                    "adapter_kind": "authority_vector",
                    "source_path": "relative.json",
                    "output_dir": "/tmp/output",
                    "maximum_elapsed_seconds": 10,
                    "maximum_output_bytes": 1024,
                }
            )

    def test_accepts_confined_absolute_paths(self):
        with tempfile.TemporaryDirectory() as root:
            source = Path(root) / "input.geojson"
            source.write_text(json.dumps({"type": "FeatureCollection", "features": []}))
            command = NormalizeCommand.parse(
                {
                    "schema_version": 1,
                    "acquisition_id": "acquisition-019f5cda-8c2d-7283-88c8-a72f4a138a5e",
                    "adapter_kind": "authority_vector",
                    "source_path": str(source),
                    "output_dir": str(Path(root) / "output"),
                    "maximum_elapsed_seconds": 10,
                    "maximum_output_bytes": 1024,
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
            acquisition_id="acquisition-test",
            source_digest_sha256="a" * 64,
            version_label="sha256:" + "a" * 64,
            normalized_paths=(Path("/tmp/normalized.parquet"),),
            quality_report_path=Path("/tmp/quality.json"),
            routing_build_path=None,
        )
        payload = json.loads(result.to_json())
        self.assertEqual(payload["schema_version"], 1)
        self.assertEqual(payload["acquisition_id"], "acquisition-test")
        self.assertIsNone(payload["routing_build_path"])

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
            self.assertEqual(len(metadata["checksum_sha256"]), 64)

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
