# Time MCP Design

This document is the canonical design and operational contract for the
`time-mcp` crate.

`time-mcp` gives agents one temporal authority for civil time, military date-time
groups, GNSS time, mission epochs, operational calendars, clock quality, and
temporal events. Every resolved instant carries the authority releases and
uncertainty used to interpret it. Map and Optimization can therefore consume time
without reconstructing timezone or leap-second assumptions.

## Status

Implemented in this workspace.

The implementation includes typed temporal contracts, an authority-bound engine,
SurrealDB records, controlled IANA data acquisition, atomic release activation,
clock-quality observation, MCP discovery surfaces, durable Task API operations,
an explicitly declared administrative HTTP projection, gateway policy, Helm,
and offline image registration.

The canonical service identity is:

```text
crate       veoveo-time-mcp
folder      servers/time-mcp
slug        time
URI scheme  time
MCP         /time/mcp
admin REST  /time/admin
health      /time/healthz
port        8800
```

Gateway-mounted tools use names such as `time__resolve_time`. Resource identities
retain the `time://` scheme.

## Standards And Protocols

| Standard or protocol | Implemented profile |
|---|---|
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | Version `2026-07-28`, JSON-RPC 2.0 over Streamable HTTP, under Veoveo hosted MCP contract revision 3. The server exposes tools, resources and templates, prompts, completions, subscriptions, notifications, and typed structured content. |
| MCP Apps SEP-1865 / `io.modelcontextprotocol/ui` `2026-01-26` | The server-owned `ui://time/timeline.html` Timeline exposes clock authority, calendars, epochs, windows, and events. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Temporal expressions, authority bindings, calendars, epochs, windows, clock evidence, tasks, and results. |
| [Veoveo concrete resource components](../../platform/types/DESIGN.md#concrete-resource-components) | All resource addresses use the shared URL 2.5.8 and percent-encoding 2.3.2 profile with Time's route, ID, and cursor validation. |
| [URI Template RFC 6570](https://www.rfc-editor.org/rfc/rfc6570) | Discovery declares simple ID variables, reserved expansion for slash-separated zone keys, and form-style cursor queries. The server parses concrete addresses through its typed resource contract. |
| MCP Tasks extension `io.modelcontextprotocol/tasks` | Version `2026-07-28`; schedule expansion and timeline validation use durable, resumable task operations. |
| [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339.html) | UTC and numeric-offset timestamp input and canonical UTC output, including explicit leap-second handling. |
| [RFC 9557](https://www.rfc-editor.org/rfc/rfc9557.html) | Timestamp input with an IANA time-zone annotation and explicit ambiguity policy. |
| IANA Time Zone Database and [TZif RFC 8536](https://www.rfc-editor.org/rfc/rfc8536.html) | Versioned civil-time authority, compiled release products, zone completion, and fold/gap resolution. |
| IANA `leap-seconds.list` | Versioned TAI-UTC transition authority. Every canonical instant binds its TZDB and leap-second releases. |
| TAI, UTC, TT, TDB, GPS, and Galileo system time | Typed projections from one integral TAI instant. The server records authority and uncertainty rather than treating scales as interchangeable strings. |
| [NTPv4 RFC 5905](https://www.rfc-editor.org/rfc/rfc5905.html) and [Network Time Security RFC 8915](https://www.rfc-editor.org/rfc/rfc8915.html) | Approved node clocks may use NTP/NTS. Time MCP consumes a bounded `ntpd-rs` observation; it does not act as an NTP network endpoint. |
| HTTPS | Registered IANA authority sources are acquired under fixed host, media, digest, size, and elapsed-time policy. |
| OAuth bearer and signed JWT identity | Read, schedule, event, task, and authority-administration scopes are fixed by gateway policy and verified again in the hosted server. |
| SurrealDB 3.2.4 and SurrealQL | Private runtime persistence uses Store's qualified Rust SDK version and connection. Bound parameters carry domain values; SQL applies catalog visibility, ordering, paging and activation transactions. Store owns schema migrations. |

## Domain Contract

Time answers four operational questions:

- What physical instant does an expression identify under the active authority?
- How does that instant project into another zone or time scale?
- Which intervals and precedence constraints apply to an operation?
- Is the host clock good enough for the requested policy?

The canonical instant is `TimeInstant`:

```text
tai_seconds_since_1970
nanosecond
uncertainty_nanoseconds
authority.tzdb_release_id
authority.leap_seconds_release_id
```

Integral TAI seconds make ordering and interval arithmetic independent of civil
clock changes. Nanoseconds retain subsecond coordinates. The uncertainty field is
carried forward as evidence rather than folded into the timestamp. Authority ids
bind every instant to one TZDB release and one leap-second release.

Intervals are half-open `[start, end)`. This convention makes adjacent shifts,
reservations, and routing windows compose without double-counting their shared
boundary.

## Architecture

### Library Features

The server library owns the public `contract` module. Consumers select
`default-features = false, features = ["contract"]` to reuse temporal IDs, DTOs,
authority references, and the `TimeScope` enum. This feature depends on
`veoveo-types`, Serde, Schemars, and Chrono's date/time representation with clock
support disabled. It includes no MCP transport, asynchronous runtime, database
client, acquisition engine, or provider dependency.

`runtime` adds the temporal engine, catalog, acquisition service, clock observation,
and application state. `mcp` adds the HTTP and MCP adapters and enables `runtime`.
The default feature is `mcp`; the binary requires it. Native catalog tests require
`runtime`. Contract consumer tests require only `contract`. The runtime still uses
shared task and identity contracts; only the contract feature promises dependency
isolation from those components.

`TimeScope` declares read, schedule, timeline, event-write, and administrative wire
names once through `scope_enum!`. MCP handlers and Task admission use the enum when
checking authenticated grants. Administrative configuration accepts a validated
`ScopeName`, allowing installation-defined names, and defaults to `TimeScope::Admin`.
Unrelated scope names in a caller's grant set remain valid.

`mcp/setup.rs` implements `McpServerContract` with `TimeScope` and `TimeResource`.
The HTTP process validates its `McpServerSetup` before serving. MCP initialization,
resource discovery, and domain scope membership consume that setup. Resource
descriptors are built from typed addresses, including the Timeline App and embedded
documents. The setup preserves App metadata and Time's reserved-expansion and query
templates; the owning resource tests qualify their routes. Installation-defined
administrative scopes continue through their configured `ScopeName` policy.

Template descriptors use `McpResourceTemplate` and the foundational RFC 6570 type.
Native contract tests expand every Time declaration through iri-string and compare
the result with `TimeResource` construction. Reserved zone names preserve slashes
and plus signs; collection templates cover both an absent cursor and each typed
cursor envelope. Concrete resource reads continue through the same owning parser.

`TimeResource` owns every resource route and implements `ResourceAddress`. Its variants
carry the corresponding ID, version, zone key, or collection cursor. The parser uses
the shared URI components and the builder emits one spelling for each address. Reads
and subscriptions use this same contract. Unsupported parameters, fragments, encoded
ID aliases, relative paths, and incorrect ID families are rejected before dispatch.
`TimeAuthorityReleaseUri` restricts provenance fields to the authority-release variant
and requires an `AuthorityReleaseId` when constructed.

`TimeVersion` validates the positive signed database range. `TimeZoneId` validates a
relative TZDB key with nonempty ASCII name components and a maximum of 1024 bytes;
the active authority determines whether that zone exists. It preserves slash-separated
names and literal plus signs in the public resource path.

`CalendarCursor`, `EpochCursor`, and `EventCursor` retain their family's typed IDs and
validate the v1 envelope, position, and collection name during deserialization. Page
responses serialize these types as opaque strings. Catalog methods require the matching
cursor type; admin query extraction and event recovery use those same types. The
contract feature includes Serde JSON and hex for this wire format.

Temporal IDs validate their domain prefix, length, and character set during JSON
deserialization as well as construction. `TimeAccessContext` carries tenant and
principal selection for database access; it is distinct from an authorization scope.
Resource builder adoption is tracked in the
[foundations plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md#modular-types-and-server-contracts).

### Hosted Process

```text
agent
  |
  | MCP and Task API
  v
mcp-gateway
  |
  | signed internal identity
  v
time-mcp container
  |-- temporal engine
  |-- MCP resources, tools, prompts, completions, and subscriptions
  |-- durable task adapter
  |-- administrative REST
  |-- IANA acquisition and zic compilation
  |-- ntpd-rs observation adapter
  |-- SurrealDB platform store
  `-- persistent authority release volume
```

The Rust process owns the HTTP server, acquisition jobs, active authority cache,
event watchers, and Task API workers. The same image contains `zic`, the bootstrap
TZif database, and the bootstrap IANA leap-second file supplied by Debian `tzdata`.

## Authority Data

Time uses two independently versioned authority families.

| Family | Authoritative content | Runtime product |
|---|---|---|
| `tzdb` | IANA Time Zone Database source release | compiled TZif directory |
| `leap_seconds` | IANA `leap-seconds.list` | validated TAI-UTC transition table |

An installation begins with the TZif and leap files in the image. Their bootstrap
release ids are explicit configuration values. Tenant administrators can register
HTTPS sources, acquire new content, inspect staged releases, and activate one
release per family.

An `AuthorityBinding` selects the active pair. Activating one family preserves the
current release of the other family. The engine reloads the pair as one context,
which keeps civil and physical projections coherent.

Every effective family also carries a compiler-ready `TimeAuthorityReference`.
The reference names its immutable `time://authorities/releases/{release_id}` URI,
release id, dataset kind, version label, and canonical SHA-256 digest. Acquired
releases name the registered source and producing acquisition. Image-provided
authorities use the explicit `bootstrap` source kind and a digest of the packaged
source file. Consumers can therefore admit a result from its returned references
without searching the authority catalog.

### Acquisition Flow

The administrative acquisition job runs inside `time-mcp`:

1. Resolve the enabled registered source under the authenticated tenant.
2. Download over HTTPS with redirects disabled, a response deadline, a media-type
   check, and a byte limit.
3. Compute SHA-256 while streaming to the private acquisition workspace.
4. Compare an optional expected digest before interpreting the content.
5. Validate leap data or safely extract and compile a TZDB source archive with
   `zic`.
6. Load representative TZif records or parse the leap table.
7. Move the product into an immutable release directory and create a staged
   release record.

Archive extraction accepts regular files and directories. Absolute paths, parent
traversal, device entries, links, and expanded content beyond the configured limit
are excluded from the product.

Acquisition idempotency is scoped to tenant, principal, and idempotency key. A
repeated request returns its existing job when source and expected digest match.
Cancellation produces the terminal `cancelled` state and removes scratch content.

### Activation

Activation uses two checks before publication:

- optimistic versions for the staged release and active family pointer;
- a full load of the prospective TZDB and leap-second pair.

The SurrealDB transaction marks the candidate active, advances the family pointer,
and retires the superseded release. The process then replaces the tenant engine and
notifies subscribers of `time://authorities/current`.

Authority release records retain source id, source URL, SHA-256 digest, retrieval
time, validation time, version label, artifact path, lifecycle state, and optimistic
record version.

## Accepted Time Expressions

`resolve_time` accepts a tagged `TimeExpression`.

| Format | Contract |
|---|---|
| `rfc3339` | UTC or fixed-offset timestamp |
| `rfc9557` | timestamp with an IANA zone annotation and explicit disambiguation |
| `civil` | local date-time, IANA zone, TZDB release id, and fold/gap policy |
| `unix` | seconds and nanoseconds since the Unix epoch |
| `tai` | TAI seconds and nanoseconds since 1970-01-01 TAI |
| `gps` | GPS week and seconds of week |
| `julian_tai` | Julian day in the TAI scale |
| `military_dtg` | `DDHHMMZMONYY` or `DDHHMMSSZMONYY` with a NATO zone letter |
| `epoch_relative` | mission epoch id and signed nanosecond offset |

Civil folds and gaps default to `reject`. Callers select `earlier` or `later` when
the operation has an explicit policy. Military zone `J` requires an IANA zone and
is rejected by the DTG parser because `J` denotes local time rather than a fixed
offset.

Resolution returns the canonical instant together with UTC RFC 3339, an explicit
`utc_is_leap_second` flag, military DTG, Unix seconds, GPS week/seconds when the
instant follows the GPS epoch, and Julian TAI day. A positive leap second keeps its
`:60` representation instead of collapsing onto an adjacent UTC second. Resolution
and conversion return the two effective authority references. They do not attach a
current clock observation because supplied-instant conversion is deterministic.

`convert_time` validates the instant's authority binding before projecting it. It
can return selected IANA zoned values and UTC, TAI, TT, TDB, GPST, and GST scale
representations.

## Operational Calendars

An `OperationalCalendar` is immutable by `(calendar_id, version)`. It declares one
IANA zone, local windows, daily or weekly recurrence, optional weekday filters,
count or until bounds, excluded civil dates, and caller-defined labels.

`expand_schedule` resolves the local windows through the active TZDB and clips them
to an authority-bound horizon. The output is ordered, numbered, and bounded by
`maximum_occurrences`. Expansion understands offset changes because every occurrence
is resolved from its local civil time rather than by adding fixed UTC durations.

The operation accepts at most 1,000,000 returned occurrences and uses a bounded
calendar search horizon. Invalid local times remain explicit errors unless the
calendar chooses a supported unambiguous time.

## Mission Epochs And Timelines

A mission epoch gives a named physical instant a version. Agents can then resolve
expressions such as an offset from launch, H-hour, or the start of a convoy window.
Each calculation forks the tenant's authority engine and loads only the epoch ids
named by its input expressions. SurrealDB selects the latest version of each key
inside the caller's tenant, in batches of at most 100 keys. The request has a
30-second epoch-read deadline and accepts at most 100,000 expressions. A calculation's
epoch set belongs to that calculation, so concurrent requests cannot replace it.
An exact epoch resource read selects its tenant and epoch key in SurrealDB, orders
versions descending, and fetches one record. Clock observations, exact resource reads,
and zone completions use the authority cache without loading the epoch catalog.

`validate_timeline` resolves named points and evaluates directed constraints. Each
constraint identifies a predecessor, successor, minimum separation, and optional
maximum separation. The result reports every violated constraint by input index.

Timeline evaluation accepts up to 100,000 points and 1,000,000 constraints in one
task. Point names must be unique, referenced points must exist, and a maximum cannot
be below its minimum.

## Window Algebra

`evaluate_windows` calculates union, intersection, and difference over authority-
bound half-open windows. Adjacent ranges coalesce during union. Every bound in one
request must use the active authority pair, which makes the result suitable for
direct use by routing, scheduling, and optimization tools.

## Clock Quality

Clock synchronization belongs to node infrastructure. An installation can run
`ntpd-rs` with NTS, hardware timestamping, PTP-backed sources, GNSS, or another
approved reference architecture. `time-mcp` consumes the resulting observation and
turns it into agent-visible evidence.

The implemented adapter reads one bounded JSON observation from an ntpd-rs Unix
socket. It projects:

- synchronized state and stratum;
- estimated offset and conservative error bound in nanoseconds;
- independent source count;
- holdover age when the adapter supplies it;
- NTP/NTS traceability labels and observation time.

`assess_clock` compares the observation with an explicit request policy or the
tenant's stored default. Policy controls maximum error, maximum stratum, minimum
source diversity, and maximum holdover age. An installation without an observation
socket reports an unmeasured system clock with an unbounded error estimate.

`ClockQualityPolicy` exposes a named builder and read-only accessors. JSON decoding
uses the same validation. Maximum error in nanoseconds and maximum holdover in
seconds must lie in `1..=i64::MAX`; maximum stratum is `1..=15`, and minimum source
diversity is `1..=u32::MAX`. The published schema declares those bounds. Assessment
requests and persisted policies share this contract.

The health endpoint proves that the authority and configured clock adapter can be
read. Mission acceptance remains a policy decision returned by `assess_clock` and
`time://clock/quality`. The clock-derived `time://clock/current` resource includes
the effective policy and measured quality, including holdover age.

## MCP Surface

### Tools

| Tool | Scope | Execution | Result |
|---|---|---|---|
| `resolve_time` | `time:read` | direct | canonical instant and projections |
| `convert_time` | `time:read` | direct | selected zone and scale projections |
| `assess_clock` | `time:read` | direct | clock observation, policy, and violations |
| `evaluate_windows` | `time:schedule` | direct | normalized interval set |
| `expand_schedule` | `time:schedule` | Task API required | bounded calendar occurrences |
| `validate_timeline` | `time:timeline` | Task API required | constraint verdict and violations |
| `create_temporal_event` | `time:event:write` | direct | owner-scoped event |
| `cancel_temporal_event` | `time:event:write` | direct | cancelled event version |

Every successful tool result contains typed structured content. Tool annotations state
read-only, destructive, idempotent, and open-world behavior.

### Task API

Schedule expansion and timeline validation use the final Task API extension. A normal
tool call returns an instruction to invoke the task form. The Task adapter supports
create, get, update, cancel, list discovery, and task subscriptions.

Tasks persist through `veoveo-task-runtime` in SurrealDB. Ownership includes tenant,
principal, profile, server, and data labels. Workers claim 120-second leases and renew
them every 40 seconds. Both temporal task types use `Resume` recovery because their
outputs are deterministic under the persisted authority-bound request. Terminal task
records retain for seven days unless a retention pin extends their lifetime.

### Zone Completion Template Compatibility

Time advertises `time://zones/{+zone_id}`, which preserves the slash and plus signs
in a zone key. The public completion adapter also accepts the v1 reference spelling
`time://zones/{zone_id}` through the 0.1.x support series. Both spellings select the
same zone completion handler after the same `time:read` check. The adapter does not
change resource reads, policy targets, or the concrete zone URI format.

Clients refresh `resources/templates/list` and use the advertised template for new
completion requests. The Time server owns this adapter. Removal may occur in 0.2.0
only after installed acceptance proves refreshed discovery and completion for both
profiles and supported clients have adopted the advertised form. Upgrade from v1-only
servers requires a drained Time service and refreshed template discovery before
traffic resumes; overlap with v1-only replicas is unsupported. Rollback also drains
the service and refreshes discovery. Neither direction converts persisted data.
Native cases qualify both exact reference spellings and leave unrelated references
unchanged. The reference installation's rebuild must qualify that drain and discovery
refresh before release, as tracked in the foundations plan.

### Resources

| URI | Content |
|---|---|
| `time://clock/current` | current resolved instant with effective policy and measured clock quality |
| `time://clock/quality` | measured clock-quality record |
| `time://authorities/current` | effective compiler-ready authority references, including bootstrap authorities |
| `time://calendars` | a page of tenant calendar versions |
| `time://epochs` | a page of tenant mission epoch versions |
| `time://events` | a page of owner-scoped temporal events |

`resources/list` advertises the stable roots, documents, and Timeline App. Domain
records are reached through the collection pages and exact URI templates. Changes
to domain records invalidate resource contents; the discovery inventory is static
and does not advertise resource-list change notifications.

Calendar, epoch, and event pages contain `items`, `limit: 100`, and `next_cursor`.
Pass the opaque cursor through the root's `?cursor=` query parameter. SurrealDB
applies authorization and the cursor position before its 101-record lookahead.
Calendars and epochs sort by key ascending and version descending. Events sort by
TAI seconds, nanosecond, then event key ascending. These positions use immutable
fields. Each page observes the records present at its read. Inserts before its position
appear on a fresh walk. The administrative calendar and epoch lists use the same
pages and optional cursor query parameter.

Subscribe to the collection root to invalidate its pages. Cursor URIs do not accept
separate subscriptions. Timeline uses the shared workbench's Previous and Next
controls, reads one page at a time, and refreshes the current cursor after a root
notification. Selecting a different resource starts at its first page.

Resource templates expose:

```text
time://zones/{+zone_id}
time://authorities/releases/{release_id}
time://calendars{?cursor}
time://epochs{?cursor}
time://events{?cursor}
time://calendars/{calendar_id}/versions/{version}
time://epochs/{epoch_id}
time://events/{event_id}
```

Completions enumerate the packaged IANA zones and visible calendar, version,
epoch, and event ids. Store-backed completion queries apply tenant and event-owner
predicates, text matching, distinct selection, and ordering in SurrealDB before
fetching at most 101 candidates. MCP returns 100 values and sets `hasMore` when a
further candidate exists. It reports a total only when the complete match set fits.
Calendar-version completion narrows its SQL query to the selected `calendar_id`
when the client supplies that argument in completion context.
Calendars, epochs, authorities, clock quality, and events emit resource updates.
Reading an event or an event page restores its scheduled watchers. An event-root
subscription restores all of its owner's scheduled events through SQL-filtered
pages, with a 60-second deadline. A timeout rejects listener establishment instead
of claiming complete recovery. Exact event subscriptions restore that event's
watcher. Resource discovery does not start watchers.

### Prompts

| Prompt | Purpose |
|---|---|
| `resolve_operational_time` | guide authority-aware normalization |
| `expand_operational_calendar` | prepare a bounded Task API expansion |
| `validate_mission_timeline` | prepare named points and separation constraints |

## Temporal Events

Temporal events are owner-scoped durable records. Creation validates an authority-
bound due instant and uses a principal-scoped idempotency key. A watcher transitions
the record from `scheduled` to `due` under optimistic concurrency, then emits updates
for the collection and event URI. Cancellation updates the durable record and cancels
the local watcher.

Event reads, collections, due queries, and optimistic transitions bind both the
tenant and authenticated owner in SQL. Completion applies the same owner predicate
before its limit. The catalog decodes only the records selected by those queries;
notification scheduling uses those owner-scoped reads.

## Administrative HTTP Projection

Time retains this accepted projection for installation authority workflows
that do not yet use an equivalent MCP administration surface. It reuses the
same typed temporal models, authorization, catalog, and task state, and it is
not an alternate authority. The gateway exposes it through
`/admin/{profile}/servers/time/{*path}`. The upstream server receives it at
`/time/admin/{path}` and requires `time:admin` in the signed internal identity.

| Method and path | Operation |
|---|---|
| `GET /sources` | list registered authority sources |
| `POST /sources` | create a source |
| `GET /sources/{source_id}` | read a source |
| `PUT /sources/{source_id}` | replace a source under optimistic concurrency |
| `GET /acquisitions` | list acquisition jobs |
| `POST /acquisitions` | start an idempotent acquisition |
| `GET /acquisitions/{acquisition_id}` | read acquisition progress |
| `POST /acquisitions/{acquisition_id}/cancel` | request cancellation |
| `GET /releases` | list authority releases |
| `GET /releases/{release_id}` | read release provenance |
| `POST /releases/{release_id}/activate` | preflight and activate a staged release |
| `GET /active-authorities` | read active family pointers |
| `GET /calendars` | list calendar versions |
| `POST /calendars` | create an immutable calendar version |
| `GET /calendars/{calendar_id}/versions/{version}` | read a calendar version |
| `GET /epochs` | list mission epochs |
| `POST /epochs` | create a mission epoch version |
| `GET /epochs/{epoch_id}` | read the latest epoch version |
| `GET /clock-policy` | read the tenant clock policy |
| `PUT /clock-policy` | replace clock policy under optimistic concurrency |

Administrative errors use a typed body containing `code`, `message`, `retryable`, and
`trace_id`. The gateway applies `admin_read` or `admin_write` policy and records the
proxied operation in the standard audit path.

## Persistence

SurrealDB is the canonical temporal catalog and task store.

The runtime's private `persistence/` modules own temporal queries, driver records,
mutation drafts and validation. `PlatformStore` supplies the connection and platform
identity; Store owns the migration catalog. Time uses the same pinned SurrealDB 3.2.4
SDK as Store, behind its `runtime` feature. Contract-only consumers do not resolve it.

Catalog calls retain source, release, acquisition, calendar, epoch and event ID types
through the persistence interface. Calendar, epoch and existing-record guards use `TimeVersion`.
Collection queries accept `CalendarCursor`, `EpochCursor` or `EventCursor` directly;
requested epoch batches and completion parents also retain their domain IDs. The
driver binds their text and numeric values alongside typed database identity records.
Only fixed table/field choices and fixed activation clauses enter SQL construction.
Tenant and event-owner predicates, sorting, grouping and limits execute in SurrealDB.

Public Time IDs accept bounded prefixed names because bootstrap authority provenance
can name image-provided releases. Stored catalog keys require the corresponding
prefix followed by a UUID whose version is 7, preserving the existing persistence
profile. Public syntax does not establish that a stored record exists. Bootstrap
references keep their public spelling and are not converted into database keys.
The persistence admission check applies before stored reads and writes. It does not
narrow public provenance deserialization or rewrite retained keys. Store's migrations
define tables and record fields; the Time contract defines JSON bodies and cursor versions.
Broader DTO field typing remains in the foundations plan.

| Table | Responsibility |
|---|---|
| `time_source` | registered IANA source endpoints and media policy |
| `time_authority_release` | immutable release provenance and lifecycle |
| `time_active_authority` | one optimistic pointer per tenant and data family |
| `time_acquisition` | idempotent acquisition state and progress |
| `time_calendar_version` | immutable operational calendars |
| `time_mission_epoch` | versioned named physical instants |
| `time_temporal_event` | owner-scoped scheduled events |
| `time_clock_policy` | tenant clock acceptance policy |

All tables are schema-full and carry a 30-day changefeed. Platform migrations create
their fields and indexes during installation bootstrap. The server connects with the
database-scoped runtime identity and never applies migrations.

### Version Guards

`TimeVersion` admits positive integers through `i64::MAX`. Source replacement,
event transitions and release activation require it. Clock-policy replacement and
active-pointer admission use `TimeWriteGuard::Absent` or `Existing(TimeVersion)`;
serialization keeps the established numeric zero/positive representation. Source
creation keeps its separate `record_version: 0` request sentinel. These request
schemas reject values outside their storage range before dispatch.

Every lifecycle increment checks the storage maximum. Exhaustion fails the write
without wrapping, resetting or changing the stored value. Release activation checks
both proposed versions before dispatch. Retirement increments the previous release
inside that transaction with a positive, below-maximum version predicate; failure
rolls back the candidate release and pointer together.

### Retained Catalog Metadata

`catalog/records.rs` converts SQL-selected rows into public values. It checks each
stored UUID key against the typed ID in the JSON body and the physical record ID.
Calendar and epoch keys include their positive version. Event and epoch nanoseconds
must be below one billion. SQL applies tenant and event-owner visibility before this
conversion; a visible inconsistent record fails its read or page without being dropped
from the results. Reads never repair or rewrite retained data.

| Entity | Body fields that must agree with the row | Values taken from lifecycle columns |
|---|---|---|
| Source | ID, name, dataset kind, URL, content type, enabled flag | record version |
| Authority release | release and source IDs, dataset kind, version label, URL, digest, artifact path, retrieval and validation times | state, record version |
| Acquisition | acquisition and source IDs, expected digest | status, phase, staged release ID, record version, update time |
| Calendar | ID, version, name, zone | none |
| Mission epoch | ID, version, name, TAI seconds and nanosecond | none |
| Temporal event | ID, name, due TAI seconds and nanosecond | state, record version |

Lifecycle columns can advance while a historical JSON body keeps its earlier values.
Retirement uses this rule for release state and version. Acquisition creation time
in the public body records enqueue time; the row's creation time records insertion.
Acquisition updates preserve the public creation time, source and expected digest.
The catalog checks those values before dispatch, and the SQL version predicate fences
competing updates. Stored lifecycle versions must be positive. A staged acquisition
release reference must satisfy the stored release-ID profile. Body decoding and
consistency errors identify the entity and field without quoting stored content.

`catalog/clock.rs` checks the physical clock-policy ID and tenant, converts stored
signed scalars without narrowing, and admits the policy and version through their
contract types. Invalid scalar diagnostics name the field without quoting stored
values. Reads leave rejected rows unchanged.

The metadata admission profile uses the existing tables and JSON representation.
The Time owner must preflight retained catalogs before an installation upgrade,
checking bodies and keys under each tenant and event owner, plus clock scalar bounds
and positive lifecycle versions. Clients must send policies within the declared bounds;
zero-version update requests are supported only where the absence guard applies. Export rejected rows
for investigation and correct them through an explicit operator repair before retrying.
Drain Time requests, acquisition workers and event watchers during the coordinated
upgrade; overlapping readers could otherwise disagree about corrupt records. Keep a
database snapshot and the prior image for rollback. The upgrade performs no conversion,
and rollback restores that snapshot with the prior image. The disposable reference
installation uses the foundations plan's authorized reset. Installed preflight and
rollback qualification remain pending.

Active-authority pointer/parent consistency and broader public DTO construction have
separate work in the foundations inventory.

Compiled authority products live under `/var/lib/veoveo/time/releases`. Acquisition
scratch data lives under `/var/lib/veoveo/time/acquisitions` and is removed at terminal
completion. Kubernetes mounts `/var/lib/veoveo/time` from a persistent volume.

## Authorization

The gateway profile and server both enforce domain scopes.

| Scope | Capability |
|---|---|
| `time:read` | authority, clock, conversion, resources, prompts, completions |
| `time:schedule` | window algebra and calendar expansion |
| `time:timeline` | mission timeline validation |
| `time:event:write` | create and cancel owner events |
| `time:admin` | sources, acquisitions, releases, calendars, epochs, clock policy |

Every request also carries the selected gateway profile, principal, tenant, roles,
data labels, and policy evidence in the signed internal identity. Task and event
ownership is derived from that identity.

## Deployment

The image runs as UID 10001 with a read-only root filesystem under the Kubernetes
security profile. Writable paths are the Time persistent volume and `/tmp`. The
container includes CA roots, `tzdata`, `zic`, and the single Rust service binary.

Helm installs one replica with a `ReadWriteOnce` PVC because release activation and
local event watchers are process-owned. The gateway reaches the service only over the
cluster network. SurrealDB retains the durable coordination state.

Connected installations grant the pod HTTPS egress only to approved IANA mirrors or
installation-controlled authority endpoints. Source endpoints remain tenant admin
records. Offline installations use the image bootstrap authority and can stage
content through an approved internal HTTPS endpoint.

The optional ntpd-rs observation socket is supplied with
`--ntpd-observation-socket`. A node deployment mounts that Unix socket and grants the
Time process read/connect access according to the installation's clock architecture.

## Cross-Domain Use

Map requests can carry canonical `TimeInstant` values for departure time, restriction
validity, traffic snapshots, weather windows, tides, and authority-effective routing.
Optimization can expand calendars and validate timeline constraints before assigning
vehicles, crews, facilities, and route legs. Frames can use mission-relative epochs for
sensor and platform transformations while keeping Earth geography in Map.

Examples of agent requests include:

- Resolve `141530ZJUL26`, then convert it to the warehouse and destination zones.
- Assess whether this node meets a 5 ms error budget with two independent sources.
- Expand calendar version 4 across the convoy planning horizon through the Task API.
- Validate that border clearance follows arrival by 15 to 45 minutes.
- Create an event at H-hour plus 90 seconds and subscribe to its resource.
- Intersect crew availability, port access, and daylight windows.

## Implementation Map

| Path | Responsibility |
|---|---|
| `src/contract/` | strong ids, time expressions, calendars, events, admin models |
| `src/authority.rs` | TZif context and IANA leap-second interpretation |
| `src/engine.rs` | resolution, projection, recurrence, timelines, interval algebra |
| `src/clock.rs` | observation adapter and clock-policy assessment |
| `src/contract/clock_policy.rs`, `src/contract/version.rs` | validated policy builder, numeric request schemas, positive versions and optional-row guards |
| `src/catalog/clock.rs` | checked stored clock-policy scalars, identity and version |
| `src/catalog.rs`, `src/catalog/pages.rs` | typed catalog operations, domain body decoding, collection envelopes and completion |
| `src/catalog/records.rs` | retained body/key and indexed-field checks, lifecycle-column decoding and redacted metadata errors |
| `src/persistence/` | private typed query/mutation interfaces, SurrealDB driver records, admission, SQL visibility and atomic activation |
| `src/index.rs` | collection-bound opaque cursors and page envelopes |
| `src/registry.rs` | tenant authority caches and activation preflight |
| `src/acquisition/` | bounded download, validation, compilation, staging, cancellation |
| `src/admin/` | typed administrative routes and errors |
| `src/mcp.rs` | MCP tools, resources, templates, completions, subscriptions |
| `src/prompts.rs` | reusable temporal interaction prompts |
| `src/server/tasks.rs` | final Task API adapter, leases, recovery, subscriptions |
| `src/server/` | configuration, internal auth, host checks, HTTP assembly |
| `platform/store/migrations/0019_time_domain.surql` | temporal schema and indexes |

## Verification

Unit tests cover leap authority validation, positive-leap projection, RFC/GPS/DTG
equivalence, DST ambiguity, DST-aware schedule expansion, half-open interval algebra,
timeline violations, clock policy, canonical URIs, acquisition configuration, and
archive traversal rejection.
Time runtime tests cover stored URL/ID admission, SQL isolation and pagination, requested
epoch batches and completion. Retained-metadata cases corrupt rows through a separate
fixture connection, verify read/collection rejection, preserve lifecycle columns, and
reject acquisition identity changes before writing. Isolated authority tests use separate connections to
qualify retirement, competing pointer updates and exhaustion rollback followed by retry.
Numeric contract cases compare schema bounds with JSON admission, and compile-fail
examples reject unchecked construction. Native scalar cases reject negative, zero,
truncated and exhausted values without changing rows; competing clock replacements
admit one writer. Store tests own schema migrations.
Gateway validation,
Helm rendering and linting, the container build, and the shared SurrealDB integration
harness exercise the deployment boundary.

## Replica Resource Observation

Each replica opens one shared group of projected Store LIVE queries for authority releases and activation, acquisitions, calendars, mission epochs, temporal events and clock policy.
Committed changes invalidate only each listener's accepted resource identities and
requested catalog. Writes coalesce over 100 milliseconds. Source reconnection invalidates readers after a delivery gap; reads retain normal
current authority. Idle sources emit no periodic resource-change notifications. This observes durable state and cannot dispatch work.
