# Veoveo MCP Server Contract

This document is the normative contract for every hosted MCP server and every
extension registered with a Veoveo installation. It consolidates the protocol,
schema, runtime, packaging, documentation, and self-description requirements
that were previously stated across `AGENTS.md`, `docs/TECH_DESIGN.md`, and
`docs/ENTERPRISE_DEPLOYMENT.md`; those documents now point here. The crate in
this directory, `veoveo_mcp_contract`, implements the shared mechanics that
make most of the contract hold by construction.

**Contract revision: 1.** The crate exports the same value as
`veoveo_mcp_contract::CONTRACT_REVISION`. A server declares the revision it
complies with in its crate documents and in its contract resource.

## Scope And Discovery

The contract governs the servers in `servers/*-mcp/` and any independently
deployed extension whose gateway entry joins an installation's catalog.

Checks are generic over a discovered catalog and never enumerate servers by
hand:

- In the repository, a server is any cargo workspace member under `servers/`
  whose crate name ends in `-mcp`.
- Against an installation, the server set is the gateway control-plane
  catalog.

Adding a server means the checks find it. No conformance manifest, Console
page, or documentation index requires editing when a server is added.

## Protocol Surface

Veoveo does not flatten MCP into a collection of convenience tools. Each
server uses the protocol surface that matches its domain:

| Need | Canonical MCP surface |
|---|---|
| action | tool with declared input and output JSON Schemas |
| durable action | task-augmented tool through the MCP tasks API |
| addressable state | resource or resource template |
| discovery | resource list/template plus completion |
| reusable interaction | prompt |
| live condition | resource subscription and notification |
| progress/result wake | task subscription |
| cross-server identity | canonical URI and resource link |

Compatibility helpers are allowed only when they are explicit product features
for clients that cannot use the richer MCP surfaces well. They must be
additive projections over the canonical protocol behavior and must reuse the
same typed models, policy checks, audit paths, task state, artifact
identities, and resource URIs. Hidden fallbacks, alternate completion paths,
unaudited content URLs, and second sources of truth are prohibited.

## Schemas And Types

Tool inputs publish one canonical JSON Schema 2020-12 document generated from
the request type. The document has an object root, contains no references, and
declares the immediate JSON type of every property. Object-shaped unions
expose `type: object` alongside their variants. Recursive tool arguments are
outside this profile; domain contracts model bounded collections explicitly.

Rust servers import `tool` from `veoveo_mcp_contract`, which selects the
shared Schemars generator for every `Parameters<T>` handler and supplies the
closed empty-object schema for handlers without arguments. Python servers pass
each Pydantic request model through `veoveo_mcp.schema.mcp_input_schema`
before publishing it.

Strong types govern every controlled shape: typed structs, enums, and explicit
domain types wherever the shape is known or owned by this contract. Raw JSON
is reserved for genuinely open-ended boundaries.

## Runtime Boundary

A hosted server owns its domain models and declared schemas and consumes the
shared mechanics of `veoveo_mcp_contract` rather than reimplementing them:
task records and the task runtime, webhook waiters, resource subscriptions,
URI conventions, Work Context propagation, and internal identity.

- Durable operations run on the shared task runtime and the final task
  extension.
- Artifact and recording operations present the forwarded short-lived
  internal identity signed by the gateway.
- Administrative HTTP, when a server has it, is served only under the
  server's canonical mount and reached through the gateway admin route.
- A server has no private control database. Durable state lives in the
  platform stores.
- A server has no private byte route. Bytes flow through the artifact plane.

## Packaging And Registration

A server ships as an OCI image with a versioned Helm chart. Its gateway entry
is registered in the typed control plane with its routes, capabilities, and
policy, and states the contract revision the server complies with. Extensions
follow the identical pattern without adopting Veoveo's source build; the
mechanics are in
[`docs/ENTERPRISE_DEPLOYMENT.md`](../../docs/ENTERPRISE_DEPLOYMENT.md).

## Well-Known Surface

Every server is self-describing. Under its canonical URI scheme it serves:

| Resource | Content |
|---|---|
| `{scheme}://docs` | index of the server's documents |
| `{scheme}://docs/{doc_id}` | a document body: at minimum `agents` (the crate `AGENTS.md`) and `design` (the crate `DESIGN.md`) |
| `{scheme}://contract` | machine-readable contract declaration: contract revision, per-item compliance status, and the server's capability inventory |

On its administrative mount the server serves the same material for REST
consumers at `{mount}/admin/docs/llms.txt` (an index in llms.txt form) and
`{mount}/admin/docs/{doc_id}`.

Documents are embedded at build time from the crate, so a running server
serves the manual for exactly the version deployed, including in offline
installations. The `veoveo_mcp_contract::docs` module provides the embedding,
declaration, and rendering machinery; consuming it is the intended way to
comply.

The Console renders these resources generically; the gateway generates an
installation llms.txt from the catalog. Neither requires per-server work.

## Crate Documents

Documentation lives beside the code it governs, written for agents first and
readable by humans:

- `DESIGN.md` — the server's domain contract, including its standards and
  protocols profile.
- `AGENTS.md` — the agent work manual, delta-only over the repository root
  `AGENTS.md`, with required sections `Purpose`, `Invariants`,
  `Build And Test`, and `Contract Compliance`. The compliance section lists
  checklist items with status `met` or `pending`, so gaps are declared rather
  than silent.

Server crates are named `*-mcp`.

## Compliance Checklist

| ID | Level | Requirement |
|---|---|---|
| C01 | MUST | Each capability uses the canonical MCP surface for its need per the Protocol Surface table. |
| C02 | MUST | Every tool declares input and output JSON Schemas. |
| C03 | MUST | Durable operations are task-augmented tools on the shared task runtime. |
| C04 | MUST | Addressable state is exposed as resources or resource templates under the server's canonical scheme. |
| C05 | MUST | The server is not flattened to a tool-only convenience surface. |
| C06 | MUST | Compatibility helpers are additive projections reusing canonical models, policy, audit, tasks, and URIs. |
| C07 | MUST | Tool input schemas follow the canonical 2020-12 profile: object root, no references, immediate types. |
| C08 | MUST | Schemas are generated through the shared machinery (`tool` macro; `mcp_input_schema` for Python). |
| C09 | MUST | Controlled shapes use strong domain types; raw JSON only at open boundaries. |
| C10 | MUST | Shared mechanics come from `veoveo_mcp_contract`, not reimplementation. |
| C11 | MUST | Artifact and recording operations use the forwarded internal identity. |
| C12 | MUST | Administrative HTTP exists only under the canonical mount. |
| C13 | MUST | No private control database. |
| C14 | MUST | No private byte route. |
| C15 | MUST | The server ships as an OCI image with a versioned Helm chart. |
| C16 | MUST | The gateway entry is registered in the typed control plane with routes, capabilities, and policy. |
| C17 | MUST | The registration and crate documents state the contract revision. |
| C18 | MUST | Docs resources are served under `{scheme}://docs`. |
| C19 | MUST | The contract declaration resource is served at `{scheme}://contract`. |
| C20 | MUST | The admin mount serves `docs/llms.txt` and document bodies. |
| C21 | MUST | Served documents are embedded at build time from the crate. |
| C22 | MUST | `DESIGN.md` exists beside the crate and pins the domain profile. |
| C23 | MUST | `AGENTS.md` exists beside the crate with the required sections. |
| C24 | MUST | The crate is named `*-mcp`. |

## Enforcement

Verification is layered and discovers servers per the Scope And Discovery
rules:

- **Repository structure** — `testing/mcp-conformance` asserts C22, C23, and
  C24 for every discovered server crate, including required `AGENTS.md`
  sections and a parseable `Contract Compliance` declaration.
- **Protocol conformance** — the conformance client validates advertised
  schemas (C07) and the client-facing protocol shape against a running
  server, and reads `{scheme}://contract` (C19) to compare the declaration
  with observed behavior as servers adopt the well-known surface.
- **Construction** — C03, C08, C10, C18–C21 are inherited by consuming
  `veoveo_mcp_contract`; avoiding them requires bypassing the shared crate,
  which review treats as a contract change.
- **Review** — C05, C06, C09, C13, and C14 are review-enforced boundaries;
  their violation is architectural, not stylistic.

Capability inventories are part of the contract declaration, so protocol
surface changes are reviewable diffs rather than silent drift.
