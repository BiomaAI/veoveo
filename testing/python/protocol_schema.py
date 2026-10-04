"""Producer-to-consumer checks for the repository's acyclic private JSON schemas.

This is a restricted structural qualification utility, not a JSON Schema validator.
Unsupported assertions fail rather than disappearing from the comparison.
"""
from __future__ import annotations

from decimal import Decimal
from pathlib import Path
import json

ANNOTATIONS = {"$schema", "$id", "title", "description", "examples", "deprecated", "readOnly", "writeOnly", "discriminator"}
ASSERTIONS = {"$ref", "$defs", "type", "const", "enum", "anyOf", "oneOf", "properties", "required", "additionalProperties", "propertyNames", "items", "prefixItems", "minItems", "maxItems", "minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum", "minLength", "maxLength", "pattern", "format", "default"}


class SchemaMismatch(AssertionError):
    pass


def _error(path: str, message: str) -> None:
    raise SchemaMismatch(f"{path}: {message}")


def _resolve(schema: dict | bool, root: dict, path: str, trail: tuple[str, ...] = ()) -> dict | bool:
    if isinstance(schema, bool):
        return schema
    unknown = set(schema) - ANNOTATIONS - ASSERTIONS
    if unknown:
        _error(path, f"unsupported schema assertions {sorted(unknown)}")
    if "$ref" not in schema:
        return schema
    reference = schema["$ref"]
    if not reference.startswith("#/$defs/") or reference in trail:
        _error(path, "only acyclic local definition references are supported")
    definition = root["$defs"][reference.removeprefix("#/$defs/").replace("~1", "/").replace("~0", "~")]
    siblings = {key: value for key, value in schema.items() if key != "$ref"}
    if set(siblings) - ANNOTATIONS - {"default"}:
        _error(path, "assertion siblings of references require explicit support")
    resolved = _resolve(definition, root, path, (*trail, reference))
    if "default" in siblings:
        if isinstance(resolved, bool):
            _error(path, "defaults on boolean references are unsupported")
        return {**resolved, "default": siblings["default"]}
    return resolved


def _kind(schema: dict) -> str | None:
    if "type" in schema:
        return schema["type"]
    values = schema.get("enum", [schema["const"]] if "const" in schema else [])
    if values:
        kinds = {"null" if value is None else "boolean" if isinstance(value, bool) else "integer" if isinstance(value, int) else "number" if isinstance(value, float) else "string" if isinstance(value, str) else "unsupported" for value in values}
        if len(kinds) == 1:
            return kinds.pop()
    return None


def _inspect(schema: dict | bool, root: dict, path: str, trail: tuple[str, ...] = ()) -> None:
    if isinstance(schema, bool):
        return
    resolved = _resolve(schema, root, path, trail)
    if "$ref" in schema:
        _inspect(resolved, root, path, (*trail, schema["$ref"]))
        return
    if "enum" in schema or "const" in schema:
        values = schema.get("enum", [schema["const"]] if "const" in schema else [])
        value_kinds = {"null" if value is None else "boolean" if type(value) is bool else "integer" if type(value) is int else "number" if type(value) is float else "string" if type(value) is str else "nonscalar" for value in values}
        if len(value_kinds) != 1 or "nonscalar" in value_kinds:
            _error(path, "mixed-kind or nonscalar constants require explicit qualification")
        if "type" in schema:
            declared = schema["type"] if isinstance(schema["type"], list) else [schema["type"]]
            actual = next(iter(value_kinds))
            if actual not in declared and not (actual == "integer" and "number" in declared):
                _error(path, "constant kind contradicts the declared wire type")
    if "prefixItems" in schema:
        _error(path, "tuple-array assertions require explicit qualification")
    for name, child in schema.get("properties", {}).items():
        _inspect(child, root, path + "." + name, trail)
    for name in ("items", "additionalProperties", "propertyNames"):
        if name in schema:
            _inspect(schema[name], root, path + "." + name, trail)
    for name in ("anyOf", "oneOf"):
        for child in schema.get(name, []):
            _inspect(child, root, path, trail)
    if "oneOf" in schema:
        groups = [_branches(child, root, path) for child in schema["oneOf"]]
        # Exclusive alternatives must stay disjoint after nullable expansion.
        for index, group in enumerate(groups):
            for other in groups[index + 1:]:
                for left in group:
                    for right in other:
                        if isinstance(left, bool) or isinstance(right, bool):
                            _error(path, "unqualified exclusive union")
                        if _kind(left) is None or _kind(right) is None:
                            _error(path, "untyped exclusive-union alternatives require explicit qualification")
                        if _kind(left) != _kind(right) and {_kind(left), _kind(right)} != {"number", "integer"}:
                            continue
                        lv, rv = _scalar_values(left, path), _scalar_values(right, path)
                        if lv is not None and rv is not None and not any(value in rv for value in lv):
                            continue
                        for key in set(left.get("required", [])) & set(right.get("required", [])):
                            lfield = _resolve(left.get("properties", {}).get(key, True), root, path)
                            rfield = _resolve(right.get("properties", {}).get(key, True), root, path)
                            if isinstance(lfield, dict) and isinstance(rfield, dict):
                                lv, rv = _scalar_values(lfield, path), _scalar_values(rfield, path)
                                if lv is not None and rv is not None and not any(value in rv for value in lv):
                                    break
                        else:
                            _error(path, "oneOf alternatives are not demonstrably disjoint")
    # Check formats even when a broader consumer admits the whole shape.
    for branch in _branches(schema, root, path):
        if isinstance(branch, dict):
            assertions = set(branch) - ANNOTATIONS - {"$defs", "default"}
            if assertions and _kind(branch) not in {"null", "boolean", "integer", "number", "string", "object", "array"}:
                _error(path, "untyped or mixed-kind assertions require explicit qualification")
            _compare_scalar(branch, {}, path)


def _branches(schema: dict | bool, root: dict, path: str) -> list[dict | bool]:
    schema = _resolve(schema, root, path)
    if isinstance(schema, bool):
        return [schema]
    for union in ("anyOf", "oneOf"):
        if union in schema:
            if set(schema) - ANNOTATIONS - {union, "$defs", "default"}:
                _error(path, "union assertion siblings require explicit support")
            return [branch for entry in schema[union] for branch in _branches(entry, root, path)]
    if isinstance(schema.get("type"), list):
        return [{**schema, "type": kind} for kind in schema["type"]]
    return [schema]


def _bounds(schema: dict, path: str) -> tuple[tuple[Decimal | None, bool], tuple[Decimal | None, bool]]:
    lower = (Decimal(str(schema["minimum"])), False) if "minimum" in schema else (None, False)
    upper = (Decimal(str(schema["maximum"])), False) if "maximum" in schema else (None, False)
    if "exclusiveMinimum" in schema:
        bound = (Decimal(str(schema["exclusiveMinimum"])), True)
        if lower[0] is None or bound[0] >= lower[0]:
            lower = bound
    if "exclusiveMaximum" in schema:
        bound = (Decimal(str(schema["exclusiveMaximum"])), True)
        if upper[0] is None or bound[0] <= upper[0]:
            upper = bound
    values = schema.get("enum", [schema["const"]] if "const" in schema else [])
    if values and all(type(value) in (int, float) for value in values):
        enum_lower, enum_upper = Decimal(str(min(values))), Decimal(str(max(values)))
        if lower[0] is None or lower[0] < enum_lower:
            lower = (enum_lower, False)
        if upper[0] is None or upper[0] > enum_upper:
            upper = (enum_upper, False)
    spelling = schema.get("format", "")
    if spelling.startswith(("uint", "int")):
        unsigned = spelling.startswith("uint")
        digits = spelling[4:] if unsigned else spelling[3:]
        if not digits.isdigit() or int(digits) not in {8, 16, 32, 64, 128}:
            _error(path, f"unsupported integer format {spelling}")
        bits = int(digits)
        minimum = Decimal(0 if unsigned else -(2**(bits-1)))
        maximum = Decimal(2**bits-1 if unsigned else 2**(bits-1)-1)
        if lower[0] is None or lower[0] < minimum:
            lower = (minimum, False)
        if upper[0] is None or upper[0] > maximum:
            upper = (maximum, False)
    return lower, upper


def _included_bound(producer: tuple, consumer: tuple, lower: bool) -> bool:
    p, p_exclusive = producer
    c, c_exclusive = consumer
    if c is None:
        return True
    if p is None:
        return False
    return (p > c if lower else p < c) or (p == c and (p_exclusive or not c_exclusive))


def _scalar_values(schema: dict, path: str) -> list | None:
    if "const" in schema:
        return [schema["const"]]
    if "enum" in schema:
        return schema["enum"]
    if _kind(schema) in ("number", "integer"):
        lower, upper = _bounds(schema, path)
        if lower[0] is not None and lower[0] == upper[0] and not lower[1] and not upper[1]:
            return [int(lower[0]) if schema["type"] == "integer" else float(lower[0])]
    return None


def _compare_scalar(producer: dict, consumer: dict, path: str) -> None:
    p_values, c_values = _scalar_values(producer, path), _scalar_values(consumer, path)
    if c_values is not None and (p_values is None or any(value not in c_values for value in p_values)):
        _error(path, "producer enum/constant exceeds consumer admission")
    kind = _kind(producer)
    if kind in {"number", "integer"}:
        p_lower, p_upper = _bounds(producer, path)
        c_lower, c_upper = _bounds(consumer, path)
        if not _included_bound(p_lower, c_lower, True) or not _included_bound(p_upper, c_upper, False):
            _error(path, "producer numeric range exceeds consumer admission")
        # JSON numbers are decoded as f64 in Python. Rust f32 production is
        # admitted there; a generic f64 producer cannot promise an f32 consumer.
        if consumer.get("format") == "float" and producer.get("format") != "float":
            _error(path, "f64 producer lacks an f32 wire profile")
    limits = (("minLength", "maxLength"),) if kind == "string" else (("minItems", "maxItems"),) if kind == "array" else ()
    for lower, upper in limits:
        if producer.get(lower, 0) < consumer.get(lower, 0) or producer.get(upper, float("inf")) > consumer.get(upper, float("inf")):
            _error(path, f"producer {lower}/{upper} exceeds consumer admission")
    if kind == "string" and "pattern" in consumer and producer.get("pattern") != consumer["pattern"]:
        _error(path, "pattern inclusion requires the same declared pattern")
    for profile in (producer, consumer):
        spelling = profile.get("format")
        profile_kind = _kind(profile)
        if profile_kind == "null":
            continue
        if spelling and spelling not in {"float", "double", "date-time", "uuid", "path", "binary"} and not spelling.startswith(("int", "uint")):
            _error(path, f"unsupported format {spelling}")
        if spelling and spelling.startswith(("int", "uint")):
            if profile_kind != "integer":
                _error(path, "integer formats require an integer wire type")
            _bounds(profile, path)
        if spelling in {"float", "double"} and profile_kind != "number":
            _error(path, "floating formats require a number wire type")
        if spelling in {"date-time", "uuid", "path"} and profile_kind != "string":
            _error(path, "text formats require a string wire type")
    if kind == "string" and consumer.get("format") in {"date-time", "uuid"} and producer.get("format") != consumer["format"]:
        _error(path, "producer lacks the consumer's date/UUID profile")
    if consumer.get("format") == "binary" or producer.get("format") == "binary":
        _error(path, "binary format requires a qualified JSON encoding profile")


def _compatible(producer: dict | bool, consumer: dict | bool, p_root: dict, c_root: dict, path: str) -> None:
    p_branches, c_branches = _branches(producer, p_root, path), _branches(consumer, c_root, path)
    if len(p_branches) != 1 or len(c_branches) != 1:
        for p_branch in p_branches:
            failures = []
            for c_branch in c_branches:
                try:
                    _compatible(p_branch, c_branch, p_root, c_root, path)
                    break
                except SchemaMismatch as failure:
                    failures.append(str(failure))
            else:
                _error(path, "no consumer union alternative admits producer branch: " + " | ".join(failures))
        return
    p, c = p_branches[0], c_branches[0]
    if isinstance(p, dict) and not (set(p) - ANNOTATIONS - {"$defs"}):
        p = True
    if isinstance(c, dict) and not (set(c) - ANNOTATIONS - {"$defs"}):
        c = True
    if c is True or p is False:
        return
    if p is True or c is False:
        _error(path, "unconstrained producer is not admitted by consumer")
    p_kind, c_kind = _kind(p), _kind(c)
    if p_kind != c_kind and not (p_kind == "integer" and c_kind == "number"):
        _error(path, f"producer type {p_kind} differs from consumer {c_kind}")
    _compare_scalar(p, c, path)
    if p_kind == "object":
        p_props, c_props = p.get("properties", {}), c.get("properties", {})
        if set(c.get("required", [])) - set(p.get("required", [])):
            _error(path, "producer may omit a consumer-required field")
        p_extra, c_extra = p.get("additionalProperties", True), c.get("additionalProperties", True)
        for name, p_field in p_props.items():
            c_field = c_props.get(name, c_extra)
            _compatible(p_field, c_field, p_root, c_root, f"{path}.{name}")
            p_default = _resolve(p_field, p_root, path)
            c_default = _resolve(c_props.get(name, True), c_root, path)
            if isinstance(p_default, dict) and isinstance(c_default, dict) and "default" in p_default and "default" in c_default and p_default["default"] != c_default["default"]:
                _error(f"{path}.{name}", "peer defaults disagree")
        if p_extra is not False:
            key_schema = _resolve(p.get("propertyNames", True), p_root, path + ".<key>")
            key_values = _scalar_values(key_schema, path) if isinstance(key_schema, dict) else None
            finite_keys = key_values is not None and all(isinstance(key, str) for key in key_values)
            if finite_keys and set(key_schema) - ANNOTATIONS - {"type", "enum", "const", "default"}:
                _error(path + ".<key>", "finite map-key assertion intersections require explicit qualification")
            for name, c_field in c_props.items():
                if name not in p_props and (not finite_keys or name in key_values):
                    _compatible(p_extra, c_field, p_root, c_root, f"{path}.{name}")
            if finite_keys:
                for name in key_values:
                    if name not in p_props:
                        _compatible(p_extra, c_props.get(name, c_extra), p_root, c_root, f"{path}.{name}")
            else:
                if c_extra is False:
                    _error(path, "open producer can emit undeclared consumer fields")
                _compatible(p_extra, c_extra, p_root, c_root, path + ".*")
        if "propertyNames" in c:
            for name in p_props:
                _compatible({"type": "string", "const": name}, c["propertyNames"], p_root, c_root, path + ".<key>")
            if p_extra is not False and "propertyNames" not in p:
                _error(path, "producer lacks consumer map-key admission")
            if p_extra is not False:
                _compatible(p["propertyNames"], c["propertyNames"], p_root, c_root, path + ".<key>")
    elif p_kind == "array":
        if "prefixItems" in p or "prefixItems" in c:
            _error(path, "tuple-array assertions require explicit qualification")
        _compatible(p.get("items", True), c.get("items", True), p_root, c_root, path + "[]")


def assert_compatible(producer: dict, consumer: dict, *, context: str) -> None:
    """Every producible schema branch must fit a consumer branch recursively."""
    _inspect(producer, producer, context + " producer")
    _inspect(consumer, consumer, context + " consumer")
    _compatible(producer, consumer, producer, consumer, context)


def assert_peer_snapshot(path: Path, request: dict, response: dict) -> None:
    snapshot = json.loads(path.read_text(encoding="utf-8"))
    assert_compatible(snapshot["request"], request, context="Rust request -> Python validation")
    assert_compatible(response, snapshot["response"], context="Python serialization -> Rust response")
