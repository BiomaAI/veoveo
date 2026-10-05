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
    from veoveo_mcp.types import ResourceUri

    profile = DatasetProfile(row_count=1, column_count=0, columns=[], correlations=[])
    artifact_id = ArtifactId(str(new_task_id()))
    artifact = ArtifactMetadata(artifact_id=artifact_id, byte_len=5, artifact_uri=str(uris.artifact_uri(artifact_id)), created_at=datetime.now(timezone.utc))
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
