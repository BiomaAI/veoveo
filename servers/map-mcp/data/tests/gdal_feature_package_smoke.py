"""Container-only round-trip acceptance for the pinned GDAL GeoPackage adapter."""

from __future__ import annotations

import json
from pathlib import Path
import tempfile

from osgeo import gdal, ogr, osr

from map_data.feature_package import execute


def _source_geopackage(path: Path) -> None:
    driver = ogr.GetDriverByName("GPKG")
    dataset = driver.CreateDataSource(str(path), options=["VERSION=1.4"])
    spatial_reference = osr.SpatialReference()
    spatial_reference.ImportFromEPSG(3857)
    layer = dataset.CreateLayer(
        "named places",
        srs=spatial_reference,
        geom_type=ogr.wkbPoint,
        options=["GEOMETRY_NAME=shape", "SPATIAL_INDEX=YES"],
    )
    for name, field_type, subtype in (
        ("external id", ogr.OFTString, None),
        ("kind", ogr.OFTString, None),
        ("name", ogr.OFTString, None),
        ("active", ogr.OFTInteger, ogr.OFSTBoolean),
        ("nested", ogr.OFTString, ogr.OFSTJSON),
        ("valid from", ogr.OFTString, None),
    ):
        field = ogr.FieldDefn(name, field_type)
        if subtype is not None:
            field.SetSubType(subtype)
        assert layer.CreateField(field) == ogr.OGRERR_NONE
    feature = ogr.Feature(layer.GetLayerDefn())
    feature.SetField("external id", "place-1")
    feature.SetField("kind", "NamedPlace")
    feature.SetField("name", "Origin")
    feature.SetField("active", True)
    feature.SetField("nested", '{"rank":1}')
    feature.SetField("valid from", "2026-01-01T00:00:00Z")
    geometry = ogr.Geometry(ogr.wkbPoint)
    geometry.AddPoint_2D(0, 0)
    feature.SetGeometry(geometry)
    assert layer.CreateFeature(feature) == ogr.OGRERR_NONE
    dataset = None


def main() -> None:
    gdal.UseExceptions()
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        source = root / "source.gpkg"
        decoded_dir = root / "decoded"
        encoded_dir = root / "encoded"
        decoded_dir.mkdir()
        encoded_dir.mkdir()
        _source_geopackage(source)
        inspected = execute(
            {"schema_version": 1, "operation": "inspect", "source_path": str(source)}
        )
        table = inspected["manifest"]["feature_tables"][0]
        assert table["table"] == "named places"
        assert table["feature_count"] == 1
        assert table["has_spatial_index"] is True
        decoded = execute(
            {
                "schema_version": 1,
                "operation": "decode",
                "source_path": str(source),
                "output_dir": str(decoded_dir),
                "maximum_output_bytes": 1_048_576,
                "maximum_features": 10_000,
                "table": "named places",
                "identity_column": "external id",
                "semantic_type_column": "kind",
                "default_semantic_type": "NamedPlace",
                "title_column": "name",
                "valid_from_column": "valid from",
                "valid_until_column": None,
            }
        )
        record = Path(decoded["path"]).read_bytes()
        feature = json.loads(record[1:-1])
        assert feature["id"] == "place-1"
        assert feature["featureType"] == "NamedPlace"
        assert feature["title"] == "Origin"
        assert feature["geometry"] == {"type": "Point", "coordinates": [0.0, 0.0]}
        assert feature["properties"] == {"active": True, "nested": {"rank": 1}}
        encoded = execute(
            {
                "schema_version": 1,
                "operation": "encode",
                "source_path": decoded["path"],
                "output_dir": str(encoded_dir),
                "maximum_output_bytes": 1_048_576,
                "table": "published places",
            }
        )
        round_trip = execute(
            {"schema_version": 1, "operation": "inspect", "source_path": encoded["path"]}
        )["manifest"]["feature_tables"][0]
        assert round_trip["table"] == "published places"
        assert round_trip["feature_count"] == 1
        assert round_trip["has_spatial_index"] is True
        print("GeoPackage inspect/decode/encode round trip passed")


if __name__ == "__main__":
    main()
