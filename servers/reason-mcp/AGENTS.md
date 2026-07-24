# Reason MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 2.

## Purpose

Provider neutral video reasoning: durable tasks answer semantic and temporal
questions about recorded sensor video (`describe_segment`, `detect_events`,
`answer_question`) by serving a locally mounted world model checkpoint through
the vLLM runtime in the deployable image. Runtime and vendor names never
appear in its public MCP identities.

## Invariants

- Owns the `reason://` scheme: pipelines, models, analyses, results, and
  artifacts.
- Recording authorization matches perception: authorize the canonical
  `recording://recordings/{uuidv7}` identity, re-resolve it inside the
  durable task, read only frozen or sealed segments, and persist no
  filesystem path or bearer token. The video ingest profile is the one pinned
  in `servers/perception-mcp/DESIGN.md`.
- Every result carries its audit identity (model, engine digest, prompt
  template revision, decode parameters) and states
  `confidence_basis: model_reported`. Never present reasoning output as
  calibrated detector confidence.
- Grounding accepts the typed perception results schema only, resolved with
  the caller's authority at submission. It never travels as a bearer token or
  a URL.
- Runner responses are validated fail closed: answer kind must match the
  task, events must lie inside the requested range in strict order, and
  counts, label lengths, and bytes are capped. The runner writes nothing to
  stdout.
- The checkpoint is a site supplied deployment input mounted read only. No
  CPU inference fallback and no optimization at request time.

## Build And Test

- `cargo check -p veoveo-reason-mcp`
- `cargo test -p veoveo-reason-mcp` — crate tests run without a GPU.
- The GPU smoke requires an NVIDIA driver compatible with the image's CUDA
  and vLLM build, NVIDIA Container Toolkit, the device plugin, and a world
  model checkpoint in Hugging Face layout loaded into the model cache. The
  Helm workload ships disabled until that checkpoint is supplied.
- The runner contract lives beside the crate in `runner/`; the runner ships
  with the deployable image.

## Contract Compliance

Contract revision: 2

- C01: met
- C02: met
- C03: met
- C04: met
- C05: met
- C06: met
- C07: met
- C08: met
- C09: met
- C10: met
- C11: met
- C12: met
- C13: met
- C14: met
- C15: met
- C16: met
- C17: pending — gateway registration does not state the contract revision
- C18: pending — well-known surface not yet wired
- C19: pending — well-known surface not yet wired
- C20: pending — well-known surface not yet wired
- C21: pending — well-known surface not yet wired
- C22: met
- C23: met
- C25: met
- C26: met
- C27: met
- C28: met
- C29: met
- C24: met
