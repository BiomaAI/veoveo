# Python Hosted Server Contracts

The Python SDK supplies common types and MCP integration for independently owned
servers. Each server package defines its scope vocabulary, resource variants and
domain admission. The [SDK guide](README.md) describes identity verification, Tasks,
Artifact transport and development commands.

## Standards And Protocols

Controlled SDK JSON objects require Pydantic 2.13 or newer and admit only their current field aliases. A shared preflight derives permitted keys from the owning model fields before either Python-object or JSON decoding; metadata values keep their open provider shape.

| Standard or protocol | Supported profile |
|---|---|
| MCP `2026-07-28`, Python SDK `mcp==2.0.0` | Hosted stateless Streamable HTTP, Discover, resource descriptors and request-scoped subscriptions; the [hosted contract](../../mcp/contract/DESIGN.md) defines the required surface |
| JSON Schema 2020-12 | Complete tool schemas through `schema.py`; Pydantic validates wire models. Shared Artifact and Usage objects admit camelCase keys through explicit aliases and preserve Python attribute names; ordinary controlled values use snake_case. Signed identity and native Task records retain their declared snake_case format. |
| RFC 3986, [rfc3986 2.0.0](https://pypi.org/project/rfc3986/2.0.0/) | Concrete hierarchical URI syntax with input spelling preserved; owner resource components reject credentials, ports, fragments, malformed UTF-8 and repeated query names |
| RFC 6570, [uri-template 1.3.0](https://pypi.org/project/uri-template/1.3.0/) | ASCII resource templates with a fixed scheme; component builders use simple, reserved, path and query expansion. Library-specific defaults, array notation and variable aliases are rejected. Partial expansion is not used |
| RFC 6749 scope-token | Closed owner scope enums; external `ScopeName` values follow Veoveo's printable identifier profile |
| Chrono 0.4.45 DateTime JSON | Artifact timestamp strings preserve the original wire, nanoseconds, leap encoding and signed Gregorian years -262143..262142; The qualified profile uses padded date/time fields and standard numeric offsets. SDK schema declares the extended string syntax without generic date-time format. |
| RFC 3339 | Task compare-and-set timestamp strings preserve up to nine fractional digits; their native driver profile stays separate from Artifact Chrono strings |
| RFC 9562 UUIDv7 | Task identities and nominal Artifact identities at wire admission |
| `ai.veoveo/knowledge-source` | Embedded document collections, content digests, negotiated observations and conditional reads |

## Type Ownership

`veoveo_mcp.types` has no MCP or runtime imports. `ScopeName`, `ResourceScheme`,
`ResourceUri` and `ResourceTemplateUri` are distinct checked string types. Their
constructors and Pydantic adapters apply the same validation. JSON serialization
preserves their wire strings. Generic network references may include ports and
fragments; `ResourceUri.components()` applies the stricter domain address profile.

Owners define enums derived from `ScopeEnum`. Enum construction rejects invalid
OAuth spelling and aliases. `ScopeDefinition` exposes each variant's validated
name. Parsing an external name establishes syntax, while the owner's enum identifies
a permission known to its implementation.

`ResourceAddress` exposes `to_uri()`. Owner variants carry specific identities and
cursor types. Their constructors check those types; owner parsers reconstruct the
variants and reject unsupported route shapes. `ResourceUriBuilder` accepts checked
scheme, authority and segments. The URI libraries own escaping and component parsing.
The builder rejects duplicate query names before expansion can collapse them.

`CheckedText` lets an owner define additional nominal identities with shared
Pydantic integration. `contract.artifacts.ArtifactId` uses it for canonical UUIDv7
spelling. Wider identity models and runtime contracts keep their owning modules.

## Artifact Timestamp Values

`timestamp.ChronoTimestamp`, exported by `veoveo_mcp.types`, stores an immutable checked wire string. Its admission uses the standard library's proleptic Gregorian calendar, checks local calendar components and the UTC Chrono range, and keeps the leap second as a preceding whole second with nanoseconds at or above one billion. The qualified profile permits at most nine fractional digits, padded components, `T` and `Z` or a standard numeric offset. Broader relaxed Rust parsing is outside this SDK profile until producer/receiver qualification establishes it. Explicit `same_instant` comparison uses full nanosecond precision. Values have no lexical ordering.

All six Artifact creation, retention and capability-expiry fields use this scalar. Python-object, JSON and TypeAdapter admission run the same checks, including when handed a nominal instance. Artifact copy helpers re-admit updates and wire emission preserves the timestamp token. Repository clock callers construct values with `from_datetime`, which requires an aware datetime. `as_datetime_exact` refuses leap seconds, unrepresentable years and sub-microsecond precision. `as_datetime_lossy_microseconds` explicitly drops sub-microsecond digits but still refuses leap seconds and unrepresentable years. Neither conversion changes the stored wire.

Rust Artifact timestamp schemas explicitly select the foundational `ChronoUtcTimestampSchema` lexical carrier, which admits signed and extended years emitted by Chrono. Actual producer/schema/browser controls qualify that receiving path separately from SDK scalar decoding. Native Task timestamp/CAS records, JWT timestamps and Knowledge grant fields keep their declared profiles.

## Checked MCP Setup

`contract.server.McpServerContract` associates a server's scope and resource types
with its implementation identity, documents and descriptors. The protocol contains
no server registry. `McpResource` and `McpResourceTemplate` builders bind descriptors
to their checked addresses and reject an attempted address replacement.

`McpServerSetup` checks scope membership, duplicate declarations, typed resource
round trips, required documents and the docs/contract roots. It attaches document
collection metadata to the declared document template. Handlers receive sorted
copies of the validated descriptors, so changing a response cannot mutate setup.
`configure()` checks the actual MCP server identity and required resource handlers
before declaring the knowledge extension.

`has_scope()` accepts the owner's scope type and authenticated grant names. It
rejects another owner's enum and refuses undeclared permissions. The owner still
checks tenant, Work Context, resource grants and other current domain policy.

Servers construct setup before connecting Stores or resuming workers. Datasheet's
`server/contract.py` supplies the working template. Importing that module validates
its declarations before `serve()` starts dependencies. Resource reads authenticate
the request, parse the owner address, and apply the existing SQL owner query.

Python protocols provide annotations for static consumers; constructor checks and
native tests establish runtime declaration consistency. Hosted conformance,
authorization, recovery and GPU acceptance require their own applicable checks.

## Qualification

Run `uv run --locked --all-extras pytest` from this directory. The independent owner
fixture exercises setup without importing Datasheet. It checks descriptor tampering,
missing and duplicate declarations, permission type mismatches and discovery-copy
isolation. URI cases exercise reserved characters, malformed escapes, template
separation and nominal wire decoding. A fresh process proves that foundational
imports do not load MCP, SurrealDB or the Task runtime.

Datasheet qualifies template expansion against its resource variants and exercises
the handlers over MCP. Its installed acceptance uses the reference installation's
hosted and knowledge-source suites. The unrelated fork workload also runs against
the shared SDK to detect wire and packaging regressions.

## Internal Identity Assertions

The receiver requires `veoveo.ai/gateway-internal-assertion/v2` on the Ed25519 JWT
and `veoveo.ai/gateway-request-context/v2` on its signed request context. Missing or
unsupported formats fail verification. Hosted receivers and the gateway require a
coordinated upgrade and drain; this SDK admits one internal format.

`AuditManagedExecution` carries checked instance and optional UUIDv7 episode identity
with positive unsigned 64-bit generation and dispatch counters. The closed access-token
model rejects the public `managed_agent` claim in internal assertions. Only an automated
service without a browser session may carry execution attribution. The receiver checks
actor, source principal, client, tenant, context, invocation provenance and source-token
expiry before delivery. Current registration and domain permissions remain the server's
responsibility.

Normalized principals use the closed `PrincipalAssurance` vocabulary shared with
Rust: `us_person`. An omitted assurance set defaults to empty, and unknown normalized
values fail model and signed internal-token admission. External JWT claims keep their
gateway normalization policy; this vocabulary applies to the principal delivered to
the server.

## Native Task Records

The Task owner uses `tasks/records.py` for both admitted reads and native writes.
Creation, idempotency links and input rows have closed record models. Request,
owner-context and authority envelopes share constructors across admission, claims,
transitions and compare-and-set snapshots. JSON-valued envelopes use the JSON codec;
native record references and datetimes remain outside it. The codec preserves
unsigned integers and explicit JSON null. An absent failure detail stays omitted.
Native nullable lifecycle fields
use database NONE, and authority label sets and retention pins serialize in sorted
order. Query predicates and lease checks compare the complete admitted envelopes.

## Task Query Assets

The Task runtime stores complete SurrealQL statements under
`src/veoveo_mcp/tasks/queries/`. Owner, Work Context, operation-type and cursor
choices select complete files before execution. Authorization predicates run in
SQL before limits and decoding. The package loader caches each UTF-8 resource
through `importlib.resources`; wheels carry the same assets. The native SHOW
changefeed template substitutes only validated numeric cursor and limit values.

Test mutations live under `tests/queries/`. The native fixture starts an isolated,
digest-pinned SurrealDB 3.3 container and uses a prebuilt Gateway composition
binary to generate an empty optional-module selection, prepare the installation
and migrate its planned kernel lanes. `VEOVEO_TEST_GATEWAY_BIN` selects that
executable; the default is the repository's `target/debug/gateway`. Tests fail
with a build prerequisite diagnostic when it is unavailable. They neither build
the binary nor publish an installation control plane. Docker creation writes
an invocation-private CID file before the fixture admits the returned identity.
Cleanup removes only that CID and requires Docker to confirm removal within 30
seconds. An uncertain launch or removal keeps its private receipt and redacted
diagnostics; a container name never authorizes cleanup.

## MCP Task Wire Admission

The Tasks extension emits camelCase fields through the MCP `2026-07-28`
flattened Task profile. `GetTaskResult.from_wire` admits every detailed status
with the existing Pydantic model and preserves the response's `_meta`. It rejects
retired field names, mixed spellings, missing required fields and unknown controlled
fields. Result and error payloads keep their domain-defined keys. Input requests
and responses retain the maintained SDK's typed protocol shapes and open schemas.

The maintained server dispatcher decodes the registered Get, Update, Cancel and
subscription parameter types with alias-only admission. Standard `_meta` values
keep their upstream extensibility. `TaskSubscriptionFilter` also preserves the
upstream filter's standard fields and contributions from other extensions; it
rejects alternate spellings of its known fields without closing that extension map.

Typed Python constructor keyword arguments use the model's attribute names.
External `model_validate`, JSON decoding and the explicit flattened receiver apply
current-key preflight from `CurrentWireModel` before Pydantic validation. The
admission context reaches nested controlled values. Internal result and notification
wrappers emit their flattened wire through `wire()`; native stored Task records
follow the separate profile below.

The owning protocol controls exercise the maintained server dispatcher and every
Task status, including preservation of extension metadata and open payloads.
These controls establish SDK admission and emission, independently of installed
provider completion and recovery qualification.

## Stored Task Records

The Task's `owner` column is its native Principal record reference. The required
`owner_context` object stores the public TaskOwner snapshot, including its full
invocation authority and caller clearance. The closed request envelope contains
required opaque `input` and optional `status_message`, `ttl_ms` and
`poll_interval_ms`. Provider input may contain arbitrary nested fields and JSON
null. Public TaskOwner and TaskSnapshot wire forms keep their existing shapes.

The decoder rejects unknown fields in the stored owner, request and shared
authority models. It compares complete typed record references for Task, Tenant,
Principal, profile, Work Context, initiator and server. It also checks the stored
authority and indexed provenance against the owner snapshot. Caller clearance
and output-policy labels describe separate permissions and may differ.

Request replacement and input settlement compare both the expected request and
expected owner context inside their transaction. An ownership change fails the
comparison even when the Task timestamp has not changed. Stored timing metadata
uses strict unsigned 64-bit admission. The existing Store decimal adapter preserves
metadata and opaque JSON integers above the native signed integer range. Runtime create validates the
expiration datetime before identity or Task writes. An absent TTL uses seven days;
zero expires at creation.

The schema derives private `created_at_exact` and `updated_at_exact` strings from
the native Task datetimes on every write. This driver metadata also travels in
native changefeed snapshots. The Python SDK's ordinary datetime view has microsecond
precision; the checked Task timestamp preserves the original RFC3339 string and
binds it through the SDK's `Datetime` type for exact compare-and-set checks.
Snapshot wire round trips keep those strings in the existing `created_at` and
`updated_at` fields. Owner page cursors retain the checked creation token and bind
its full precision alongside the Task identity. Equivalent RFC3339 offsets and
fractional spellings compare by exact seconds and nanoseconds. The private fields
are not added to the public snapshot. Driver rows must carry both tokens; missing
or invalid tokens fail decoding.

## Packaged Compliance Profiles

Rust's requirement catalog supplies the revision-2 identities and wording. Hosted
contract revision 4 remains separate. Owners author a complete `contract-compliance.json`;
`ComplianceProfile` validates every entry against the generated catalog and stores an
immutable ordered value. Required explanations preserve their admitted bytes.
Blankness uses the actual Rust whitespace characters exported in the catalog. Conditional
applicability is checked against discovery; the shared document setup declares its
knowledge-source collection and rejects an absent-extension declaration.

`_compliance.py` uses only the Python standard library. Runtime loading and the isolated
Hatch hook reuse its admission and deterministic marked-section renderer. The hook loads
the module directly without importing the SDK or its dependencies. It packages original
manual/design bytes, the owner profile and Rust-generated catalog/schema exports with
SHA-256 digests. Installed loading reads these package artifacts without repository access,
checks all digests and compares the manual section with the admitted profile. It does
not reconstruct declarations from Markdown or replace served bytes. Source loading requires
an explicit owner root and the same profile agreement. A stale section, unsupported
revision, missing entry or altered artifact fails startup.

## Artifact And Usage Value Admission

Upload receipts carry distinct nominal upload and occurrence UUIDv7 identities,
complete owner fields, a matching plane URI, an unsigned 64-bit byte count and a
fixed lowercase SHA-256 digest. The SDK refuses retired or mixed field names
before an installed receipt consumer opens the Artifact plane.

Artifact metadata admits canonical UUIDv7 identities, matching neutral or domain
presentation addresses, unsigned 64-bit byte lengths and the Artifact owner's
release vocabulary and timezone-aware timestamps. Known compliance fields use the identity and invocation
profiles shared with the gateway. Unknown metadata extensions remain admitted.
Metadata and compliance values are immutable; presentation and download-location
removal reconstruct admitted values. The stream client parses the neutral owner
address before opening a connection. Head and object reads compare the returned
occurrence with the requested identity before exposing metadata or delivering bytes. Capability Tasks use a distinct UUIDv7 type
with RFC variant admission and canonical alias output, matching the Rust owner.

Usage records admit finite quantities and amounts without constraining their sign.
Reports check every record's Task parent and the actual-over-estimate selection,
common currency and sum used by their builder. Generic MCP Task IDs remain opaque;
the native owner query checks its selected UUIDv7 parent separately. Usage models
do not establish caller permission, provider billing accuracy or an owner's
resource route. SQL selection and the template's typed address own those checks.

## Offset Pagination

The hosted list helper shares the 64-bit Rust `usize` offset profile: `v1:` followed by ASCII decimal digits, optionally preceded by one `+`. Leading zeros are admitted without a wire-length limit. Values must fit unsigned 64 bits; nondecimal, Unicode and overflowing values produce `PaginationError` before listing. Emission uses `v1:N` without a sign or leading zeros. The local `Page` attribute names are internal Python values; MCP result aliases belong to the upstream SDK.
