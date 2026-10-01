# Audit Log

Veoveo keeps one audit log for every component. A record answers who acted, on whose
behalf, under which policy revision, against which target, with what outcome, and
how that action correlates with its trace. The gateway, the Artifact service,
simulator live views, Computers, and upload publication all write the same record
type through one writer library. Sealed blocks make tampering detectable, and an
installation can export the log to storage its own administrators cannot rewrite.

This document is the target design. The
[implementation plan](PLATFORM_FOUNDATIONS_PLAN.md#phase-4-unified-audit-log)
replaces the current `audit_event` table with it by hard cut. Contract evolution
[CE-12](CONTRACT_EVOLUTION.md#ce-12-one-audit-record-per-logical-action) records the
decision.

## Standards And Protocols

| Standard or protocol | Profile |
|---|---|
| [OCSF](https://schema.ocsf.io/) 1.9.0 | Export schema. Each record maps to one OCSF event class; the stored record uses Veoveo field names and the exporter owns the mapping |
| NIST SP 800-53 r5 AU-2, AU-3, AU-6, AU-8, AU-9, AU-11, and AU-12 | Control objectives for event selection, record content, review, time stamps, protection, retention, and generation; Veoveo does not claim an assessed implementation |
| [W3C Trace Context](https://www.w3.org/TR/trace-context/) | Every record carries a 32-hex trace ID and a span ID; the gateway starts a trace when a request arrives without a valid `traceparent` |
| [RFC 9562](https://www.rfc-editor.org/rfc/rfc9562.html) UUIDv7 | Record IDs and request IDs |
| [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339.html) | `occurred_at` and `recorded_at` |
| [RFC 8785](https://www.rfc-editor.org/rfc/rfc8785.html) JSON Canonicalization Scheme | Canonical bytes of a record before hashing |
| [RFC 9162](https://www.rfc-editor.org/rfc/rfc9162.html) section 2.1 | Merkle tree hashing for sealed blocks, with SHA-256 |
| [RFC 8032](https://www.rfc-editor.org/rfc/rfc8032.html) Ed25519 | Signature over each sealed block head |
| S3 Object Lock, compliance mode | Optional write-once export destination owned by the installation |
| OpenTelemetry Logs over OTLP/HTTP | Optional export of records as log events named `veoveo.audit.<class>` to the installation's collector |
| SurrealDB 3.3 | `audit_record`, `audit_block`, and the `audit_daily` table view in the platform store; compound record IDs, record links, `READONLY` fields, change feeds, and LIVE queries |

## Record

Each record is one row in `audit_record`, in the platform store beside the records it
references. Its fields are typed. The record has no free-form metadata map, so a writer
cannot add an unreviewed field.

The record ID is the compound `audit_record:[partition, id]`, where `id` is a UUIDv7.
SurrealDB orders records by ID, so a partition's records in time order are a
primary-key range, such as `audit_record:[$tenant, $from]..[$tenant, $to]`, and
queries read it without a secondary time index. Sealing and retention follow commit
order instead, as [Integrity](#integrity) describes, because writer clocks can
disagree.

| Field | Content |
|---|---|
| `id` | UUIDv7 that the writer generates once and reuses on every retry, so a retried write stores one record; the second element of the record ID |
| `schema` | `veoveo.ai/audit-record/v1` |
| `partition` | The tenant, or `installation` for events without a tenant: unauthenticated denials, client-credentials clients, and installation administration |
| `class` | `api_activity`, `authentication`, `account_change`, `artifact_activity`, `live_view_access`, or `computer_activity` |
| `activity` | Class-specific action, such as `resource_read`, `tool_call`, `token_refresh`, `download`, `open` |
| `outcome` | `allowed`, `denied`, `succeeded`, or `failed`, plus a typed reason code |
| `actor` | Principal ID and kind, tenant, OAuth client, session family, the delegating principal, and for managed agents the instance, generation, dispatch epoch, and episode |
| `authority` | Profile, Work Context, policy revision, and the principal's scopes and data labels at decision time, stored once |
| `target` | Typed target. A platform record, such as an artifact occurrence, Computer, Task, principal, or Work Context, is a record link, so queries join and traverse from the record to the rest of the platform. A server and tool, a resource URI, or an administrative object uses its typed identifier |
| `detail` | One typed variant per activity, such as a tool call's result kind, duration, and JSON-RPC error code, or a knowledge read's observation |
| `request` | Request ID, trace ID, span ID, and source IP address |
| `occurred_at`, `recorded_at` | Writer clock, and database time at commit |
| `latency_ms` | Time from request receipt to record creation, when the writer measures it |

Details hold identifiers, digests, counts, durations, enums, and timestamps. They
never hold prompts, tool arguments, provider payloads, file or media bytes, tokens,
signed URLs, link bearers, webhook bodies, command text, or upstream error text. The
types enforce this list: no `detail` variant has a free-text field, and a
repository test rejects any string field outside an allowlist of identifier and
reason-code types.

## Event Selection

Veoveo writes one record per logical action. Authentication, authorization, and
execution of the same request share that record.

| Action | Records |
|---|---|
| Read: `resources/read`, a list method, completion, or a status read of domain data | One record after the result is ready and before it returns |
| Tool call or other mutation | One `allowed` record before dispatch, and one completion record after it returns |
| Denied request of any kind | One `denied` record, before any upstream call |
| Discovery list | One record with the visible-item count, the denied-item count, and a SHA-256 of the visible set; the policy revision in `authority` reproduces each item decision |
| Dictation session | One record when the session opens, one when it ends with chunk counts and durations, and one for every denial; chunks write no record |
| Live-view authorization | One record for each issuance, renewal, close, expiry, and revocation, and one for every denial |
| Artifact range downloads | One record per actor, artifact, and five-minute window, and one for every denial |
| Reads by an approved indexing client | One record per collection and five-minute window with read and outcome counts and a SHA-256 over the member URIs and revisions read; the index's chunk records hold each member's revision |
| Token lifecycle: issue, refresh, revoke, replay, credential denial | One `authentication` record each |
| Successful bearer verification | No record; the actor block of the request's record carries it |
| Polling a caller's own Task or operation status | No record; the Task's creation and terminal outcome are recorded |
| Reading the audit log | One record when a Console audit view or export opens |

HTTP range requests have no end event, so a download is recorded by window. A replica
records the first range request of an actor for an artifact in each five-minute
window. Artifact service coalesces concurrent requests through a cache of at most 4,096
acknowledged windows. Store atomically claims each actor/artifact/window with its record,
so cache eviction and replica changes preserve the same window. Denials bypass that
cache. Indexing reads use collection windows because one sync is one logical action.

Each action has one owner. The gateway records requests. A domain service records the
state changes it owns, such as an authorization issuance, a publication, or a Computer
lifecycle change, with the request ID of the call that caused it. A tool call that
issues a live-view authorization therefore has the gateway's two records and the
simulator's issuance record, which describe different facts. For artifact range
downloads, dictation chunks, and indexing reads, the window or session record is the
only record: the gateway writes no per-request record for those requests unless it
denies one.

## Write Path

Writers call one library, `platform/audit`, with one of two modes:

- A transactional write commits the record inside the caller's domain transaction.
  Upload publication, Computers lifecycle changes, and Work Context transitions use it.
- A request write joins a group commit. The writer yields to ready request tasks,
  then commits up to 64 queued records in one transaction. Records arriving during
  that commit form the next group. Each caller waits for its own commit; an idle
  request incurs no batching timer.

A request fails when its required record cannot be committed. A completion record
after a tool call is the exception: the effect already happened, so the writer
retries it and emits an error metric instead of failing the response. The `allowed`
record written before dispatch already proves admission.

An authorization issuance, including a live-view authorization, requires its record.
When the audit store is unavailable, no new authorization is issued. An authorization
already issued keeps working until it expires or is revoked, and viewers and
simulation continue.

Audit writes produce no outbox events. `audit_record` has a change feed without
`INCLUDE ORIGINAL`, because a record never changes after commit. A reader that follows
the log, such as the Console's live view or an agent that reacts to audit, holds a LIVE
query filtered by partition and resumes from its change-feed cursor after a reconnect,
as `platform/store/src/resource_changes.rs` does for Time and Recording. No reader
polls or scans other partitions.

Upload publication builds its checked draft from the persisted completion context and
appends it through Store's shared function in the publication transaction. The
synchronous `DEFINE EVENT` alternative would reconstruct that draft from domain fields
and duplicate the Rust enum and identifier conversions in SQL. The library call keeps
one draft builder and lets the publication branch choose when a state change occurred.

## Measurement

Each request reports its policy evaluation, audit commit, and upstream times
separately in its trace, and the gateway exports them as histograms. Performance work
starts from these measurements. The audit log has no fixed latency target; its
acceptance is that audit commit is not the dominant cost of catalog and read latency.

Discovery decisions are cached by caller authority, policy revision, and catalog
generation, and change events invalidate them. A list served from that cache
evaluates no policy and writes its single record.

The [paired native measurements](../platform/audit/measurements/2026-09-30.md)
record request, policy, audit and upstream timing before and after this implementation.
They qualify queued-record batching on fresh RocksDB stores and report the remaining
catalog overhead and read-tail variation. Installed latency requires its own measurement.

## Integrity

The gateway runs the sealer under a store lease, so one replica seals at a time. The
sealer follows the `audit_record` change feed, which lists records in commit order,
including a record whose writer clock lagged behind other writers. A LIVE notification
opens a one-second batching window; the worker then drains pending feed pages and writes an
`audit_block`: partition, block sequence, the change-feed versionstamp range, the IDs
of its records in commit order, the RFC 9162 Merkle root over their canonical hashes,
the previous block's hash, and an Ed25519 signature over the block head. The signing
key is a dedicated installation audit key.

`gateway audit verify` recomputes each block's root from the records it lists and
checks the links and signatures for a partition and time range. It detects a sealed
record that was changed or deleted, a removed block, and a forged signature.
`audit_record` fields are `READONLY`, and only the retention worker deletes records.
A record inserted directly with database credentials is sealed like any other record,
so verification compares the record's ID, occurrence and database timestamps with the
commit time encoded in its signed change-feed versionstamp. The Store adapter owns
this qualified single-node SurrealDB clock layout. A difference beyond the configured
clock-skew bound flags a backdated or future record. Sealer downtime can delay a block
without changing that commit time. An attacker who
holds both database root credentials and the signing key can rewrite history that has
not been exported yet; the write-once export protects everything exported before.

An available, caught-up sealer batches for about one second. An outage or backlog
extends the unsealed window, and export waits for its own committed marker. Sealing runs
outside the request path, so it adds no latency to writes and needs no shared
sequence at commit time.

## Retention And Export

Each installation sets audit retention in days in its configuration. The gateway
refuses to start without it, so no default ever deletes records silently. Retention
applies to every class.

The gateway's retention worker deletes whole blocks: a block's records, then the
block itself, once the block is older than the retention period. When export is
configured, it deletes only blocks that reached the export destination, and
verification checks the oldest retained block's link against the exported copy. The exporter writes one OCSF JSON Lines object
per sealed block, with the block head and signature beside it, and records its cursor
in the store. An S3 bucket with Object Lock in compliance mode keeps exported blocks
beyond the reach of installation administrators.

| Record class | OCSF 1.9.0 event class |
|---|---|
| `api_activity`, `live_view_access`, `computer_activity` | API Activity (6003) |
| `authentication` | Authentication (3002) |
| `account_change` targeting a principal | Account Change (3001; deprecated but supported in OCSF 1.9.0) |
| Work Context changes | API Activity (6003) |
| Records without a class's required identity or source facts | Base Event (0), preserving the typed source |
| `artifact_activity` | File Hosting Activity (6006) |

The [export design](../platform/audit/src/export/DESIGN.md) records the OCSF required
attributes, provider completion profiles and failure handling. Source mapping checks
use the upstream 1.9.0 class and object definitions; independent schema validation
remains part of qualification. OCSF events preserve the typed source under
`unmapped.veoveo`. The exporter can send the same events to an OpenTelemetry collector.
Store acknowledges each configured destination separately. Delivery is at least once,
and record IDs make duplicates identifiable.

Export and compliance-mode acceptance are tracked in the foundations plan. The bundled
RustFS configuration proves no Object Lock guarantee; gap G9 stays open until a
supporting provider passes its installed tests.

## Access

Reading the audit log requires the `audit:read` scope. A reader sees partitions of
its own tenant. Installation administrators and the auditor role also read the
`installation` partition. Every query is paged and bounded at the store with filters
for class, actor, target, outcome, trace, and time. The Console's overview reads
`audit_daily`, a table view defined with `AS SELECT … GROUP BY` that SurrealDB maintains
incrementally with counts of retained records by partition, day, class, and outcome, so
the overview scans no records. The Console reads records through this
query and exports the full filtered result on the server. Opening a view first commits
its access record. Pages and streams reference that record, and Store checks the
current actor, profile, selected partition and fifteen-minute receipt lifetime.
Current authorization still applies to every read.

Console notifications use partition-scoped LIVE queries. Sealed block sequences carry
commit-order recovery after reconnect, while unsealed records invalidate pages
immediately. Export waits for its access marker to be sealed, freezes the retained
interval and aborts if retention removes a member during the read. JSON Lines readers
require the matching completion footer before accepting a complete export.

## Correlation

The gateway assigns each HTTP request a UUIDv7 request ID. It accepts a valid inbound
`traceparent` and starts a new trace otherwise. It forwards the request ID and trace
context to upstream servers in the signed request context, so records written by the
Artifact service, Computers, and simulator carry the same identifiers as the
gateway's record.

## Scope

The log covers access and actions through Veoveo interfaces. These records stay
outside it:

- The recording ingest ledger. Successful batch appends are data-plane records in
  their own domain tables; stream lifecycle changes and every denial are audited.
- The agent kernel's Rerun decision log. It is episode memory for replay and
  evaluation. The agent's actions reach the audit log through the gateway with the
  managed-agent identity.
- Kubernetes API audit, which the installation operates.

## Implementation Map

| Path | Responsibility |
|---|---|
| `platform/audit/contract` | record, actor, authority, target, detail, and outcome types |
| `platform/audit` | writer with transactional and group-commit modes, sealer, exporter, and verification |
| `platform/store/src/audit/` and its migration | `audit_record`, `audit_block`, and `audit_daily`, compound record IDs, `READONLY` fields, record links, bounded range queries, LIVE and change-feed readers, and retention |
| `platform/gateway` | request IDs, trace context in the signed request context, request records, discovery aggregation, token lifecycle records, the sealer, exporter, and retention worker, and the `audit verify` command |
| `platform/artifacts/service` | artifact activity records and download sessions |
| `servers/uav-sim-mcp` | live-view access records |
| `platform/computers` | Computer lifecycle records in domain transactions |
| `apps/console` | paged audit queries and server-side export |
