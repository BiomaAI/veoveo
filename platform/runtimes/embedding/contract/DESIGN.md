# Embedding Contract

## Standards And Protocols

| Standard or format | Supported profile |
|---|---|
| JSON and JSON Schema 2020-12 | checked model, revision, dimension and vector values shared by adapters |
| SHA-256 | runtime image content digest through `veoveo_types::Sha256Digest` |
| IEEE 754 | finite `f32` vectors with a checked dimension and L2 normalization |

## Ownership

This crate owns embedding-space identity below the runtime client, knowledge domain
and Store. It has no HTTP, database, MCP or inference dependency. The runtime client
described in the [embedding design](../DESIGN.md#client) constructs these values from
its qualified runtime response and deployment configuration.

An `EmbeddingSpace` contains a model name, checkpoint revision, dimension and runtime
image digest. A change to any field denotes a different space. Model names and
revisions use separate types with 1–256 printable bytes. Dimensions range from 1 to
8,192; a valid dimension does not establish that a model or device is qualified.

`EmbeddingVector::new` and deserialization enforce the same rules. The vector has
exactly the space's dimension, every component is finite, and its squared L2 norm is
within 0.002 of one. Consumers compare the complete space before combining vectors.
These are data-admission checks. Runtime tests still require the hardware GPU,
checkpoint, reference-vector and scheduling checks in the owning runtime design.
