"""Bounded OGC GeoPackage vector inspection and conversion.

The Rust server authorizes and stages every source before invoking this module.
This process receives one typed JSON command on stdin and emits one typed JSON
result on stdout. Diagnostics go to stderr.
"""

from __future__ import annotations

import hashlib
import json
import math
import os
from pathlib import Path
import sqlite3
import subprocess
import sys
from typing import Any, Iterator


SCHEMA_VERSION = 1
MAX_FEATURES = 10_000
MAX_TABLES = 256
MAX_FIELDS_PER_TABLE = 512
MAX_TOTAL_FIELDS = 4_096
MAX_METADATA_CHARS = 16_384
RS = b"\x1e"
CORE_CONFORMANCE = "http://www.opengis.net/spec/json-fg-1/1.0/conf/core"
TYPES_CONFORMANCE = "http://www.opengis.net/spec/json-fg-1/1.0/conf/types-schemas"
PROPERTY_PREFIX = "property:"
RESERVED_FIELDS = (
    "veoveo_id",
    "veoveo_feature_type",
    "veoveo_title",
    "veoveo_valid_from",
    "veoveo_valid_until",
)


class ContractError(ValueError):
    """A caller-visible contract rejection."""


def _osgeo() -> tuple[Any, Any, Any]:
    try:
        from osgeo import gdal, ogr, osr
    except ImportError as error:
        raise ContractError("the pinned GDAL Python bindings are unavailable") from error
    gdal.UseExceptions()
    return gdal, ogr, osr


def _bounded_string(value: Any, field: str, maximum: int = MAX_METADATA_CHARS) -> str:
    if not isinstance(value, str) or not value or len(value) > maximum:
        raise ContractError(f"{field} must be a non-empty string of at most {maximum} characters")
    if any(ord(character) < 32 or 127 <= ord(character) <= 159 for character in value):
        raise ContractError(f"{field} cannot contain control characters")
    return value


def _identifier(value: Any, field: str) -> str:
    return _bounded_string(value, field, 255)


def _absolute_file(value: Any, field: str) -> Path:
    path = Path(_bounded_string(value, field, 4_096))
    if not path.is_absolute() or not path.is_file() or path.is_symlink():
        raise ContractError(f"{field} must be an absolute regular non-symlink file")
    return path.resolve(strict=True)


def _absolute_output_directory(value: Any) -> Path:
    path = Path(_bounded_string(value, "output_dir", 4_096))
    if not path.is_absolute() or path.is_symlink():
        raise ContractError("output_dir must be an absolute non-symlink directory")
    path.mkdir(parents=False, exist_ok=True)
    if not path.is_dir():
        raise ContractError("output_dir must resolve to a directory")
    return path.resolve(strict=True)


def _positive_integer(value: Any, field: str, maximum: int | None = None) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        raise ContractError(f"{field} must be a positive integer")
    if maximum is not None and value > maximum:
        raise ContractError(f"{field} cannot exceed {maximum}")
    return value


def _optional_identifier(command: dict[str, Any], field: str) -> str | None:
    value = command.get(field)
    return None if value is None else _identifier(value, field)


def _validate_command(command: Any) -> dict[str, Any]:
    if not isinstance(command, dict) or command.get("schema_version") != SCHEMA_VERSION:
        raise ContractError("unsupported feature-package command schema")
    operation = command.get("operation")
    if operation not in {"inspect", "decode", "encode"}:
        raise ContractError("operation must be inspect, decode, or encode")
    return command


def _validate_geopackage(path: Path) -> tuple[bool, str]:
    process = subprocess.run(
        ["gdal", "driver", "gpkg", "validate", "--full-check", str(path)],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
        timeout=120,
        env={**os.environ, "PROJ_NETWORK": "OFF"},
    )
    diagnostic = (process.stdout + process.stderr).decode("utf-8", errors="replace")
    return process.returncode == 0, diagnostic[:32_768]


def _open_geopackage(path: Path) -> Any:
    gdal, _, _ = _osgeo()
    dataset = gdal.OpenEx(
        str(path),
        gdal.OF_VECTOR | gdal.OF_READONLY,
        allowed_drivers=["GPKG"],
        open_options=["LIST_ALL_TABLES=NO", "IMMUTABLE=YES"],
    )
    if dataset is None or dataset.GetDriver().ShortName != "GPKG":
        raise ContractError("source is not an OGC GeoPackage dataset")
    return dataset


def _sqlite_header(path: Path) -> tuple[int, int, str]:
    with path.open("rb") as source:
        header = source.read(72)
    if len(header) < 72 or header[:16] != b"SQLite format 3\x00":
        raise ContractError("GeoPackage does not have a valid SQLite header")
    application_id = int.from_bytes(header[68:72], "big")
    user_version = int.from_bytes(header[60:64], "big")
    versions = {10000: "1.0", 10100: "1.1", 10200: "1.2", 10300: "1.3", 10400: "1.4"}
    return application_id, user_version, versions.get(user_version, f"unknown:{user_version}")


def _sqlite_rows(path: Path, sql: str) -> list[tuple[Any, ...]]:
    uri = f"{path.as_uri()}?mode=ro&immutable=1"
    with sqlite3.connect(uri, uri=True) as connection:
        return list(connection.execute(sql))


def _field_type(field: Any, ogr: Any) -> str:
    if field.GetType() == ogr.OFTInteger and field.GetSubType() == ogr.OFSTBoolean:
        return "boolean"
    return {
        ogr.OFTInteger: "integer",
        ogr.OFTInteger64: "integer64",
        ogr.OFTReal: "real",
        ogr.OFTString: "string",
        ogr.OFTBinary: "binary",
        ogr.OFTDate: "date",
        ogr.OFTDateTime: "date_time",
    }.get(field.GetType(), "other")


def _declared_spatial_indexes(path: Path) -> set[tuple[str, str]]:
    try:
        rows = _sqlite_rows(
            path,
            "SELECT table_name, column_name FROM gpkg_extensions "
            "WHERE extension_name = 'gpkg_rtree_index'",
        )
    except sqlite3.Error:
        return set()
    return {(str(table), str(column)) for table, column in rows}


def _extensions(path: Path) -> list[dict[str, Any]]:
    try:
        rows = _sqlite_rows(
            path,
            "SELECT table_name, column_name, extension_name, definition, scope "
            "FROM gpkg_extensions ORDER BY extension_name, table_name, column_name",
        )
    except sqlite3.Error:
        return []
    output: list[dict[str, Any]] = []
    for table, column, name, definition, scope in rows:
        output.append(
            {
                "name": _bounded_string(str(name), "extension name"),
                "table": None if table is None else _identifier(str(table), "extension table"),
                "column": None if column is None else _identifier(str(column), "extension column"),
                "definition": _bounded_string(str(definition), "extension definition"),
                "scope": _bounded_string(str(scope), "extension scope", 64),
            }
        )
    return output


def _feature_table_names(path: Path) -> list[str]:
    try:
        rows = _sqlite_rows(
            path,
            "SELECT table_name FROM gpkg_contents "
            "WHERE data_type = 'features' ORDER BY table_name",
        )
    except sqlite3.Error as error:
        raise ContractError("GeoPackage is missing its feature table catalog") from error
    return [_identifier(str(row[0]), "feature table name") for row in rows]


def _contents_metadata(path: Path) -> dict[str, tuple[str, str]]:
    rows = _sqlite_rows(
        path,
        "SELECT table_name, identifier, description FROM gpkg_contents "
        "WHERE data_type = 'features'",
    )
    return {
        str(table): (
            str(identifier)[:MAX_METADATA_CHARS],
            str(description or "")[:MAX_METADATA_CHARS],
        )
        for table, identifier, description in rows
    }


def _geometry_catalog(path: Path) -> dict[str, tuple[str, str, int]]:
    rows = _sqlite_rows(
        path,
        "SELECT table_name, column_name, geometry_type_name, srs_id "
        "FROM gpkg_geometry_columns",
    )
    return {
        str(table): (str(column), str(geometry_type), int(srs_id))
        for table, column, geometry_type, srs_id in rows
    }


def _wgs84_extent(layer: Any) -> dict[str, float] | None:
    spatial_reference = layer.GetSpatialRef()
    if spatial_reference is None:
        return None
    authority = spatial_reference.GetAuthorityName(None)
    code = spatial_reference.GetAuthorityCode(None)
    if authority != "EPSG" or code != "4326":
        return None
    extent = layer.GetExtent(force=1)
    if extent is None or not all(math.isfinite(value) for value in extent):
        return None
    minimum_x, maximum_x, minimum_y, maximum_y = extent
    if minimum_x < -180 or maximum_x > 180 or minimum_y < -90 or maximum_y > 90:
        return None
    return {
        "west": minimum_x,
        "south": minimum_y,
        "east": maximum_x,
        "north": maximum_y,
    }


def inspect(path: Path) -> dict[str, Any]:
    valid, diagnostic = _validate_geopackage(path)
    dataset = _open_geopackage(path)
    _, ogr, _ = _osgeo()
    table_names = _feature_table_names(path)
    if len(table_names) > MAX_TABLES:
        raise ContractError(f"GeoPackage contains more than {MAX_TABLES} feature tables")
    spatial_indexes = _declared_spatial_indexes(path)
    contents = _contents_metadata(path)
    geometry_catalog = _geometry_catalog(path)
    feature_tables: list[dict[str, Any]] = []
    total_fields = 0
    for table_name in table_names:
        layer = _layer(dataset, table_name)
        definition = layer.GetLayerDefn()
        if definition.GetFieldCount() > MAX_FIELDS_PER_TABLE:
            raise ContractError(
                f"GeoPackage table {layer.GetName()!r} contains more than "
                f"{MAX_FIELDS_PER_TABLE} fields"
            )
        total_fields += definition.GetFieldCount()
        if total_fields > MAX_TOTAL_FIELDS:
            raise ContractError(f"GeoPackage contains more than {MAX_TOTAL_FIELDS} total fields")
        if table_name not in geometry_catalog:
            raise ContractError(f"feature table {table_name!r} has no geometry catalog entry")
        geometry_column, geometry_type, srs_id = geometry_catalog[table_name]
        fields = []
        for field_index in range(definition.GetFieldCount()):
            field = definition.GetFieldDefn(field_index)
            fields.append(
                {
                    "name": _identifier(field.GetName(), "field name"),
                    "field_type": _field_type(field, ogr),
                    "nullable": bool(field.IsNullable()),
                }
            )
        spatial_reference = layer.GetSpatialRef()
        crs_name = None
        if spatial_reference is not None:
            crs_name = spatial_reference.GetName() or None
        feature_count = layer.GetFeatureCount(force=1)
        if feature_count < 0:
            raise ContractError(f"cannot determine feature count for {layer.GetName()!r}")
        identifier, description = contents[table_name]
        feature_tables.append(
            {
                "table": _identifier(layer.GetName(), "table name"),
                "identifier": identifier,
                "description": description,
                "geometry_column": _identifier(geometry_column, "geometry column"),
                "geometry_type": geometry_type,
                "srs_id": srs_id,
                "crs_name": crs_name,
                "feature_count": feature_count,
                "extent_wgs84": _wgs84_extent(layer),
                "fields": fields,
                "has_spatial_index": (layer.GetName(), geometry_column) in spatial_indexes,
            }
        )
    application_id, user_version, version = _sqlite_header(path)
    findings = []
    if diagnostic.strip():
        findings.append(
            {
                "level": "warning" if valid else "error",
                "code": "gdal_full_validation",
                "message": diagnostic.strip(),
                "table": None,
            }
        )
    if not valid and not findings:
        findings.append(
            {
                "level": "error",
                "code": "gdal_full_validation",
                "message": "GDAL rejected GeoPackage conformance",
                "table": None,
            }
        )
    return {
        "version": version,
        "application_id": application_id,
        "user_version": user_version,
        "feature_tables": feature_tables,
        "extensions": _extensions(path),
        "findings": findings,
    }


def _layer(dataset: Any, table: str) -> Any:
    layer = dataset.GetLayerByName(table)
    if layer is None or layer.GetName() != table:
        raise ContractError(f"GeoPackage feature table {table!r} does not exist")
    return layer


def _canonical_ring(ring: list[Any], counterclockwise: bool) -> list[Any]:
    if len(ring) < 4 or ring[0] != ring[-1]:
        raise ContractError("GeoPackage polygon rings must be closed")
    area = sum(
        float(ring[index][0]) * float(ring[index + 1][1])
        - float(ring[index + 1][0]) * float(ring[index][1])
        for index in range(len(ring) - 1)
    )
    if (area > 0) != counterclockwise:
        ring = list(reversed(ring))
    return ring


def _canonical_geometry(value: dict[str, Any]) -> dict[str, Any]:
    geometry_type = value.get("type")
    coordinates = value.get("coordinates")
    if geometry_type == "Polygon":
        value["coordinates"] = [
            _canonical_ring(ring, index == 0) for index, ring in enumerate(coordinates)
        ]
    elif geometry_type == "MultiPolygon":
        value["coordinates"] = [
            [_canonical_ring(ring, index == 0) for index, ring in enumerate(polygon)]
            for polygon in coordinates
        ]
    return value


def _decoded_properties(feature: dict[str, Any], mapped: set[str], definition: Any, ogr: Any) -> dict[str, Any]:
    source = feature.get("properties") or {}
    properties: dict[str, Any] = {}
    for field_index in range(definition.GetFieldCount()):
        field = definition.GetFieldDefn(field_index)
        name = field.GetName()
        if name in mapped:
            continue
        target = name[len(PROPERTY_PREFIX) :] if name.startswith(PROPERTY_PREFIX) else name
        value = source.get(name)
        if value is not None and field.GetSubType() == ogr.OFSTJSON:
            try:
                value = json.loads(value)
            except (TypeError, json.JSONDecodeError) as error:
                raise ContractError(f"field {name!r} contains invalid JSON subtype data") from error
        properties[target] = value
    return properties


def decode(command: dict[str, Any]) -> dict[str, Any]:
    source_path = _absolute_file(command.get("source_path"), "source_path")
    output_dir = _absolute_output_directory(command.get("output_dir"))
    maximum_output_bytes = _positive_integer(command.get("maximum_output_bytes"), "maximum_output_bytes")
    maximum_features = _positive_integer(command.get("maximum_features"), "maximum_features", MAX_FEATURES)
    table = _identifier(command.get("table"), "table")
    default_semantic_type = _bounded_string(command.get("default_semantic_type"), "default_semantic_type", 256)
    mappings = {
        "identity": _optional_identifier(command, "identity_column"),
        "semantic": _optional_identifier(command, "semantic_type_column"),
        "title": _optional_identifier(command, "title_column"),
        "valid_from": _optional_identifier(command, "valid_from_column"),
        "valid_until": _optional_identifier(command, "valid_until_column"),
    }
    valid, diagnostic = _validate_geopackage(source_path)
    if not valid:
        raise ContractError(f"GeoPackage conformance validation failed: {diagnostic}")
    dataset = _open_geopackage(source_path)
    _, ogr, osr = _osgeo()
    layer = _layer(dataset, table)
    count = layer.GetFeatureCount(force=1)
    if count <= 0 or count > maximum_features:
        raise ContractError(f"selected table must contain between 1 and {maximum_features} features")
    definition = layer.GetLayerDefn()
    names = {definition.GetFieldDefn(index).GetName() for index in range(definition.GetFieldCount())}
    missing = sorted(value for value in mappings.values() if value is not None and value not in names)
    if missing:
        raise ContractError(f"mapped GeoPackage fields do not exist: {missing}")
    source_srs = layer.GetSpatialRef()
    if source_srs is None:
        raise ContractError("selected GeoPackage feature table has no CRS")
    source_srs.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
    destination_srs = osr.SpatialReference()
    destination_srs.SetFromUserInput("OGC:CRS84")
    destination_srs.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
    transform = None if source_srs.IsSame(destination_srs) else osr.CoordinateTransformation(source_srs, destination_srs)
    mapped = {value for value in mappings.values() if value is not None}
    path = output_dir / "features.geojsons"
    digest = hashlib.sha256()
    byte_count = 0
    try:
        with path.open("xb") as output:
            for index, ogr_feature in enumerate(layer):
                if index >= maximum_features:
                    raise ContractError(f"selected table exceeds {maximum_features} features")
                raw = json.loads(ogr_feature.ExportToJson())
                geometry = ogr_feature.GetGeometryRef()
                if geometry is None or geometry.IsEmpty() or not geometry.IsValid():
                    raise ContractError("GeoPackage features require non-empty valid geometry")
                if geometry.Is3D() or geometry.IsMeasured():
                    raise ContractError("GeoPackage import supports bounded two-dimensional geometry only")
                geometry = geometry.Clone()
                if transform is not None:
                    geometry.Transform(transform)
                geometry_name = geometry.GetGeometryName().upper()
                if geometry_name not in {
                    "POINT", "MULTIPOINT", "LINESTRING", "MULTILINESTRING", "POLYGON", "MULTIPOLYGON"
                }:
                    raise ContractError(f"unsupported GeoPackage geometry type {geometry_name}")
                geometry_json = _canonical_geometry(json.loads(geometry.ExportToJson()))
                raw_properties = raw.get("properties") or {}
                semantic = raw_properties.get(mappings["semantic"]) if mappings["semantic"] else None
                if semantic is None:
                    semantic = default_semantic_type
                semantic = _bounded_string(semantic, "feature semantic type", 256)
                identity = raw_properties.get(mappings["identity"]) if mappings["identity"] else ogr_feature.GetFID()
                if identity is None:
                    raise ContractError("every imported GeoPackage feature requires an identity")
                output_feature: dict[str, Any] = {
                    "type": "Feature",
                    "conformsTo": [CORE_CONFORMANCE, TYPES_CONFORMANCE],
                    "id": identity,
                    "geometry": geometry_json,
                    "properties": _decoded_properties(raw, mapped, definition, ogr),
                    "featureType": semantic,
                }
                title = raw_properties.get(mappings["title"]) if mappings["title"] else None
                if title is not None:
                    output_feature["title"] = title
                valid_from = raw_properties.get(mappings["valid_from"]) if mappings["valid_from"] else None
                valid_until = raw_properties.get(mappings["valid_until"]) if mappings["valid_until"] else None
                if valid_from is not None or valid_until is not None:
                    output_feature["time"] = {
                        "interval": [valid_from if valid_from is not None else "..", valid_until if valid_until is not None else ".."]
                    }
                record = RS + json.dumps(output_feature, ensure_ascii=False, separators=(",", ":")).encode("utf-8") + b"\n"
                byte_count += len(record)
                if byte_count > maximum_output_bytes:
                    raise ContractError("decoded GeoJSON sequence exceeds the output byte limit")
                output.write(record)
                digest.update(record)
            output.flush()
            os.fsync(output.fileno())
    except Exception:
        path.unlink(missing_ok=True)
        raise
    return {
        "schema_version": SCHEMA_VERSION,
        "path": str(path),
        "filename": "features.geojsons",
        "mime_type": "application/geo+json-seq",
        "feature_count": count,
        "byte_count": byte_count,
        "digest_sha256": digest.hexdigest(),
    }


def _sequence(path: Path, maximum_input_bytes: int) -> Iterator[dict[str, Any]]:
    if path.stat().st_size > maximum_input_bytes:
        raise ContractError("GeoJSON sequence exceeds the configured byte limit")
    with path.open("rb") as source:
        count = 0
        for line in source:
            if not line.startswith(RS) or not line.endswith(b"\n"):
                raise ContractError("GeoJSON sequence records require ASCII RS and trailing LF")
            try:
                value = json.loads(line[1:-1])
            except json.JSONDecodeError as error:
                raise ContractError("GeoJSON sequence contains invalid JSON") from error
            if not isinstance(value, dict) or value.get("type") != "Feature":
                raise ContractError("GeoJSON sequence records must be Features")
            count += 1
            if count > MAX_FEATURES:
                raise ContractError(f"GeoJSON sequence exceeds {MAX_FEATURES} features")
            yield value
        if count == 0:
            raise ContractError("GeoJSON sequence cannot be empty")


def _value_kind(value: Any) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, int):
        return "integer64"
    if isinstance(value, float) and math.isfinite(value):
        return "real"
    if isinstance(value, str):
        return "string"
    return "json"


def _property_schema(path: Path, maximum_input_bytes: int) -> dict[str, str]:
    kinds: dict[str, set[str]] = {}
    for feature in _sequence(path, maximum_input_bytes):
        properties = feature.get("properties") or {}
        if not isinstance(properties, dict):
            raise ContractError("feature properties must be an object")
        for name, value in properties.items():
            _identifier(name, "property name")
            if len(PROPERTY_PREFIX) + len(name) > 255:
                raise ContractError("prefixed GeoPackage property field names cannot exceed 255 characters")
            kinds.setdefault(name, set()).add(_value_kind(value))
    if len(kinds) + len(RESERVED_FIELDS) > MAX_FIELDS_PER_TABLE:
        raise ContractError(f"GeoPackage output cannot exceed {MAX_FIELDS_PER_TABLE} fields")
    schema = {}
    for name, observed in kinds.items():
        concrete = observed - {"null"}
        schema[name] = next(iter(concrete)) if len(concrete) == 1 else "json"
    return schema


def _create_field(layer: Any, name: str, kind: str, ogr: Any, nullable: bool = True) -> None:
    field_type = {
        "boolean": ogr.OFTInteger,
        "integer64": ogr.OFTInteger64,
        "real": ogr.OFTReal,
        "string": ogr.OFTString,
        "json": ogr.OFTString,
    }[kind]
    field = ogr.FieldDefn(name, field_type)
    field.SetNullable(nullable)
    if kind == "boolean":
        field.SetSubType(ogr.OFSTBoolean)
    elif kind == "json":
        field.SetSubType(ogr.OFSTJSON)
    if layer.CreateField(field) != ogr.OGRERR_NONE:
        raise ContractError(f"GDAL could not create field {name!r}")


def _set_field(target: Any, name: str, value: Any, kind: str) -> None:
    if value is None:
        return
    if kind == "json":
        target.SetField(name, json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True))
    else:
        target.SetField(name, value)


def encode(command: dict[str, Any]) -> dict[str, Any]:
    source_path = _absolute_file(command.get("source_path"), "source_path")
    output_dir = _absolute_output_directory(command.get("output_dir"))
    maximum_output_bytes = _positive_integer(command.get("maximum_output_bytes"), "maximum_output_bytes")
    table = _identifier(command.get("table"), "table")
    schema = _property_schema(source_path, maximum_output_bytes)
    gdal, ogr, osr = _osgeo()
    output_path = output_dir / "layer.gpkg"
    if output_path.exists():
        raise ContractError("GeoPackage output already exists")
    spatial_reference = osr.SpatialReference()
    spatial_reference.ImportFromEPSG(4326)
    spatial_reference.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
    driver = ogr.GetDriverByName("GPKG")
    dataset = driver.CreateDataSource(
        str(output_path), options=["VERSION=1.4", "ADD_GPKG_OGR_CONTENTS=YES"]
    )
    if dataset is None:
        raise ContractError("GDAL could not create GeoPackage output")
    feature_count = 0
    try:
        layer = dataset.CreateLayer(
            table,
            srs=spatial_reference,
            geom_type=ogr.wkbUnknown,
            options=["GEOMETRY_NAME=geom", "GEOMETRY_NULLABLE=NO", "SPATIAL_INDEX=YES"],
        )
        if layer is None:
            raise ContractError("GDAL could not create the GeoPackage feature table")
        for name in RESERVED_FIELDS:
            _create_field(layer, name, "string", ogr)
        for name, kind in sorted(schema.items()):
            _create_field(layer, f"{PROPERTY_PREFIX}{name}", kind, ogr)
        for feature in _sequence(source_path, maximum_output_bytes):
            geometry_value = feature.get("geometry")
            geometry = ogr.CreateGeometryFromJson(json.dumps(geometry_value, separators=(",", ":")))
            if geometry is None or geometry.IsEmpty() or not geometry.IsValid():
                raise ContractError("GeoJSON export features require valid non-empty geometry")
            if geometry.Is3D() or geometry.IsMeasured():
                raise ContractError("GeoPackage export supports two-dimensional geometry only")
            target = ogr.Feature(layer.GetLayerDefn())
            target.SetGeometry(geometry)
            _set_field(target, "veoveo_id", feature.get("id"), "string")
            _set_field(target, "veoveo_feature_type", feature.get("featureType"), "string")
            _set_field(target, "veoveo_title", feature.get("title"), "string")
            interval = (feature.get("time") or {}).get("interval", [None, None])
            if not isinstance(interval, list) or len(interval) != 2:
                raise ContractError("feature time interval must contain exactly two bounds")
            _set_field(target, "veoveo_valid_from", None if interval[0] == ".." else interval[0], "string")
            _set_field(target, "veoveo_valid_until", None if interval[1] == ".." else interval[1], "string")
            properties = feature.get("properties") or {}
            for name, kind in schema.items():
                _set_field(target, f"{PROPERTY_PREFIX}{name}", properties.get(name), kind)
            if layer.CreateFeature(target) != ogr.OGRERR_NONE:
                raise ContractError("GDAL could not write a GeoPackage feature")
            feature_count += 1
        dataset = None
        size = output_path.stat().st_size
        if size > maximum_output_bytes:
            raise ContractError("GeoPackage output exceeds the configured byte limit")
        valid, diagnostic = _validate_geopackage(output_path)
        if not valid:
            raise ContractError(f"generated GeoPackage failed conformance validation: {diagnostic}")
        reopened = _open_geopackage(output_path)
        if _layer(reopened, table).GetFeatureCount(force=1) != feature_count:
            raise ContractError("generated GeoPackage did not reopen with the expected feature count")
        digest = hashlib.sha256()
        with output_path.open("rb") as source:
            for block in iter(lambda: source.read(1024 * 1024), b""):
                digest.update(block)
        return {
            "schema_version": SCHEMA_VERSION,
            "path": str(output_path),
            "filename": "layer.gpkg",
            "mime_type": "application/geopackage+sqlite3",
            "feature_count": feature_count,
            "byte_count": size,
            "digest_sha256": digest.hexdigest(),
        }
    except Exception:
        dataset = None
        output_path.unlink(missing_ok=True)
        raise


def execute(command: Any) -> dict[str, Any]:
    command = _validate_command(command)
    operation = command["operation"]
    if operation == "inspect":
        manifest = inspect(_absolute_file(command.get("source_path"), "source_path"))
        return {"schema_version": SCHEMA_VERSION, "manifest": manifest}
    if operation == "decode":
        return decode(command)
    return encode(command)


def main() -> int:
    try:
        command = json.load(sys.stdin)
        result = execute(command)
        json.dump(result, sys.stdout, ensure_ascii=False, separators=(",", ":"))
        sys.stdout.write("\n")
        return 0
    except (ContractError, OSError, sqlite3.Error, subprocess.SubprocessError, ValueError) as error:
        print(str(error), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
