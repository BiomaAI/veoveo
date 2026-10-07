from __future__ import annotations

import json
from pathlib import Path

import pytest

from map_data.feature_package import (
    ContractError,
    _property_schema,
    _sequence,
    _validate_command,
)


def _write_sequence(path: Path, features: list[dict[str, object]]) -> None:
    with path.open("wb") as output:
        for feature in features:
            output.write(b"\x1e")
            output.write(json.dumps(feature, separators=(",", ":")).encode())
            output.write(b"\n")


def test_command_contract_is_versioned_and_closed(tmp_path: Path, monkeypatch) -> None:
    from map_data import feature_package

    # These are the complete fields serialized by the three Rust command variants.
    common = {"schemaVersion": 2, "sourcePath": str(tmp_path / "source")}
    inspect = {**common, "operation": "inspect"}
    encode = {**common, "operation": "encode", "outputDir": str(tmp_path / "encoded"),
              "maximumOutputBytes": 1024, "table": "features"}
    decode = {**encode, "operation": "decode", "outputDir": str(tmp_path / "decoded"),
              "maximumFeatures": 10, "defaultSemanticType": "facility",
              "identityColumn": None, "semanticTypeColumn": None, "titleColumn": None,
              "validFromColumn": None, "validUntilColumn": None}
    for current in [inspect, encode, decode]:
        assert _validate_command(current) == current
    # Nullable decode options also admit omission, matching their reader semantics.
    assert _validate_command({key: value for key, value in decode.items() if value is not None})

    def forbidden_effect(*args, **kwargs):
        raise AssertionError("invalid command reached file/GDAL dispatch")

    for operation in ["inspect", "decode", "encode"]:
        monkeypatch.setattr(feature_package, operation, forbidden_effect)
    for current in [inspect, encode, decode]:
        required = set(current) - {"identityColumn", "semanticTypeColumn", "titleColumn",
                                   "validFromColumn", "validUntilColumn"}
        for key in required:
            bad = dict(current)
            del bad[key]
            with pytest.raises(ContractError):
                feature_package.execute(bad)
        for key in current:
            if any(character.isupper() for character in key):
                retired = "".join("_" + c.lower() if c.isupper() else c for c in key)
                for mixed in [False, True]:
                    bad = dict(current)
                    bad[retired] = current[key]
                    if not mixed:
                        del bad[key]
                    with pytest.raises(ContractError):
                        feature_package.execute(bad)
        for extra in [{"undeclared": True}, {"schemaVersion": 1}, {"schemaVersion": 2.0},
                      {"sourcePath": "relative"}]:
            with pytest.raises(ContractError):
                feature_package.execute({**current, **extra})
    for current in [encode, decode]:
        for invalid in [{"table": ""}, {"table": None}, {"maximumOutputBytes": 0},
                        {"maximumOutputBytes": True}, {"outputDir": "relative"}]:
            with pytest.raises(ContractError):
                feature_package.execute({**current, **invalid})
    for invalid in [{"maximumFeatures": 0}, {"maximumFeatures": 10001},
                    {"defaultSemanticType": ""}, {"identityColumn": []}]:
        with pytest.raises(ContractError):
            feature_package.execute({**decode, **invalid})
    for wrong_operation_field in ["maximumFeatures", "defaultSemanticType", "identityColumn"]:
        with pytest.raises(ContractError, match="unsupported field"):
            feature_package.execute({**encode, wrong_operation_field: decode[wrong_operation_field]})
    with pytest.raises(ContractError, match="unsupported field"):
        feature_package.execute({**inspect, "table": "features"})
    with pytest.raises(ContractError, match="inspect, decode, or encode"):
        feature_package.execute({**common, "operation": "convert_everything"})
    assert not (tmp_path / "encoded").exists()
    assert not (tmp_path / "decoded").exists()


def test_geojson_sequence_and_property_schema_are_bounded(tmp_path: Path) -> None:
    source = tmp_path / "features.geojsons"
    _write_sequence(
        source,
        [
            {
                "type": "Feature",
                "geometry": {"type": "Point", "coordinates": [0, 0]},
                "properties": {"name": "alpha", "count": 1, "nested": {"a": True}},
            },
            {
                "type": "Feature",
                "geometry": {"type": "Point", "coordinates": [1, 1]},
                "properties": {"name": None, "count": 2, "nested": [1, 2]},
            },
        ],
    )
    assert len(list(_sequence(source, source.stat().st_size))) == 2
    assert _property_schema(source, source.stat().st_size) == {
        "name": "string",
        "count": "integer64",
        "nested": "json",
    }
    with pytest.raises(ContractError, match="configured byte limit"):
        list(_sequence(source, source.stat().st_size - 1))


def test_geojson_sequence_requires_record_separators(tmp_path: Path) -> None:
    source = tmp_path / "bad.geojsons"
    source.write_text('{"type":"Feature"}\n')
    with pytest.raises(ContractError, match="ASCII RS"):
        list(_sequence(source, source.stat().st_size))
