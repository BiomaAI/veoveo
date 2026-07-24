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
administrative REST, gateway policy, Helm, and offline image registration.

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
| [Model Context Protocol](https://modelcontextprotocol.io/specification/) | JSON-RPC 2.0 over Streamable HTTP with tools, resources and templates, prompts, completions, subscriptions, notifications, and typed structured content. |
| [JSON Schema Draft 2020-12](https://json-schema.org/draft/2020-12/) | Temporal expressions, authority bindings, calendars, epochs, windows, clock evidence, tasks, and results. |
| [Veoveo final task extension](../../mcp/task-extension) | Version `2026-06-30`; schedule expansion and timeline validation use durable, resumable task operations. |
| [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339.html) | UTC and numeric-offset timestamp input and canonical UTC output, including explicit leap-second handling. |
| [RFC 9557](https://www.rfc-editor.org/rfc/rfc9557.html) | Timestamp input with an IANA time-zone annotation and explicit ambiguity policy. |
| IANA Time Zone Database and [TZif RFC 8536](https://www.rfc-editor.org/rfc/rfc8536.html) | Versioned civil-time authority, compiled release products, zone completion, and fold/gap resolution. |
| IANA `leap-seconds.list` | Versioned TAI-UTC transition authority. Every canonical instant binds its TZDB and leap-second releases. |
| TAI, UTC, TT, TDB, GPS, and Galileo system time | Typed projections from one integral TAI instant. The server records authority and uncertainty rather than treating scales as interchangeable strings. |
| [NTPv4 RFC 5905](https://www.rfc-editor.org/rfc/rfc5905.html) and [Network Time Security RFC 8915](https://www.rfc-editor.org/rfc/rfc8915.html) | Approved node clocks may use NTP/NTS. Time MCP consumes a bounded `ntpd-rs` observation; it does not act as an NTP network endpoint. |
| HTTPS | Registered IANA authority sources are acquired under fixed host, media, digest, size, and elapsed-time policy. |
| OAuth bearer and signed JWT identity | Read, schedule, event, task, and authority-administration scopes are fixed by gateway policy and verified again in the hosted server. |

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
`:60` representation instead of collapsing onto an adjacent UTC second.

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
The active tenant engine loads the latest persisted version for each epoch id.

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

The health endpoint proves that the authority and configured clock adapter can be
read. Mission acceptance remains a policy decision returned by `assess_clock` and
`time://clock/quality`.

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

### Resources

| URI | Content |
|---|---|
| `time://clock/current` | current resolved instant with clock evidence |
| `time://clock/quality` | measured clock-quality record |
| `time://authorities/current` | active authority binding and releases |
| `time://calendars` | visible calendar versions |
| `time://epochs` | visible mission epochs |
| `time://events` | owner-scoped temporal events |

Resource templates expose:

```text
time://zones/{zone_id}
time://calendars/{calendar_id}/versions/{version}
time://epochs/{epoch_id}
time://events/{event_id}
```

Completions enumerate IANA zones and visible calendar, version, epoch, and event ids.
Calendars, epochs, authorities, clock quality, and events emit resource updates.
Subscriptions to event resources restore their due-time watcher after a process
restart or client reconnect.

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

Reading, listing, completion, and notification scheduling enforce the authenticated
owner boundary inside the tenant.

## Administrative REST

The gateway exposes this API through
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
| `src/catalog.rs` | typed platform-store projection and owner isolation |
| `src/registry.rs` | tenant authority caches, activation preflight, epoch loading |
| `src/acquisition/` | bounded download, validation, compilation, staging, cancellation |
| `src/admin/` | typed administrative routes and errors |
| `src/mcp.rs` | MCP tools, resources, templates, completions, subscriptions |
| `src/prompts.rs` | reusable temporal interaction prompts |
| `src/server/tasks.rs` | final Task API adapter, leases, recovery, subscriptions |
| `src/server/` | configuration, internal auth, host checks, HTTP assembly |
| `platform/store/src/time.rs` | SurrealDB persistence methods and validation |
| `platform/store/migrations/0019_time_domain.surql` | temporal schema and indexes |

## Verification

Unit tests cover leap authority validation, positive-leap projection, RFC/GPS/DTG
equivalence, DST ambiguity, DST-aware schedule expansion, half-open interval algebra,
timeline violations, clock policy, canonical URIs, acquisition configuration, and
archive traversal rejection.
Platform-store tests cover URL, id, migration, and policy invariants. Gateway validation,
Helm rendering and linting, the container build, and the shared SurrealDB integration
harness exercise the deployment boundary.
