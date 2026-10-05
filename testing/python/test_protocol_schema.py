"""Qualification for the supported private-schema comparison subset."""
import copy
import unittest
from testing.python.protocol_schema import SchemaMismatch, assert_compatible


def obj(field, *, required=True, extra=False):
    return {"type": "object", "properties": {"value": field}, "required": ["value"] if required else [], "additionalProperties": extra}


class SchemaComparisonTests(unittest.TestCase):
    def compatible(self, producer, consumer):
        assert_compatible(producer, consumer, context="qualification")

    def rejects(self, producer, consumer):
        with self.assertRaises(SchemaMismatch):
            self.compatible(producer, consumer)

    def test_nullable_representations(self):
        listed = {"type": ["integer", "null"], "minimum": 0, "maximum": 255}
        union = {"anyOf": [{"type": "integer", "format": "uint8"}, {"type": "null"}]}
        self.compatible(listed, union)
        self.compatible(union, listed)
        self.rejects(listed, {"type": "integer"})
        self.rejects(listed, {"anyOf": [{"type": "integer", "maximum": 254}, {"type": "null"}]})

    def test_references_constants_and_defaults(self):
        referenced = obj({"$ref": "#/$defs/flag", "default": "yes"})
        referenced["$defs"] = {"flag": {"type": "string", "enum": ["yes"]}}
        inline = obj({"type": "string", "const": "yes", "default": "yes"})
        self.compatible(referenced, inline)
        self.compatible(inline, referenced)
        drift = copy.deepcopy(inline)
        drift["properties"]["value"]["default"] = "no"
        self.rejects(referenced, drift)

    def test_nested_shape_required_enum_and_null_drift(self):
        schema = obj(obj({"type": "string", "enum": ["a", "b"]}))
        for changed in [obj(obj({"type": "integer"})), obj(obj({"type": ["string", "null"]}))]:
            self.rejects(changed, schema)
        self.rejects(schema, obj(obj({"type": "string", "enum": ["a"]})))
        self.rejects(obj(schema, required=False), obj(schema))
        self.rejects(obj({"type": "string"}, extra=True), obj({"type": "string"}))

    def test_integer_width_bounds_and_array_drift(self):
        byte = {"type": "integer", "format": "uint8"}
        self.compatible(byte, {"type": "integer", "minimum": 0, "maximum": 255})
        self.rejects({"type": "integer", "format": "uint16"}, byte)
        self.rejects({"type": "number", "minimum": 0}, {"type": "number", "exclusiveMinimum": 0})
        self.rejects({"type": "array", "items": {"type": "string"}}, {"type": "array", "items": byte})
        self.rejects({"type": "array", "items": byte}, {"type": "array", "items": byte, "maxItems": 2})

    def test_open_maps_preserve_values_and_keys(self):
        mapping = {"type": "object", "additionalProperties": {"type": "integer"}, "propertyNames": {"enum": ["a", "b"]}}
        self.compatible(mapping, copy.deepcopy(mapping))
        self.rejects({"type": "object", "additionalProperties": True}, mapping)
        self.rejects(mapping, {**mapping, "additionalProperties": {"type": "string"}})
        self.rejects(mapping, {**mapping, "propertyNames": {"enum": ["a"]}})
        self.rejects({"type": "object", "additionalProperties": {"type": "integer"}}, obj({"type": "string"}, required=False, extra=True))

    def test_unsupported_assertions_fail_even_under_open_consumer(self):
        for keyword, value in [("multipleOf", 2), ("not", {}), ("allOf", []), ("prefixItems", [])]:
            self.rejects(obj({"type": "integer", keyword: value}), {})
        self.rejects({"type": "string", "format": "unqualified"}, {})
        self.rejects({"type": "string", "format": "uint8"}, {})
        self.rejects({"type": "integer", "format": "uint999"}, {})
        self.rejects({"oneOf": [{"type": "string"}, {"type": "string"}]}, {})

    def test_tagged_exclusive_union_and_formats(self):
        variants = [{"type": "object", "properties": {"kind": {"const": kind}}, "required": ["kind"], "additionalProperties": False} for kind in ("a", "b")]
        self.compatible({"oneOf": variants}, {"anyOf": variants})
        self.compatible({"type": "string", "format": "uuid"}, {"type": "string", "format": "uuid"})
        self.rejects({"type": "string"}, {"type": "string", "format": "uuid"})
        self.rejects({"type": "string", "format": "uuid"}, {"type": "string", "format": "date-time"})
        self.compatible({"type": ["string", "null"], "format": "uuid"}, {"anyOf": [{"type": "string", "format": "uuid"}, {"type": "null"}]})
        self.rejects({"type": "number", "format": "double"}, {"type": "number", "format": "float"})

    def test_untyped_mixed_and_overlapping_nullable_shapes_fail(self):
        self.rejects({"minimum": 0}, {"minimum": 5})
        self.rejects({"enum": [True, 1]}, {"enum": [1]})
        self.rejects({"oneOf": [{"type": ["string", "null"]}, {"type": ["integer", "null"]}]}, {})
        self.compatible({"oneOf": [{"type": ["string", "null"]}, {"type": "integer"}]}, {"type": ["string", "integer", "null"]})

    def test_exclusive_unconstrained_and_nonscalar_constants_fail(self):
        self.rejects({"type": "string"}, {"oneOf": [{}, {"type": "string"}]})
        self.rejects({"type": "object", "const": {"x": True}}, {"type": "object", "const": {"x": 1}})
        self.rejects({"type": "integer", "enum": [True, 1]}, {"type": "integer", "enum": [1]})
        self.rejects({"type": "array", "enum": [[True]]}, {"type": "array", "enum": [[1]]})

    def test_finite_enum_key_map_matches_enumerated_closed_properties(self):
        value = {"type": "number", "minimum": 0}
        mapping = {"type": "object", "propertyNames": {"enum": ["a", "b"]}, "additionalProperties": value}
        enumerated = {"type": "object", "properties": {"a": value, "b": value}, "additionalProperties": False}
        self.compatible(mapping, enumerated)
        self.compatible(enumerated, mapping)
        self.rejects({**mapping, "propertyNames": {"enum": ["a", "b", "c"]}}, enumerated)
        self.rejects({**mapping, "additionalProperties": {"type": "string"}}, enumerated)
        self.rejects(mapping, {**enumerated, "required": ["a"]})
        self.rejects({"type": "object", "additionalProperties": value}, enumerated)
        self.rejects({**mapping, "properties": {"a": {"type": "string"}}}, enumerated)
        referenced = {**mapping, "propertyNames": {"$ref": "#/$defs/Keys"}, "$defs": {"Keys": {"type": "string", "enum": ["a", "b"]}}}
        self.compatible(referenced, enumerated)
        self.rejects({**mapping, "propertyNames": {"enum": ["a", "b"], "pattern": "^a$"}}, enumerated)
        self.rejects(enumerated, {**mapping, "propertyNames": {"enum": ["a", "b"], "pattern": "^a$"}})

    def test_bounded_f64_to_f32_conversion_profile(self):
        producer = {"type": "number", "minimum": 0, "maximum": 100}
        consumer = {**producer, "format": "float"}
        self.compatible(producer, consumer)
        self.compatible({**producer, "minimum": 10, "maximum": 90}, consumer)
        self.compatible({**producer, "minimum": -100}, {**consumer, "minimum": -100})
        for changed in [
            {"type": "number"},
            {**producer, "minimum": -1},
            {**producer, "maximum": 101},
            {**producer, "maximum": 1e100},
            {**producer, "maximum": float("inf")},
            {**producer, "minimum": float("nan")},
            {**producer, "exclusiveMinimum": 0},
        ]:
            self.rejects(changed, consumer)
        for changed in [
            {"type": "number", "format": "float"},
            {**consumer, "maximum": 0.1},
            {**consumer, "maximum": 1e100},
            {**consumer, "maximum": float("inf")},
            {**consumer, "minimum": float("nan")},
            {**consumer, "exclusiveMaximum": 100},
            {**consumer, "enum": [0, 100]},
        ]:
            self.rejects({**producer, "maximum": changed.get("maximum", 100)}, changed)
        self.compatible({**producer, "format": "double"}, consumer)
        self.rejects({**producer, "maximum": 90}, {**consumer, "exclusiveMaximum": 100})
        self.rejects({**producer, "minimum": 0.1}, {**consumer, "minimum": 0.1})
        self.rejects({**producer, "const": 0}, {**consumer, "enum": [0, 100]})
        self.rejects({**producer, "const": 100}, {**consumer, "const": 100})
        # A zero-width representable interval is also safe without enum admission.
        self.compatible({**producer, "minimum": 100}, {**consumer, "minimum": 100})
