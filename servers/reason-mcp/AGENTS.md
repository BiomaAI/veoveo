# Reason MCP Server — Agent Manual

Delta over the repository root `AGENTS.md`. The normative server contract is
[`mcp/contract/DESIGN.md`](../../mcp/contract/DESIGN.md), revision 3.

## Purpose

Provider neutral video reasoning: durable tasks answer semantic and temporal
questions about recorded sensor video (`describe_segment`, `detect_events`,
`answer_question`) by serving a locally mounted world model checkpoint through
the vLLM runtime in the deployable image. Runtime and vendor names never
appear in its public MCP identities.

## Invariants

- The server is hosted through `veoveo_mcp_contract::hosting`: `ReasonMcp` implements
  `DomainServer`, knowledge members read as `DomainRead::no_store`, and
  `ReasonListener` is the `DurableListener` that joins analysis task updates with
  finding changes. `hosted::server` builds the server for the binary and the hosted
  tests. Do not add a `ServerHandler`, router, host check or authentication
  middleware here.
- Owns the `reason://` scheme: pipelines, models, analyses, results, and
  artifacts.
- Keep pipeline, model and analysis IDs distinct through internal APIs. Public
  resource builders and cursors belong to `contract`; hosted declarations use
  the shared checked setup. Exact analysis reads and subscription admission use
  the Task runtime's SQL owner read before decoding.
- Construct catalog and analysis responses through their contract constructors.
  Derive repeated identities from one typed value and reject conflicting wire fields.
  Retained successful outputs must match their owning Task and requested pipeline;
  corrupt output is an explicit recovery error.
- Publish terminal success through the shared Reason result builder with one
  canonical `result_uri` and one product link. Task reads and subscriptions validate
  the current result type after owner checks. Replace old contracts by hard cut;
  do not add historical-data readers or compatibility migrations.
- Owner-scoped analysis and result notifications use the shared Task-backed resource listener.
  Do not restore process-local broadcasts or emit Task status for resource-only
  listeners. Additional Task-backed routes implement the owning contract trait.
- Use analyses, findings and results for Reason domain types and copy. Documents
  refer to the server's actual documentation. `FindingSummary` carries reusable
  result excerpts and recorded provenance.
- Reusable finding queries inherit current access from the result Artifact through
  `ArtifactReadScope`; keep that predicate before SQL limits and output decoding.
  A result grant does not confer Task control or access to other output Artifacts.
  Finding observations use Artifact's access mapping and recheck source admission
  after Artifact metadata reads. Capture checked `FindingData` before publishing
  Artifacts and require it in successful Task outputs; summary reads never download
  result bytes. Collection notifications cover every admitted member, including
  those beyond the first page, through the Store observer and SQL fingerprints.
  A lost member invalidates its previously admitted URI and collection before
  ending the subscription; it cannot continue observing inaccessible state.
- Recording authorization matches stream: authorize the canonical
  `recording://recordings/{uuidv7}` identity, re-resolve it inside the
  durable task, and capture one bounded source snapshot. The snapshot may
  contain frozen or sealed segments and complete acknowledged parts from the
  writing segment. It persists no filesystem path or submitted gateway bearer token. The video
  ingest profile is the one pinned in `servers/stream-mcp/DESIGN.md`.
- Every result carries its audit identity (model, engine digest, prompt
  template revision, decode parameters) and states
  `confidence_basis: model_reported`. Never present reasoning output as
  calibrated detector confidence.
- Grounding imports Stream's contract-only result and Artifact address types.
  Require the same recording, entity and timeline with a covering replay range;
  admit citations only from selected frames. Capture classification and labels
  from the authorized Artifact read and require them on the output capability.
  Persist the validated subset and issued capabilities for current-task recovery;
  the Artifact service owns output-label persistence and enforcement.
- Runner responses are validated fail closed: answer kind must match the
  task, events must lie inside the requested range in strict order, and
  counts, label lengths, and bytes are capped. The runner writes nothing to
  stdout.
- The checkpoint is a site supplied deployment input mounted read only. No
  CPU inference fallback and no optimization at request time.

## Build And Test

- Public consumers select `default-features = false, features = ["contract"]`.
  Keep its dependencies free of MCP, Store, Rerun and runtime execution. `runtime`
  supplies domain execution; `mcp` adds hosted transport and is the default.
  Qualify the contract tests through an independent consumer as well as the service.
  The `knowledge` feature adds `FindingCollection::descriptor` using the lightweight
  knowledge-extension contract. It does not enable MCP transport or model execution.

- `cargo check -p veoveo-reason-mcp`
- `cargo test -p veoveo-reason-mcp` — crate tests run without a GPU.
- `cargo test -p veoveo-reason-mcp --test knowledge` checks the finding query with
  isolated SurrealDB containers, readable pages behind malformed denied rows, result
  provenance and grant deadlines. Its completed outputs are inert fixtures.
- `cargo test -p veoveo-reason-mcp --bin reason-mcp hosted_tests::` checks the
  production HTTP router with generated signing keys, the real Artifact service and
  an isolated Store. Owner probes recreate the source and mutate grants. Completed
  findings are fixtures; this check performs no inference or GPU readiness acceptance.
- `tests/installed_knowledge.rs` checks a completed installed GPU analysis through
  the public gateway. The read-only case compares both finding collections with the
  completed output and verifies search links, conditional reads and source revisions.
  The access case requires a disposable analysis, an Artifact administrator and a
  reviewer in a different Work Context. It creates one previously absent result
  grant and removes it even when a later assertion fails. Both cases require hardware
  embeddings; they never invoke inference. Input files and commands are described in
  the [reference runbook](../../examples/bioma/README.md#acceptance).
- `tests/gateway_source_conformance.rs` qualifies both finding collections through the gateway,
  reusing a completed analysis and Artifact's grant driver. It derives the result
  Artifact from the admitted finding and restarts only the selected Reason Deployment.
  Inputs and commands follow [the installed harness contract](../../testing/installed/DESIGN.md).
  This is source-contract acceptance and performs no inference.
- The GPU smoke requires an NVIDIA driver compatible with the image's CUDA
  and vLLM build, NVIDIA Container Toolkit, the device plugin, and a world
  model checkpoint in Hugging Face layout loaded into the model cache. The
  Helm workload ships disabled until that checkpoint is supplied.
- The runner contract lives beside the crate in `runner/`; the runner ships
  with the deployable image.
- Runner unit checks use `uv run --project servers/reason-mcp/runner --locked --extra dev pytest servers/reason-mcp/runner/tests`.
- The internal GPU input adapter admits Qwen3-VL and requires NVDEC, CUDA
  preprocessing, and an in-process vLLM embedding handoff. Model-neutral MCP
  names do not imply admission of other checkpoint architectures.

## Contract Compliance

Contract revision: 3

- C01: met
- C02: met — v1 terminal products carry canonical `result_uri`; native current-format Task delivery passes, while [installed qualification](../../docs/PLATFORM_FOUNDATIONS_PLAN.md#deferred-work) is pending
- C03: met
- C04: met — analyses use Store cursor pages; discovery lists roots and fixed catalog entries
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
- C17: met — both gateway registrations declare revision 3
- C18: met
- C19: met
- C20: met
- C21: met
- C22: met
- C23: met
- C25: met
- C26: met
- C27: met — analysis and result invalidations use the shared database-backed Task watch with current-owner SQL checks; resource-only requests omit Task payloads
- C28: met — discovery contains immutable catalog entries and roots; task changes do not advertise discovery-list changes
- C29: met
- C30: met — the endpoint is stateless; durable and domain state never derives authority from a protocol connection
- C24: met
- C31: pending — installed Discover and list readiness qualification is pending
- C32: pending — shared typed docs support is wired; K01–K08 qualification is in progress
