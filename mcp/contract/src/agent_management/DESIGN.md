# Agent Management HTTP Contract

## Standards And Protocols

The management surface uses authenticated HTTP JSON and cursor-based SSE. Rust owns
closed request/response DTOs and JSON Schema 2020-12 definitions used to generate the
browser's TypeScript types. SHA-256 values use the shared `sha256:` representation.
Management is a Veoveo application API; MCP `2026-07-28` remains the protocol for
capability execution and native Tasks, as specified by the parent server contract.

## Boundary

Definition creation, draft editing, publication and instance control are distinct from
the message/input-request projections in `../agents.rs`. IDs are bounded installation
keys; the tenant and actor always come from authentication. Provider connections and
runtime templates are approved references. Browser input cannot select a provider URL,
credential value, container command or invocation authority.

Metadata responses omit instructions. Draft/revision reads require a separate content
permission. Publication binds both a management revision and an executable digest.
Every mutation carries a UUIDv7 request identity; it is independent of the definition
ID. Native MCP Tasks keep their own identities and cancellation semantics.

The [delivery plan](../../../../docs/AGENT_MANAGEMENT_PLAN.md) describes the full
feature. These types do not establish installed route, client or lifecycle support.

`templates.rs` separates installation-owned runtime packages from their public
authoring projection. Parameter inputs have closed scalar shapes. Public choices
disclose the approved service scopes, roles, membership and retained storage; they
omit image, namespace, Secret and environment bindings. The gateway and manager
must validate the full template before admitting an instance.

`instances.rs` owns asynchronous provisioning and generation-preconditioned lifecycle
requests. Its `ManagedAgentToken` is a signed repository extension to access tokens,
binding the instance key, active generation and dispatch epoch. A token is evidence
of issuance; current durable registration still determines whether it can be used.

Managed inventory lives at `/admin/{profile}/agent-instances`, with operation reads
at `/admin/{profile}/agent-operations/{id}`. Both browser edges expose the same suffixes.
Provisioning and lifecycle mutations return HTTP 202 with a durable operation identity.
The response reports accepted intent; `desired`, `observed`, `generation` and
`activeGeneration` distinguish that intent from completed convergence.

An owner or context manager can inspect an instance. Deploy and control are separate
policy actions. Resume, retry and revision changes also require deployment authority
and current template/model admission; pause, stop and archive remain available when
a template has been removed. Replaying an already admitted request retains its original
operation even after configuration changes. Shared per-context capacity includes
archived instances and their retained storage. Installation limits are
`VEOVEO_AGENT_INSTANCE_LIMIT` and `VEOVEO_AGENT_STORAGE_LIMIT_GIB`.

The existing contentless agent event stream includes authorized lifecycle changes.
Durable event heads recover missed hints; observation does not call a model or query
a provider's job status. Public projections omit deployment Secret and image details.
