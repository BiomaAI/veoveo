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
    inline = ProfileDatasetOutput(profile=profile).model_dump(mode="json", exclude_none=True)
    product = ProfileDatasetOutput(profile=profile, artifact=artifact, result_uri=ResourceUri(artifact.artifact_uri)).model_dump(mode="json", exclude_none=True)
    schema = ProfileDatasetOutput.model_json_schema()
    validator = Draft202012Validator(schema)
    for value in [inline, product]:
        assert validator.is_valid(value)
        assert ProfileDatasetOutput.model_validate(value).model_dump(mode="json", exclude_none=True) == value
    malformed = [
        {**inline, "result_uri": None},
        {**inline, "result_uri": artifact.artifact_uri},
        {key: value for key, value in product.items() if key != "result_uri"},
        {**product, "result_uri": None},
    ]
    for value in malformed:
        assert not validator.is_valid(value), value
        with pytest.raises(ValidationError):
            ProfileDatasetOutput.model_validate(value)
    with pytest.raises(ValidationError):
        ProfileDatasetOutput.model_validate({**product, "result_uri": str(uris.artifact_uri(ArtifactId(str(new_task_id()))))})
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
    output = ProfileDatasetOutput(profile=DatasetProfile(row_count=1, column_count=0, columns=[], correlations=[]), artifact=artifact, result_uri=ResourceUri(artifact.artifact_uri))
    envelope = {"content": [{"type": "resource_link", "uri": artifact.artifact_uri, "name": "profile"}], "structuredContent": json.loads(output.model_dump_json(exclude_none=True))}
    assert mcp_task_completion("profile complete", envelope).result_uri() == ResourceUri(artifact.artifact_uri)
    corrupt = {**envelope["structuredContent"], "artifact": {**artifact.model_dump(mode="json"), "byte_len": -1}}
    with pytest.raises(ValidationError):
        ProfileDatasetOutput.model_validate(corrupt)
