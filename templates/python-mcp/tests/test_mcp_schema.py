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
        assert "$defs" not in schema
        assert "$ref" not in str(schema)
        assert all("type" in value for value in schema["properties"].values())
