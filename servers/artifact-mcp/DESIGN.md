# Artifact MCP Server

The artifact server is the typed MCP surface over the artifact plane:
discovery, metadata, access grants, release state, and revocable sharing.
It projects artifact-service state through MCP; bytes never flow through this
server. Byte policy enforcement and client streaming remain in Artifact service,
and SurrealDB remains authoritative for occurrences, identity, grants, release
state, shares, policy, and audit.

## Standards And Protocols

Model Context Protocol `2026-07-28` over JSON-RPC 2.0 and Streamable HTTP; JSON Schema
2020-12 tool contracts; platform artifact identities per
[`docs/WORK_CONTEXT_GOVERNANCE.md`](../../docs/WORK_CONTEXT_GOVERNANCE.md).
MCP Apps uses SEP-1865 / `io.modelcontextprotocol/ui` `2026-01-26`; the
server-owned `ui://artifact/library.html` resource is the canonical Library UI.
Artifact bytes use HTTP GET, HEAD, and single byte-range semantics through the
installation origin. The private S3-compatible API is not an MCP or client
contract. The repository-owned
[`ai.veoveo/knowledge-source`](../../mcp/knowledge-extension/DESIGN.md) extension
publishes metadata observations, strong revision validators and cursor enumeration.
Its dates use RFC 3339 and content digests use SHA-256.

## Protocol Surface

The server owns the `artifact://` scheme:

| Surface | Identity |
|---|---|
| index resource | `artifact://index{?cursor}` |
| Library App | `ui://artifact/library.html` |
| occurrence template | `artifact://{artifact_id}` |
| metadata template | `artifact://metadata/{artifact_id}` |
| grants template | `artifact://grants/{artifact_id}` |

Metadata, grant, release, and share operations are tools with declared input
and output schemas generated through the shared `tool` macro. Domain types
`ArtifactId`, `ArtifactMetadata`, and `ArtifactReleaseState` come from the
domain-owned [`veoveo-artifact-contract`](../../platform/artifacts/contract/DESIGN.md).
`Grant`, `ArtifactShareLink`, and its distinct ID also come from the Artifact plane
contract. Authentication, policy evaluation and service request interfaces stay in
`veoveo_mcp_contract`.

Tool requests reject undeclared fields, including nested access subjects. Share
creation keeps `artifact_id`, `expires_at` and `max_downloads` in one flat wire object.
Its strict wire decoder constructs the public request and `ShareLinkOptions`; the
generated input schema describes that same flat object.

## Boundaries

- Artifact identities are opaque `artifact://{uuidv7}` occurrences; hashes
  serve integrity and deduplication within a tenant and are never public
  addresses.
- Every call presents the forwarded gateway internal identity; the server
  verifies it with the shared gateway token verifier.
- Share links are expiring, revocable, and read only, with optional download
  limits, exactly as the governance model states.
- The server keeps no private control database and serves no bytes.

## Library Features And Addresses

Clients select `default-features = false, features = ["contract"]` on the server
library. Its public `contract` module contains tool DTOs and `ArtifactResource`.
The resolved dependencies contain identity, URI, date/time, serialization and schema
libraries. The optional `knowledge` feature exposes pure snapshot-to-access conversion
and metadata observations for derived resources such as Reason findings. Its dependency
graph excludes service clients, Store, asynchronous execution and MCP transport.
The `runtime` feature supplies service clients and persistence dependencies;
`mcp` adds the hosted executable and transport. Default builds select `mcp`.

`ArtifactResource` covers occurrences, metadata, grants, document roots, the Library App and the
contract resource. Constructors take `ArtifactId` or the closed `ArtifactDocument`
enum, and use the foundation URI builder. The index accepts one typed continuation
cursor; other resources reject query strings. Parsers reject fragments,
encoded aliases, unrelated schemes and extra segments. A URI locates an occurrence;
current service authorization still decides whether the caller can read it.
Prompt inputs decode the same Artifact ID and use these builders for references.

`src/bin/server/setup.rs` binds those types to `McpServerContract`. Startup validates
the document owner, fixed resource descriptors and all five templates before connecting
to Store. The handlers consume this setup for initialization and discovery. The first
authorized occurrence page includes these fixed resources and the Artifact index.
`ArtifactScope` declares an empty vocabulary; gateway policy and Artifact service
access decisions authorize operations.

Exact metadata reads use Artifact service's SQL admission for tenant, clearance,
retention and selected-context or live-grant access. Inaccessible records return
`NotFound` before their metadata is decoded. The plane client also exposes a checked
`ArtifactMetadataSnapshot` with read subjects, expiry and the stored metadata update
time for source observations; Artifact service owns its authorization and assembly.

The index returns at most 100 metadata links in `items`, with `uri`, `title` and an
optional `mimeType`. An optional `nextCursor` continues from the last occurrence in
descending identity order; absence ends traversal. `ArtifactIndexCursor` binds the
position to the index's version 1 token format, and the URI builder encodes its query
parameter. Metadata links lead to `artifact://metadata/{id}`. Index entries contain
no download location. The Library App follows these pages through the shared Workbench.

Artifact service selects tenant, clearance, retention and either a live direct/group
grant or the caller's selected Work Context in SQL before decoding, ordering and LIMIT.
It fetches one extra admitted row to decide whether a continuation exists. The service
checks the selected rows with its shared access evaluator and fails the page on a
policy disagreement or concurrent revocation.

The independent [contract consumer](../../testing/fixtures/server-contract-consumer/DESIGN.md)
checks the public library without runtime feature unification. Native service tests
own authorization and transport qualification.

## Installed Sharing Acceptance

`tests/gateway_sharing.rs` exercises release and share tools through the public MCP
gateway and redeems links through anonymous HTTP. It checks byte equality, read-only
delivery, download limits, expiry, explicit revocation and the current parent release
state. The test uses the official MCP client and the shared installed transport helpers.
Its `veoveo.ai/artifact-sharing-acceptance/v1` JSON report records passed checks, the
first failure and all cleanup failures without link secrets.

Set `VEOVEO_ARTIFACT_SHARING_INPUT` to an absolute JSON fixture path and run:

```sh
cargo test -p veoveo-artifact-mcp --test gateway_sharing -- --ignored --nocapture
```

The fixture uses the [installed harness input](../../testing/installed/DESIGN.md),
with `installation.callerTokenFile` holding an administrator token,
`installation.deployment` set to `artifact-mcp`, and `installation.output` selecting
a new report file. It also supplies `artifact`, its expected `owner` as an
`AccessSubject`, and an absolute `bodyFile` containing independently obtained bytes.
Choose a disposable private Artifact with 1–65,536 bytes and at least ten minutes of
remaining retention. Credentials must cover checks and cleanup. The test reads and
checks these preconditions before any mutation. It changes no grants and restarts no
service. Three minutes bound the checks; transport requests have finite deadlines.
Cleanup restores private release state first, then revokes every returned link and
checks that anonymous reads fail. A lost create response cannot trigger another create;
the restored private state blocks redemption of any link whose identity was lost.

## Knowledge Metadata

`artifact.metadata` declares `artifact://metadata/{artifact_id}` members and enumerates
through `artifact://index{?cursor}`. `src/knowledge.rs` builds observations from the
Artifact service's checked metadata snapshot. Members contain at most 64 KiB of JSON
metadata and the neutral Artifact byte URI. The collection uses `content` indexing
for that metadata JSON; artifact bytes have no knowledge collection or text extraction.

Each observation records the tenant, owner, classification and labels, selected Work
Context sharing, and read grants with their stored deadlines. The occurrence retention
deadline applies to every read path. The adapter checks the protected owner grant
before declaring owner access. It preserves the metadata update time and omits
`modifiedBy` because the ledger does not record the latest modifying principal.

The content digest covers the exact returned UTF-8 text. The revision includes that
text, its access descriptor and the stored metadata update time. Restoring prior
metadata after a change therefore requires a full read with its new modification
time. Grant changes also invalidate cached revisions. Conditional reads authorize through Artifact service
before comparing the validator. Negotiated responses carry observations; ordinary
resource reads return the same neutral metadata text. The metadata tool supplies the
installation download location for interactive use.

The collection declares `listen` changes and a 300-second freshness lifetime. Source
deadlines remain effective regardless of that lifetime. Index pages and metadata members
are subscribable. If a member loses access, the listener invalidates its previously
admitted address and the subscribed index before closing the stream. Revoked bytes
and new member identities never appear in those notifications.
The listener registers its receiver before authorization and the visible-ID query,
then emits initial invalidations for each admitted URI and requested catalog filter.
Indexing starts enumeration after these signals, preserving changes made during subscription setup.

Native tests qualify snapshot mapping, policy and deadline preservation, content/access
revisions, metadata bounds and conditional responses. Installed K01–K10 qualification
and source mutation/restart probes remain open in the [consolidated plan](../../docs/CONTRACT_CONSISTENCY_PLAN.md).

`tests/gateway_source_conformance.rs` runs source conformance through the public gateway.
It reads one selected disposable Artifact as the ordinary caller, checks that a
chosen subject has no grant, then adds and removes that subject's read grant through
the administrator's public tools. The caller must remain authorized throughout.
K07 checks both member and collection notifications, access revisions, persistence
across an actual Artifact MCP Deployment restart, and a second subscribed mutation.
Cleanup re-reads the grants and removes the fixture subject after success, failure
or an uncertain mutation response. The fixture cannot remove a pre-existing grant.
This case checks source observations; recipient access and Knowledge search policies
need their own domain cases.

The [installed harness contract](../../testing/installed/DESIGN.md) defines input
paths, credentials, deadlines and report handling. Artifact owns the grant driver in
`tests/support/read_grant_probe.rs`; Reason reuses it for derived findings without
moving Artifact vocabulary into the conformance core.

## Resource Observation

The replica watches occurrence, grant and share-link tables through Store's native
changefeed. Its checkpoint identity combines the configured replica ID and listening
port. The replica ID defaults to `HOSTNAME` in deployment and `local` outside it;
processes sharing a database and port must supply distinct replica IDs.

Store decodes typed Artifact IDs before fanout. Grant and share-link feeds keep
`INCLUDE ORIGINAL` because a deletion still needs its parent occurrence. Reconnection
and channel overflow invalidate every admitted resource. Each listener registers its
receiver before reading its baseline, then asks the Artifact service to authorize
current metadata and grant reads before sending notifications. A SQL query reuses
Store's Artifact read-admission predicate to select the earliest grant or retention
deadline across the caller's subscribed members, or the full visible collection for
an index subscription. This selection includes members beyond the first page and
ignores denied occurrences. The listener reconciles once at that stored deadline;
ordinary idle listening issues no periodic queries. Caller-token expiry invalidates
the admitted addresses and ends the stream. Each reconciliation has a 60-second
timeout and each notification has a 10-second timeout.

`src/bin/server/subscriptions/listener.rs` owns this lifecycle; `deadlines.rs` binds
typed caller and member identities to `deadlines.surql`. Native HTTP tests run the
MCP adapter and Artifact service against an isolated Store and exercise revocation,
grant expiry without a write, and retention of a member beyond the first index page.

## HTTP Probes

The shared host serves `/artifact/healthz` and `/artifact/readyz` with its
normal Host validation. Liveness reports that the HTTP process is running. Readiness requires the selected
platform database to accept a query and the internal Artifact service to report
ready, within one five-second deadline. Dependency loss returns 503 from readiness
while liveness stays 200; recovering a dependency requires no MCP process restart.

## Cursor Admission

ArtifactIndexCursor retains its Copy identity representation and literal artifact-index-v1_ prefix through an ordinary owner codec. It computes wire text at the existing conversion boundary instead of storing an allocated cache.

## Persistence Query Placement

Subscription deadlines include a complete statement from
`queries/bin/server/subscriptions/deadlines.surql`. Its fixed Artifact admission
predicate executes before the deadline aggregate; runtime scope and member values
use driver bindings. Native fixture statements live in `tests/queries/`.
