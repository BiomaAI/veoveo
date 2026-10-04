# Embedding Contract

## Standards And Protocols

| Standard or format | Supported profile |
|---|---|
| JSON and JSON Schema 2020-12 | checked input batches, tasks, model, revision, dimension and vector values shared by adapters |
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

`EmbeddingText` admits nonblank text up to 16 KiB. `EmbeddingBatch` holds 1–32 such
values with at most 128 KiB of combined UTF-8 text. `EmbeddingTask` admits a printable
instruction up to 1,024 bytes, without control characters that could change the query
prefix. Builders and deserialization use the same admission. JSON Schema expresses
the count and character ceilings; Rust admission also checks UTF-8 bytes and the
aggregate budget. `EmbeddingPriority` distinguishes interactive work from bulk indexing.
The client owns model-specific formatting and checks the formatted request's budget.

## Identity Declaration Mechanics

Embedding model and revision identities use `Id` with owner validation for printable text of 1–256 bytes. Serde applies this admission through checked String conversion. Embedding dimensions and vectors retain their separate numeric and relationship validators.

## Value Admission

EmbeddingVector stores `Checked` VectorWire facts. Its owner check enforces dimension, finite values and the existing L2 tolerance; the foundation adds no inference or embedding policy.
