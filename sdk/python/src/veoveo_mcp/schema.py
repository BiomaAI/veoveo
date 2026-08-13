"""JSON Schema 2020-12 generation for MCP tool inputs."""

from __future__ import annotations

from typing import Any

from pydantic import BaseModel

MCP_INPUT_SCHEMA_DIALECT = "https://json-schema.org/draft/2020-12/schema"
MAX_SCHEMA_DEPTH = 64
MAX_SCHEMA_NODES = 10_000
MAX_SCHEMA_REFERENCES = 512


def mcp_input_schema(model: type[BaseModel]) -> dict[str, Any]:
    """Generate the complete controlled schema without rewriting its vocabulary."""

    schema = model.model_json_schema(
        mode="validation", union_format="primitive_type_array"
    )
    schema["$schema"] = MCP_INPUT_SCHEMA_DIALECT
    schema.setdefault("type", "object")
    schema.pop("title", None)
    schema.pop("description", None)
    _validate_schema(schema, model)
    return schema


def _validate_schema(schema: dict[str, Any], model: type[BaseModel]) -> None:
    if schema.get("type") != "object":
        raise ValueError(f"MCP input schema for {model.__name__} must have an object root")

    nodes = 0
    references = 0

    def visit(value: Any, depth: int) -> None:
        nonlocal nodes, references
        nodes += 1
        if nodes > MAX_SCHEMA_NODES:
            raise ValueError(f"MCP input schema for {model.__name__} is too large")
        if depth > MAX_SCHEMA_DEPTH:
            raise ValueError(f"MCP input schema for {model.__name__} is too deeply nested")
        if isinstance(value, list):
            for item in value:
                visit(item, depth + 1)
            return
        if not isinstance(value, dict):
            return
        reference = value.get("$ref")
        if reference is not None:
            references += 1
            if references > MAX_SCHEMA_REFERENCES:
                raise ValueError(
                    f"MCP input schema for {model.__name__} has too many references"
                )
            if not isinstance(reference, str) or not reference.startswith("#/"):
                raise ValueError(
                    f"MCP input schema for {model.__name__} contains a non-local reference"
                )
            _resolve_json_pointer(schema, reference)
        for item in value.values():
            visit(item, depth + 1)

    visit(schema, 0)


def _resolve_json_pointer(root: dict[str, Any], reference: str) -> None:
    current: Any = root
    for raw_token in reference[2:].split("/"):
        token = raw_token.replace("~1", "/").replace("~0", "~")
        if not isinstance(current, dict) or token not in current:
            raise ValueError(f"MCP input schema contains unknown reference {reference!r}")
        current = current[token]
