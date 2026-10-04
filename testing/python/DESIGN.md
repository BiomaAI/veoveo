# Private Protocol Schema Checks

## Standards And Protocols

| Format or implementation | Supported profile |
|---|---|
| JSON Schema Draft 2020-12 | The acyclic structural subset emitted by the participating Rust and Python owners |
| Workspace-pinned Schemars | Rust serialization schemas for requests and deserialization schemas for responses |
| Owner-pinned Pydantic | Python validation schemas for requests and serialization schemas for responses |
| Private owner protocols | Map normalization, cuOpt execution, Reason inference and Speech worker messages |

## Ownership

Each process owner defines its wire types and maintains a Rust schema snapshot in its
own test data. Its Python suite compares the complete reachable request and response
graphs with that snapshot through `protocol_schema.py`. Rust tests regenerate the
schemas and reject snapshot drift. Set `UPDATE_PRIVATE_PROTOCOL_SCHEMAS` when changing
the owning types to refresh their snapshots, then run both peers' suites.

The shared helper belongs to test tooling. Production processes do not import it.
It reports where the declared producer shape exceeds the consumer's admission. Rust
requests are compared with Python validation; Python outputs are compared with Rust
deserialization. A consumer may accept more than its peer emits.

## Comparison Profile

The helper follows local definitions and compares object fields, required fields,
typed maps, array items, scalar types, enums, nullability and declared bounds.
Equivalent constant and enum representations and nullable type lists are supported.
Exclusive unions require demonstrably disjoint alternatives. Integer width formats
participate in range checks; date and UUID formats retain their wire requirements.

Unknown assertion keywords, recursive references and unsupported compositions fail
explicitly. Matching patterns can establish the supported string profile; the helper
does not attempt general regular-expression inclusion. Schema annotations do not
establish runtime validation. Owners must enforce constraints through their types or
checked construction and qualify behavior that their schema cannot describe.

This comparison is a restricted compatibility check. It does not establish general
JSON Schema equivalence, semantic correctness, authorization, provider execution or
GPU behavior. Shared message fixtures and native decoder tests cover actual encoding
and decoding. Installed and hardware suites qualify process execution.

## Qualification

The helper's unit tests cover equivalent schema representations and incompatible
nested fields, required values, enums, nulls, numeric bounds, arrays and open maps.
Each participating Python suite checks its whole protocol graph. Owner Rust tests
check the corresponding maintained snapshot using the correct Serde direction.
