"""Canonical client-facing JSON Schema generation for MCP tool inputs."""

from __future__ import annotations

from copy import deepcopy
from typing import Any

from pydantic import BaseModel

MCP_INPUT_SCHEMA_DIALECT = "https://json-schema.org/draft/2020-12/schema"


def mcp_input_schema(model: type[BaseModel]) -> dict[str, Any]:
    """Generate a self-contained, explicitly typed MCP input schema."""

    schema = model.model_json_schema(union_format="primitive_type_array")
    definitions = schema.pop("$defs", {})
    schema = _inline_references(schema, definitions, ())
    schema["$schema"] = MCP_INPUT_SCHEMA_DIALECT
    schema.pop("title", None)
    schema.pop("description", None)
    _expose_union_types(schema)
    _validate_input_schema(schema, model)
    return schema


def _inline_references(
    value: Any, definitions: dict[str, Any], stack: tuple[str, ...]
) -> Any:
    if isinstance(value, list):
        return [_inline_references(item, definitions, stack) for item in value]
    if not isinstance(value, dict):
        return value

    reference = value.get("$ref")
    if reference is not None:
        prefix = "#/$defs/"
        if not isinstance(reference, str) or not reference.startswith(prefix):
            raise ValueError(f"MCP input schema contains unsupported reference {reference!r}")
        name = reference.removeprefix(prefix)
        if name in stack:
            chain = " -> ".join((*stack, name))
            raise ValueError(f"MCP input schema is recursive: {chain}")
        if name not in definitions:
            raise ValueError(f"MCP input schema references unknown definition {name!r}")
        expanded = _inline_references(
            deepcopy(definitions[name]), definitions, (*stack, name)
        )
        siblings = {
            key: _inline_references(item, definitions, stack)
            for key, item in value.items()
            if key != "$ref"
        }
        expanded.update(siblings)
        return expanded

    return {
        key: _inline_references(item, definitions, stack)
        for key, item in value.items()
    }


def _expose_union_types(value: Any) -> None:
    if isinstance(value, list):
        for item in value:
            _expose_union_types(item)
        return
    if not isinstance(value, dict):
        return

    for item in value.values():
        _expose_union_types(item)
    if "type" in value:
        return
    for keyword in ("oneOf", "anyOf"):
        variants = value.get(keyword)
        if not isinstance(variants, list) or not variants:
            continue
        types: list[str] = []
        for variant in variants:
            declared = _declared_types(variant)
            if declared is None:
                types = []
                break
            for declared_type in declared:
                if declared_type not in types:
                    types.append(declared_type)
        if types:
            value["type"] = types[0] if len(types) == 1 else types
            return


def _declared_types(schema: Any) -> list[str] | None:
    if not isinstance(schema, dict):
        return None
    declared = schema.get("type")
    if isinstance(declared, str):
        return [declared]
    if isinstance(declared, list) and all(isinstance(value, str) for value in declared):
        return declared
    return None


def _validate_input_schema(schema: dict[str, Any], model: type[BaseModel]) -> None:
    if schema.get("type") != "object":
        raise ValueError(f"MCP input schema for {model.__name__} must have an object root")
    if _contains_key(schema, "$ref"):
        raise ValueError(f"MCP input schema for {model.__name__} must be self-contained")
    _validate_property_types(schema, model, "$")


def _validate_property_types(
    value: Any, model: type[BaseModel], path: str
) -> None:
    if isinstance(value, list):
        for index, item in enumerate(value):
            _validate_property_types(item, model, f"{path}/{index}")
        return
    if not isinstance(value, dict):
        return
    properties = value.get("properties", {})
    for name, property_schema in properties.items():
        if "type" not in property_schema:
            raise ValueError(
                f"MCP input schema for {model.__name__} must expose the JSON type "
                f"at {path}/properties/{name}"
            )
    for name, item in value.items():
        _validate_property_types(item, model, f"{path}/{name}")


def _contains_key(value: Any, key: str) -> bool:
    if isinstance(value, dict):
        return key in value or any(_contains_key(item, key) for item in value.values())
    if isinstance(value, list):
        return any(_contains_key(item, key) for item in value)
    return False
