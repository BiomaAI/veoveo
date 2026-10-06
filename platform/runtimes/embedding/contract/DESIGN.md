# Embedding Contract

## Standards And Protocols

| Standard or format | Supported profile |
|---|---|
| JSON and JSON Schema 2020-12 | checked input batches, tasks, model, revision, dimension and vector values shared by adapters |
| SHA-256 | length-framed vector-space, immutable execution-profile and qualification identities; image, checkpoint manifest and report digests |
| IEEE 754 | finite `f32` vectors with a checked dimension and L2 normalization |

## Ownership

This crate owns embedding-space identity below the runtime client, knowledge domain
and Store. It has no HTTP, database, MCP or inference dependency. The runtime client
described in the [embedding design](../DESIGN.md#client) constructs these values from
its qualified runtime response and deployment configuration.

An `EmbeddingSpace` contains the model, checkpoint revision, dimension, pooling,
normalization, effective precision and maximum input tokens. These fields identify
which vectors can share a generation. Runtime image and device settings belong to
`EmbeddingExecutionProfile`, whose content-derived ID includes the complete space,
image digest, checkpoint manifest digest, vLLM version, measured NVIDIA GPU, CUDA and
driver versions, and effective serving limits, priority scheduling, configured graph allowance,
observed graph execution, attention backend and an explicit nullable KV-cache override.
Graphs allowed does not claim graph execution. A null override means no explicit
reservation; it never supplies an inferred size. Every field must be present. Serving precision and token limits
must agree with the space. Each profile is immutable and each decoder recomputes its ID.

`EmbeddingQualification` binds one query profile to one producer profile in the same
space. Its ID covers both profile IDs, the space, and the admitted reference, retrieval,
scheduling and capacity report digests and passing results. Reference cosine must be
at least 0.999. A qualification is directional. `QualifiedEmbeddingRuntime` contains
the installation-selected effective profile, at most 64 admitted profiles and their
compatibility matrix. It requires explicit self qualification and rejects unknown,
duplicate or mismatched profiles. Report statements are trusted installation input;
syntactic admission does not perform hardware measurements or attest a remote process.

`EmbeddingVector::new` takes that runtime and records its producer profile ID. Serde
also checks dimension, finite components and squared L2 norm within 0.002 of one.
The Knowledge domain additionally checks the runtime, space and producer relationship
before constructing an indexed chunk. Numeric admission never qualifies a deployment.

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
