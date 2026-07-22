from typing import Annotated, Literal

import pytest
from pydantic import BaseModel, Field

from veoveo_mcp.schema import MCP_INPUT_SCHEMA_DIALECT, mcp_input_schema


class Nested(BaseModel):
    value: str


class Request(BaseModel):
    nested: Nested
    optional: str | None = None


class FirstChoice(BaseModel):
    kind: Literal["first"]
    value: str


class SecondChoice(BaseModel):
    kind: Literal["second"]
    value: int


class UnionRequest(BaseModel):
    choice: Annotated[FirstChoice | SecondChoice, Field(discriminator="kind")]


class RecursiveRequest(BaseModel):
    child: "RecursiveRequest | None" = None


def test_mcp_input_schema_is_self_contained_and_explicitly_typed():
    schema = mcp_input_schema(Request)

    assert schema["$schema"] == MCP_INPUT_SCHEMA_DIALECT
    assert schema["type"] == "object"
    assert "$defs" not in schema
    assert "$ref" not in str(schema)
    assert schema["properties"]["nested"]["type"] == "object"
    assert schema["properties"]["optional"]["type"] == ["string", "null"]


def test_mcp_input_schema_exposes_discriminated_union_as_an_object():
    schema = mcp_input_schema(UnionRequest)

    assert schema["properties"]["choice"]["type"] == "object"


def test_mcp_input_schema_rejects_recursive_tool_inputs():
    with pytest.raises(ValueError, match="recursive"):
        mcp_input_schema(RecursiveRequest)
