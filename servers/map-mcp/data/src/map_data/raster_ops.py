from __future__ import annotations

from collections import deque
import json
import math
from pathlib import Path
import sys
from typing import Any

import numpy as np
from osgeo import gdal, ogr, osr

from map_data.contract import ContractError
from map_data.terrain import corridor_sample_positions


SCHEMA_VERSION = 2
MAX_SAMPLES = 10_000
MAX_WINDOW_PIXELS = 4096 * 4096
MAX_FULL_RASTER_PIXELS = 2048 * 2048


def run(value: Any) -> dict[str, Any]:
    if not isinstance(value, dict) or value.get("schemaVersion") != SCHEMA_VERSION:
        raise ContractError("unsupported raster-operation schema")
    if value.keys() - {"schemaVersion", "sourcePath", "outputDir", "maximumOutputBytes", "operation"}:
        raise ContractError("raster command contains an unsupported field")
    operation = value.get("operation")
    if not isinstance(operation, dict) or not isinstance(operation.get("kind"), str):
        raise ContractError("operation is invalid")
    fields = {
        "sample":{"band", "positions"}, "window":{"bounds", "width", "height"},
        "corridor_maximum":{"band", "corridor", "sampleSpacing", "halfWidth", "crossTrackSamples"},
        "class_mask":{"band", "classes"}, "contour":{"band", "interval", "base"},
        "polygonize":{"band"}, "skeletonize":{"band", "threshold"},
        "derive_lines":{"band", "threshold", "minimumLength"},
    }
    kind = operation["kind"]
    if kind not in fields or operation.keys() - (fields[kind] | {"kind"}):
        raise ContractError("raster operation contains an unsupported field or kind")
    admit_operation(operation, fields[kind])
    maximum_output_bytes = positive_int(value.get("maximumOutputBytes"), "maximumOutputBytes")
    source = confined_file(value.get("sourcePath"), "sourcePath")
    output_dir = confined_directory(value.get("outputDir"), "outputDir")
    gdal.UseExceptions()
    dataset = gdal.Open(str(source), gdal.GA_ReadOnly)
    if dataset is None:
        raise ContractError("source raster cannot be opened")
    kind = operation["kind"]
    if kind not in {"sample", "window", "corridor_maximum"} and (
        dataset.RasterXSize * dataset.RasterYSize > MAX_FULL_RASTER_PIXELS
    ):
        raise ContractError("full-raster operation exceeds its source pixel limit")
    if kind == "sample":
        result = sample(dataset, operation)
        path = output_dir / "raster-samples.json"
        path.write_text(json.dumps(result, separators=(",", ":"), sort_keys=True), encoding="utf-8")
        mime_type = "application/json"
    elif kind == "window":
        path = window(dataset, operation, output_dir)
        mime_type = "image/tiff; application=geotiff; profile=cloud-optimized"
    elif kind == "corridor_maximum":
        result = corridor_maximum(dataset, operation)
        path = output_dir / "terrain-corridor-maximum.json"
        path.write_text(
            json.dumps(result, separators=(",", ":"), sort_keys=True),
            encoding="utf-8",
        )
        mime_type = "application/json"
    elif kind == "class_mask":
        path = write_mask(dataset, class_mask(dataset, operation), output_dir, "class-mask.tif")
        mime_type = "image/tiff; application=geotiff; profile=cloud-optimized"
    elif kind == "contour":
        path = contour(dataset, operation, output_dir)
        mime_type = "application/geo+json"
    elif kind == "polygonize":
        path = polygonize(dataset, operation, output_dir)
        mime_type = "application/geo+json"
    elif kind == "skeletonize":
        mask = threshold_mask(dataset, operation)
        path = write_mask(dataset, thin(mask), output_dir, "skeleton.tif")
        mime_type = "image/tiff; application=geotiff; profile=cloud-optimized"
    elif kind == "derive_lines":
        mask = thin(threshold_mask(dataset, operation))
        path = derive_lines(dataset, mask, operation, output_dir)
        mime_type = "application/geo+json"
    else:
        raise ContractError(f"unsupported raster operation {kind!r}")
    if not path.is_file() or path.stat().st_size > maximum_output_bytes:
        raise ContractError("raster operation output is absent or exceeds its byte limit")
    output_crs, output_transform = output_spatial_metadata(path, mime_type)
    return {
        "schemaVersion": SCHEMA_VERSION,
        "path": str(path),
        "filename": path.name,
        "mimeType": mime_type,
        "outputCrs": output_crs,
        "outputTransform": output_transform,
    }


def output_spatial_metadata(path: Path, mime_type: str) -> tuple[str, list[float] | None]:
    if mime_type.startswith("image/tiff"):
        dataset = gdal.Open(str(path), gdal.GA_ReadOnly)
        if dataset is None or not dataset.GetProjection():
            raise ContractError("derived raster omitted its coordinate reference system")
        return " ".join(dataset.GetProjection().split()), list(dataset.GetGeoTransform())
    return "EPSG:4326", None


def closed_object(value: Any, required: set[str], optional: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or required - value.keys() or value.keys() - (required | optional):
        raise ContractError(f"{label} contains missing or unsupported fields")
    return value


def admit_position(value: Any) -> None:
    position = closed_object(value, {"longitudeDeg", "latitudeDeg"}, {"ellipsoidalHeightM"}, "position")
    longitude = finite_number(position["longitudeDeg"], "longitudeDeg")
    latitude = finite_number(position["latitudeDeg"], "latitudeDeg")
    if not -180 <= longitude <= 180 or not -90 <= latitude <= 90:
        raise ContractError("position is outside WGS84")
    if position.get("ellipsoidalHeightM") is not None:
        finite_number(position["ellipsoidalHeightM"], "ellipsoidalHeightM")


def admit_operation(operation: dict[str, Any], fields: set[str]) -> None:
    closed_object(operation, fields | {"kind"}, set(), "raster operation")
    kind = operation["kind"]
    if "band" in fields and positive_int(operation["band"], "band") > 2**32 - 1:
        raise ContractError("band exceeds the owner integer profile")
    if kind == "sample":
        positions = operation["positions"]
        if not isinstance(positions, list) or not 1 <= len(positions) <= MAX_SAMPLES:
            raise ContractError("sample positions exceed their bound")
        for position in positions:
            admit_position(position)
    elif kind == "corridor_maximum":
        corridor = closed_object(operation["corridor"], {"coordinates"}, set(), "corridor")
        positions = corridor["coordinates"]
        if not isinstance(positions, list) or not 2 <= len(positions) <= MAX_SAMPLES:
            raise ContractError("corridor positions exceed their bound")
        for position in positions:
            admit_position(position)
        positive_number(operation["sampleSpacing"], "sampleSpacing")
        half_width = non_negative_number(operation["halfWidth"], "halfWidth")
        cross = positive_int(operation["crossTrackSamples"], "crossTrackSamples")
        if cross > 64 or (half_width == 0 and cross != 1):
            raise ContractError("corridor cross-track samples exceed their bound")
    elif kind == "window":
        bounds = closed_object(operation["bounds"], {"west", "south", "east", "north"}, set(), "bounds")
        west, south, east, north = (finite_number(bounds[key], key) for key in ["west", "south", "east", "north"])
        if not -180 <= west <= east <= 180 or not -90 <= south < north <= 90:
            raise ContractError("window bounds are invalid")
        width = positive_int(operation["width"], "width")
        height = positive_int(operation["height"], "height")
        if width > 2**32 - 1 or height > 2**32 - 1 or width * height > MAX_WINDOW_PIXELS:
            raise ContractError("window dimensions exceed their bound")
    elif kind == "class_mask":
        classes = operation["classes"]
        if not isinstance(classes, list) or not 1 <= len(classes) <= 256 or any(type(v) is not int or not -(2**63) <= v < 2**63 for v in classes):
            raise ContractError("classes exceed their owner profile")
    elif kind == "contour":
        positive_number(operation["interval"], "interval")
        finite_number(operation["base"], "base")
    elif kind in {"skeletonize", "derive_lines"}:
        finite_number(operation["threshold"], "threshold")
        if kind == "derive_lines":
            non_negative_number(operation["minimumLength"], "minimumLength")


def sample(dataset: gdal.Dataset, operation: dict[str, Any]) -> dict[str, Any]:
    band_number = band_index(dataset, operation)
    positions = operation.get("positions")
    if not isinstance(positions, list) or not 1 <= len(positions) <= MAX_SAMPLES:
        raise ContractError("sample positions exceed their bound")
    inverse = gdal.InvGeoTransform(dataset.GetGeoTransform())
    transform = coordinate_transform_from_wgs84(dataset)
    band = dataset.GetRasterBand(band_number)
    values = []
    for position in positions:
        if not isinstance(position, dict):
            raise ContractError("sample position is invalid")
        longitude = finite_number(position.get("longitudeDeg"), "longitudeDeg")
        latitude = finite_number(position.get("latitudeDeg"), "latitudeDeg")
        x, y, _ = transform.TransformPoint(longitude, latitude)
        pixel = int(math.floor(inverse[0] + inverse[1] * x + inverse[2] * y))
        row = int(math.floor(inverse[3] + inverse[4] * x + inverse[5] * y))
        value = None
        if 0 <= pixel < dataset.RasterXSize and 0 <= row < dataset.RasterYSize:
            array = band.ReadAsArray(pixel, row, 1, 1)
            if array is not None:
                candidate = float(array[0, 0])
                nodata = band.GetNoDataValue()
                if math.isfinite(candidate) and (nodata is None or candidate != nodata):
                    value = candidate
        values.append(
            {
                "position": {"longitudeDeg": longitude, "latitudeDeg": latitude},
                "value": value,
            }
        )
    return {"schemaVersion": 2, "band": band_number, "samples": values}


def corridor_maximum(dataset: gdal.Dataset, operation: dict[str, Any]) -> dict[str, Any]:
    band_number = band_index(dataset, operation)
    corridor = operation.get("corridor")
    if not isinstance(corridor, dict):
        raise ContractError("corridor geometry is invalid")
    coordinates = corridor.get("coordinates")
    if not isinstance(coordinates, list) or not 2 <= len(coordinates) <= MAX_SAMPLES:
        raise ContractError("corridor coordinates exceed their bound")
    positions = []
    for position in coordinates:
        if not isinstance(position, dict):
            raise ContractError("corridor position is invalid")
        longitude = finite_number(position.get("longitudeDeg"), "longitudeDeg")
        latitude = finite_number(position.get("latitudeDeg"), "latitudeDeg")
        if not -180 <= longitude <= 180 or not -90 <= latitude <= 90:
            raise ContractError("corridor position is outside WGS84")
        positions.append((longitude, latitude))
    spacing = positive_number(operation.get("sampleSpacing"), "sampleSpacing")
    half_width = non_negative_number(operation.get("halfWidth"), "halfWidth")
    cross_track_samples = positive_int(
        operation.get("crossTrackSamples"), "crossTrackSamples"
    )
    if cross_track_samples > 64 or (half_width == 0 and cross_track_samples != 1):
        raise ContractError("corridor cross-track sampling is invalid")
    samples = corridor_sample_positions(
        positions, spacing, half_width, cross_track_samples
    )
    if len(samples) > MAX_SAMPLES:
        raise ContractError("corridor samples exceed their bound")
    inverse = gdal.InvGeoTransform(dataset.GetGeoTransform())
    to_raster = coordinate_transform_from_wgs84(dataset)
    band = dataset.GetRasterBand(band_number)
    output_samples = []
    valid_samples = []
    for along_distance, cross_track_offset, longitude, latitude in samples:
        value = sample_value(
            dataset, band, inverse, to_raster, longitude, latitude
        )
        item = {
            "alongDistanceM": along_distance,
            "crossTrackOffsetM": cross_track_offset,
            "position": {
                "longitudeDeg": longitude,
                "latitudeDeg": latitude,
            },
            "value": value,
        }
        output_samples.append(item)
        if value is not None:
            valid_samples.append(item)
    if not valid_samples:
        raise ContractError("corridor contains no valid raster samples")
    return {
        "schemaVersion": 2,
        "band": band_number,
        "sampleSpacingM": spacing,
        "halfWidthM": half_width,
        "crossTrackSamples": cross_track_samples,
        "sampleCount": len(output_samples),
        "validSampleCount": len(valid_samples),
        "minimum": min(valid_samples, key=lambda item: item["value"]),
        "maximum": max(valid_samples, key=lambda item: item["value"]),
        "samples": output_samples,
    }


def sample_value(
    dataset: gdal.Dataset,
    band: gdal.Band,
    inverse: tuple[float, ...],
    transform: osr.CoordinateTransformation,
    longitude: float,
    latitude: float,
) -> float | None:
    x, y, _ = transform.TransformPoint(longitude, latitude)
    pixel = int(math.floor(inverse[0] + inverse[1] * x + inverse[2] * y))
    row = int(math.floor(inverse[3] + inverse[4] * x + inverse[5] * y))
    if not 0 <= pixel < dataset.RasterXSize or not 0 <= row < dataset.RasterYSize:
        return None
    array = band.ReadAsArray(pixel, row, 1, 1)
    if array is None:
        return None
    candidate = float(array[0, 0])
    nodata = band.GetNoDataValue()
    if not math.isfinite(candidate) or (nodata is not None and candidate == nodata):
        return None
    return candidate


def window(dataset: gdal.Dataset, operation: dict[str, Any], output_dir: Path) -> Path:
    bounds = operation.get("bounds")
    if not isinstance(bounds, dict):
        raise ContractError("window bounds are invalid")
    west = finite_number(bounds.get("west"), "west")
    south = finite_number(bounds.get("south"), "south")
    east = finite_number(bounds.get("east"), "east")
    north = finite_number(bounds.get("north"), "north")
    width = positive_int(operation.get("width"), "width")
    height = positive_int(operation.get("height"), "height")
    if west > east or south >= north or width * height > MAX_WINDOW_PIXELS:
        raise ContractError("window bounds or pixels exceed their limit")
    output = output_dir / "raster-window.tif"
    translated = gdal.Translate(
        str(output),
        dataset,
        format="COG",
        projWin=[west, north, east, south],
        projWinSRS="EPSG:4326",
        width=width,
        height=height,
        creationOptions=["COMPRESS=ZSTD", "OVERVIEWS=AUTO"],
    )
    if translated is None:
        raise ContractError("GDAL did not produce the raster window")
    translated = None
    return output


def class_mask(dataset: gdal.Dataset, operation: dict[str, Any]) -> np.ndarray:
    band = dataset.GetRasterBand(band_index(dataset, operation))
    classes = operation.get("classes")
    if (
        not isinstance(classes, list)
        or not 1 <= len(classes) <= 256
        or not all(isinstance(value, int) and not isinstance(value, bool) for value in classes)
    ):
        raise ContractError("class mask values are invalid")
    values = band.ReadAsArray()
    return np.isin(values, np.asarray(classes)).astype(np.uint8)


def threshold_mask(dataset: gdal.Dataset, operation: dict[str, Any]) -> np.ndarray:
    band = dataset.GetRasterBand(band_index(dataset, operation))
    threshold = finite_number(operation.get("threshold"), "threshold")
    values = band.ReadAsArray()
    nodata = band.GetNoDataValue()
    valid = np.isfinite(values)
    if nodata is not None:
        valid &= values != nodata
    return (valid & (values >= threshold)).astype(np.uint8)


def write_mask(dataset: gdal.Dataset, mask: np.ndarray, output_dir: Path, name: str) -> Path:
    temporary = output_dir / f".{name}.working.tif"
    driver = gdal.GetDriverByName("GTiff")
    target = driver.Create(
        str(temporary),
        dataset.RasterXSize,
        dataset.RasterYSize,
        1,
        gdal.GDT_Byte,
        options=["TILED=YES", "COMPRESS=ZSTD"],
    )
    target.SetGeoTransform(dataset.GetGeoTransform())
    target.SetProjection(dataset.GetProjection())
    target.GetRasterBand(1).SetNoDataValue(0)
    target.GetRasterBand(1).WriteArray(mask)
    target.FlushCache()
    target = None
    output = output_dir / name
    translated = gdal.Translate(
        str(output),
        str(temporary),
        format="COG",
        creationOptions=["COMPRESS=ZSTD", "OVERVIEWS=AUTO"],
    )
    if translated is None:
        raise ContractError("GDAL did not produce the mask")
    translated = None
    temporary.unlink()
    return output


def contour(dataset: gdal.Dataset, operation: dict[str, Any], output_dir: Path) -> Path:
    band = dataset.GetRasterBand(band_index(dataset, operation))
    interval = finite_number(operation.get("interval"), "interval")
    base = finite_number(operation.get("base"), "base")
    if interval <= 0:
        raise ContractError("contour interval must be positive")
    temporary = output_dir / ".contours.working.geojson"
    output = output_dir / "contours.geojson"
    data_source, layer = vector_layer(
        dataset, temporary, ogr.wkbLineString, "elevation"
    )
    gdal.ContourGenerateEx(
        band,
        layer,
        options=[f"LEVEL_INTERVAL={interval}", f"LEVEL_BASE={base}", "ELEV_FIELD=0"],
    )
    data_source = None
    reproject_geojson(temporary, output)
    return output


def polygonize(dataset: gdal.Dataset, operation: dict[str, Any], output_dir: Path) -> Path:
    band = dataset.GetRasterBand(band_index(dataset, operation))
    temporary = output_dir / ".polygons.working.geojson"
    output = output_dir / "polygons.geojson"
    data_source, layer = vector_layer(dataset, temporary, ogr.wkbPolygon, "value")
    gdal.Polygonize(band, band.GetMaskBand(), layer, 0, [], callback=None)
    data_source = None
    reproject_geojson(temporary, output)
    return output


def vector_layer(
    dataset: gdal.Dataset, output: Path, geometry_type: int, field_name: str
) -> tuple[ogr.DataSource, ogr.Layer]:
    driver = ogr.GetDriverByName("GeoJSON")
    data_source = driver.CreateDataSource(str(output))
    spatial_reference = osr.SpatialReference()
    spatial_reference.ImportFromWkt(dataset.GetProjection())
    layer = data_source.CreateLayer("derived", spatial_reference, geometry_type)
    layer.CreateField(ogr.FieldDefn(field_name, ogr.OFTReal))
    return data_source, layer


def reproject_geojson(source: Path, output: Path) -> None:
    translated = gdal.VectorTranslate(
        str(output),
        str(source),
        format="GeoJSON",
        dstSRS="EPSG:4326",
        layerCreationOptions=["RFC7946=YES"],
    )
    if translated is None:
        raise ContractError("GDAL did not produce the WGS84 GeoJSON derivation")
    translated = None
    source.unlink()


def thin(mask: np.ndarray) -> np.ndarray:
    image = (mask > 0).astype(np.uint8)
    changed = True
    iterations = 0
    maximum_iterations = max(image.shape) * 2
    while changed and iterations < maximum_iterations:
        changed = False
        for first_phase in (True, False):
            padded = np.pad(image, 1)
            p2 = padded[:-2, 1:-1]
            p3 = padded[:-2, 2:]
            p4 = padded[1:-1, 2:]
            p5 = padded[2:, 2:]
            p6 = padded[2:, 1:-1]
            p7 = padded[2:, :-2]
            p8 = padded[1:-1, :-2]
            p9 = padded[:-2, :-2]
            neighbours = p2 + p3 + p4 + p5 + p6 + p7 + p8 + p9
            transitions = (
                ((p2 == 0) & (p3 == 1)).astype(np.uint8)
                + ((p3 == 0) & (p4 == 1)).astype(np.uint8)
                + ((p4 == 0) & (p5 == 1)).astype(np.uint8)
                + ((p5 == 0) & (p6 == 1)).astype(np.uint8)
                + ((p6 == 0) & (p7 == 1)).astype(np.uint8)
                + ((p7 == 0) & (p8 == 1)).astype(np.uint8)
                + ((p8 == 0) & (p9 == 1)).astype(np.uint8)
                + ((p9 == 0) & (p2 == 1)).astype(np.uint8)
            )
            if first_phase:
                phase = (p2 * p4 * p6 == 0) & (p4 * p6 * p8 == 0)
            else:
                phase = (p2 * p4 * p8 == 0) & (p2 * p6 * p8 == 0)
            remove = (
                (image == 1)
                & (neighbours >= 2)
                & (neighbours <= 6)
                & (transitions == 1)
                & phase
            )
            if np.any(remove):
                image[remove] = 0
                changed = True
        iterations += 1
    return image


def derive_lines(
    dataset: gdal.Dataset,
    skeleton: np.ndarray,
    operation: dict[str, Any],
    output_dir: Path,
) -> Path:
    minimum_length = finite_number(operation.get("minimumLength"), "minimumLength")
    transform = dataset.GetGeoTransform()
    to_wgs84 = coordinate_transform_to_wgs84(dataset)
    pixels = {tuple(value) for value in np.argwhere(skeleton > 0)}
    components = []
    while pixels:
        seed = pixels.pop()
        component = {seed}
        queue = deque([seed])
        while queue:
            row, column = queue.popleft()
            for delta_row in (-1, 0, 1):
                for delta_column in (-1, 0, 1):
                    neighbour = (row + delta_row, column + delta_column)
                    if neighbour in pixels:
                        pixels.remove(neighbour)
                        component.add(neighbour)
                        queue.append(neighbour)
        components.append(component)
    features = []
    for component_index, component in enumerate(components):
        segments = []
        length = 0.0
        for row, column in sorted(component):
            start = transform_position(
                to_wgs84, pixel_center(transform, row, column)
            )
            for neighbour in ((row, column + 1), (row + 1, column - 1), (row + 1, column), (row + 1, column + 1)):
                if neighbour not in component:
                    continue
                end = transform_position(
                    to_wgs84, pixel_center(transform, *neighbour)
                )
                segments.append([start, end])
                length += segment_length(start, end)
        if segments and length >= minimum_length:
            features.append(
                {
                    "type": "Feature",
                    "id": str(component_index),
                    "properties": {"length_m": length},
                    "geometry": {"type": "MultiLineString", "coordinates": segments},
                }
            )
    output = output_dir / "derived-lines.geojson"
    output.write_text(
        json.dumps(
            {
                "type": "FeatureCollection",
                "features": features,
                "veoveo_algorithm": "zhang_suen_pixel_graph_v1",
            },
            separators=(",", ":"),
            sort_keys=True,
        ),
        encoding="utf-8",
    )
    return output


def pixel_center(transform: tuple[float, ...], row: int, column: int) -> list[float]:
    x = transform[0] + (column + 0.5) * transform[1] + (row + 0.5) * transform[2]
    y = transform[3] + (column + 0.5) * transform[4] + (row + 0.5) * transform[5]
    return [x, y]


def segment_length(start: list[float], end: list[float]) -> float:
    radius = 6_371_008.8
    latitude_1 = math.radians(start[1])
    latitude_2 = math.radians(end[1])
    delta_latitude = latitude_2 - latitude_1
    delta_longitude = math.radians(end[0] - start[0])
    value = (
        math.sin(delta_latitude / 2) ** 2
        + math.cos(latitude_1)
        * math.cos(latitude_2)
        * math.sin(delta_longitude / 2) ** 2
    )
    return 2 * radius * math.asin(math.sqrt(value))


def band_index(dataset: gdal.Dataset, operation: dict[str, Any]) -> int:
    value = positive_int(operation.get("band"), "band")
    if value > dataset.RasterCount:
        raise ContractError("raster band is unavailable")
    return value


def coordinate_transform_from_wgs84(dataset: gdal.Dataset) -> osr.CoordinateTransformation:
    source = osr.SpatialReference()
    source.ImportFromEPSG(4326)
    source.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
    target = osr.SpatialReference()
    target.ImportFromWkt(dataset.GetProjection())
    target.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
    return osr.CoordinateTransformation(source, target)


def coordinate_transform_to_wgs84(dataset: gdal.Dataset) -> osr.CoordinateTransformation:
    source = osr.SpatialReference()
    source.ImportFromWkt(dataset.GetProjection())
    source.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
    target = osr.SpatialReference()
    target.ImportFromEPSG(4326)
    target.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
    return osr.CoordinateTransformation(source, target)


def transform_position(
    transform: osr.CoordinateTransformation, position: list[float]
) -> list[float]:
    longitude, latitude, _ = transform.TransformPoint(position[0], position[1])
    return [longitude, latitude]


def confined_file(value: Any, field: str) -> Path:
    path = Path(controlled(value, field, 4096))
    if not path.is_absolute() or ".." in path.parts or not path.is_file():
        raise ContractError(f"{field} must be an absolute regular file")
    return path.resolve()


def confined_directory(value: Any, field: str) -> Path:
    path = Path(controlled(value, field, 4096))
    if not path.is_absolute() or ".." in path.parts:
        raise ContractError(f"{field} must be an absolute directory")
    path.mkdir(parents=True, exist_ok=True)
    return path.resolve()


def controlled(value: Any, field: str, maximum: int) -> str:
    if (
        not isinstance(value, str)
        or not value
        or len(value.encode("utf-8")) > maximum
        or any(ord(character) < 32 or ord(character) == 127 for character in value)
    ):
        raise ContractError(f"{field} is invalid")
    return value


def positive_int(value: Any, field: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
        raise ContractError(f"{field} must be positive")
    return value


def finite_number(value: Any, field: str) -> float:
    if not isinstance(value, (int, float)) or isinstance(value, bool) or not math.isfinite(value):
        raise ContractError(f"{field} must be finite")
    return float(value)


def positive_number(value: Any, field: str) -> float:
    number = finite_number(value, field)
    if number <= 0:
        raise ContractError(f"{field} must be positive")
    return number


def non_negative_number(value: Any, field: str) -> float:
    number = finite_number(value, field)
    if number < 0:
        raise ContractError(f"{field} must be non-negative")
    return number


def main() -> int:
    try:
        result = run(json.load(sys.stdin))
        sys.stdout.write(json.dumps(result, separators=(",", ":"), sort_keys=True) + "\n")
        return 0
    except Exception as error:
        sys.stderr.write(f"map-raster failed: {error}\n")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
