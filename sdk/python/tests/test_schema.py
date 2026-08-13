from typing import Annotated, Literal

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


def test_mcp_input_schema_preserves_complete_2020_12_shape():
    schema = mcp_input_schema(Request)

    assert schema["$schema"] == MCP_INPUT_SCHEMA_DIALECT
    assert schema["type"] == "object"
    assert schema["properties"]["nested"]["$ref"] == "#/$defs/Nested"
    assert schema["$defs"]["Nested"]["type"] == "object"
    assert schema["properties"]["optional"]["type"] == ["string", "null"]


def test_mcp_input_schema_preserves_discriminated_composition():
    schema = mcp_input_schema(UnionRequest)

    choice = schema["properties"]["choice"]
    assert len(choice["oneOf"]) == 2
    assert choice["discriminator"]["propertyName"] == "kind"


def test_mcp_input_schema_supports_bounded_local_recursion():
    schema = mcp_input_schema(RecursiveRequest)
    assert schema["$defs"]["RecursiveRequest"]["properties"]["child"]["anyOf"][0] == {
        "$ref": "#/$defs/RecursiveRequest"
    }
