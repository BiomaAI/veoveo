from datasheet_mcp.contract import (
    ColumnStatsRequest,
    PreviewDatasetRequest,
    ProfileDatasetRequest,
)
from veoveo_mcp.schema import MCP_INPUT_SCHEMA_DIALECT, mcp_input_schema


def test_every_tool_input_uses_the_canonical_schema_profile():
    for request in (PreviewDatasetRequest, ColumnStatsRequest, ProfileDatasetRequest):
        schema = mcp_input_schema(request)
        assert schema["$schema"] == MCP_INPUT_SCHEMA_DIALECT
        assert schema["type"] == "object"


def test_profile_product_schema_decoder_and_completion_agree():
    from datetime import datetime, timezone
    import pytest
    from jsonschema import Draft202012Validator
    from pydantic import ValidationError
    from datasheet_mcp.contract import DatasetProfile, ProfileDatasetOutput
    from datasheet_mcp import uris
    from veoveo_mcp.contract import ArtifactMetadata
    from veoveo_mcp.contract.artifacts import ArtifactId
    from veoveo_mcp.tasks import new_task_id, mcp_task_completion
    from veoveo_mcp.types import ChronoTimestamp, ResourceUri

    profile = DatasetProfile(row_count=1, column_count=0, columns=[], correlations=[])
    artifact_id = ArtifactId(str(new_task_id()))
    artifact = ArtifactMetadata(artifactId=artifact_id, byteLen=5, artifactUri=str(uris.artifact_uri(artifact_id)), createdAt=ChronoTimestamp.from_datetime(datetime.now(timezone.utc)))
    inline = ProfileDatasetOutput(profile=profile).model_dump(mode="json", by_alias=True, exclude_none=True)
    product = ProfileDatasetOutput(profile=profile, artifact=artifact, resultUri=ResourceUri(artifact.artifact_uri)).model_dump(mode="json", by_alias=True, exclude_none=True)
    schema = ProfileDatasetOutput.model_json_schema()
    validator = Draft202012Validator(schema)
    for value in [inline, product]:
        assert validator.is_valid(value)
        assert ProfileDatasetOutput.model_validate(value).model_dump(mode="json", by_alias=True, exclude_none=True) == value
    malformed = [
        {**inline, "resultUri": None},
        {**inline, "resultUri": artifact.artifact_uri},
        {key: value for key, value in product.items() if key != "resultUri"},
        {**product, "resultUri": None},
        {**{key: value for key, value in product.items() if key != "resultUri"}, "result_uri": artifact.artifact_uri},
        {**product, "result_uri": artifact.artifact_uri},
        {**product, "result_uri": None},
        {**inline, "result_uri": None},
    ]
    for value in malformed:
        assert not validator.is_valid(value), value
        with pytest.raises(ValidationError):
            ProfileDatasetOutput.model_validate(value)
    with pytest.raises(ValidationError):
        ProfileDatasetOutput.model_validate({**product, "resultUri": str(uris.artifact_uri(ArtifactId(str(new_task_id()))))})
    for value, content in [(inline, []), (product, [{"type": "resource_link", "uri": artifact.artifact_uri, "name": "report"}])]:
        transition = mcp_task_completion("profile complete", {"content": content, "structuredContent": value})
        assert transition.result_uri() == (ResourceUri(artifact.artifact_uri) if content else None)


def test_actual_rust_artifact_metadata_survives_template_product_and_completion():
    from pathlib import Path
    import json
    import pytest
    from pydantic import ValidationError
    from datasheet_mcp.contract import DatasetProfile, ProfileDatasetOutput
    from veoveo_mcp.contract import ArtifactMetadata
    from veoveo_mcp.tasks import mcp_task_completion
    from veoveo_mcp.types import ChronoTimestamp, ResourceUri

    source = Path(__file__).resolve().parents[3] / "platform/artifacts/contract/tests/fixtures/metadata-output.json"
    artifact = ArtifactMetadata.model_validate_json(source.read_bytes()).presented_under_scheme("datasheet").without_download_url()
    output = ProfileDatasetOutput(profile=DatasetProfile(row_count=1, column_count=0, columns=[], correlations=[]), artifact=artifact, resultUri=ResourceUri(artifact.artifact_uri))
    envelope = {"content": [{"type": "resource_link", "uri": artifact.artifact_uri, "name": "profile"}], "structuredContent": json.loads(output.model_dump_json(by_alias=True, exclude_none=True))}
    assert mcp_task_completion("profile complete", envelope).result_uri() == ResourceUri(artifact.artifact_uri)
    corrupt = {**envelope["structuredContent"], "artifact": {**artifact.model_dump(mode="json"), "byte_len": -1}}
    with pytest.raises(ValidationError):
        ProfileDatasetOutput.model_validate(corrupt)


async def test_actual_catalog_and_tool_handlers_share_current_owner_wire():
    from types import SimpleNamespace
    from jsonschema import Draft202012Validator
    import mcp.types as types
    from datasheet_mcp.server.mcp_server import build_mcp_server
    server = build_mcp_server(SimpleNamespace())
    catalog = await server.get_request_handler("tools/list").handler(None, None)
    tools = {tool.name: tool for tool in catalog.tools}
    assert set(tools) == {"preview_dataset", "column_stats", "profile_dataset"}
    call = server.get_request_handler("tools/call").handler
    csv = "name,value\na,1\nb,2\n"
    for name, fields in [("preview_dataset", {"rows": 1}), ("column_stats", {"column": "value"})]:
        args = {"inlineCsv": csv, **fields}
        assert Draft202012Validator(tools[name].input_schema).is_valid(args)
        result = await call(None, types.CallToolRequestParams(name=name, arguments=args))
        assert not result.is_error
        assert Draft202012Validator(tools[name].output_schema).is_valid(result.structured_content)
        assert "row_count" not in result.structured_content
        if name == "preview_dataset":
            assert result.structured_content["rowCount"] == 2
        else:
            assert {"nullCount", "distinctCount", "topValues"} <= result.structured_content.keys()
        for bad in [{"inline_csv": csv, **fields}, {**args, "inline_csv": csv}, {**args, "unknownField": True}]:
            assert not Draft202012Validator(tools[name].input_schema).is_valid(bad)
            assert (await call(None, types.CallToolRequestParams(name=name, arguments=bad))).is_error
    profile_args = {"inlineCsv": csv, "histogramBins": 4}
    assert Draft202012Validator(tools["profile_dataset"].input_schema).is_valid(profile_args)
    assert not Draft202012Validator(tools["profile_dataset"].input_schema).is_valid({**profile_args, "histogram_bins": 4})


def test_nested_profile_and_request_decoders_refuse_old_mixed_unknown_and_missing():
    import copy
    import pytest
    from pydantic import ValidationError
    from datasheet_mcp import engine
    from datasheet_mcp.contract import DatasetProfile, ProfileDatasetOutput
    csv = "name,value\na,1\nb,2\n"
    profile = engine.profile(engine.load_inline_csv(csv), histogram_bins=2)
    value = ProfileDatasetOutput(profile=profile).model_dump(mode="json", exclude_none=True)
    assert "rowCount" in value["profile"] and "columnCount" in value["profile"]
    assert {"nullCount", "distinctCount", "topValues"} <= value["profile"]["columns"][0].keys()
    assert ProfileDatasetOutput.model_validate_json(__import__("json").dumps(value)).profile == profile
    assert DatasetProfile.model_validate(value["profile"]) == profile
    for path, field, retired in [("profile", "rowCount", "row_count"), ("profile", "columnCount", "column_count"),
                                  ("column", "nullCount", "null_count"), ("column", "distinctCount", "distinct_count"),
                                  ("column", "topValues", "top_values")]:
        for mode in ("old", "mixed", "missing", "unknown"):
            bad = copy.deepcopy(value)
            selected = bad["profile"] if path == "profile" else bad["profile"]["columns"][0]
            if mode in ("old", "mixed"):
                selected[retired] = selected[field]
                if mode == "old": del selected[field]
            elif mode == "missing":
                if field == "topValues": continue  # optional output collection has declared default
                del selected[field]
            else: selected["unknownField"] = 1
            with pytest.raises(ValidationError): ProfileDatasetOutput.model_validate(bad)
    for request in (PreviewDatasetRequest, ColumnStatsRequest, ProfileDatasetRequest):
        args = {"inlineCsv": csv, **({"column": "value"} if request is ColumnStatsRequest else {})}
        assert request.model_validate(args).inline_csv == csv
        for bad in [{"inline_csv": csv}, {**args, "inline_csv": csv}, {**args, "unknownField": 1}, {}]:
            with pytest.raises(ValidationError): request.model_validate(bad)
