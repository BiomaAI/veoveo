# Audit Log

Veoveo keeps one audit log for every component. A record answers who acted, on whose
behalf, under which policy revision, against which target, with what outcome, and
how that action correlates with its trace. The gateway, the Artifact service,
simulator live views, Computers, and upload publication all write the same record
type through one writer library. Sealed blocks make tampering detectable, and an
installation can export the log to storage its own administrators cannot rewrite.

This document is the target design. The
[implementation plan](KNOWLEDGE_AND_IDENTIFIERS_PLAN.md#phase-4-unified-audit-log)
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
| SurrealDB 3.2.4 | `audit_record` and `audit_block` tables in the platform store |

## Record

Each record is one row in `audit_record`. Its fields are typed. The record has no
free-form metadata map, so a writer cannot add an unreviewed field.

| Field | Content |
|---|---|
| `id` | UUIDv7 that the writer generates once and reuses on every retry, so a retried write stores one record |
| `schema` | `veoveo.ai/audit-record/v1` |
| `partition` | The tenant, or `installation` for events without a tenant: unauthenticated denials, client-credentials clients, and installation administration |
| `class` | `api_activity`, `authentication`, `account_change`, `artifact_activity`, `live_view_access`, or `computer_activity` |
| `activity` | Class-specific action, such as `resource_read`, `tool_call`, `token_refresh`, `download`, `open` |
| `outcome` | `allowed`, `denied`, `succeeded`, or `failed`, plus a typed reason code |
| `actor` | Principal ID and kind, tenant, OAuth client, session family, the delegating principal, and for managed agents the instance, generation, dispatch epoch, and episode |
| `authority` | Profile, Work Context, policy revision, and the principal's scopes and data labels at decision time, stored once |
| `target` | Typed target: server and tool, resource URI, artifact ID, Computer ID, live-view authorization, or administrative object |
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
| Streamed session: dictation and live views | One record when the session opens, one when it ends with counts and durations, and one for every denial or revocation; renewals and chunks write no record |
| Artifact range downloads | One record per actor, artifact, and five-minute window, and one for every denial |
| Token lifecycle: issue, refresh, revoke, replay, credential denial | One `authentication` record each |
| Successful bearer verification | No record; the actor block of the request's record carries it |
| Polling a caller's own Task or operation status | No record; the Task's creation and terminal outcome are recorded |
| Reading the audit log | One record when a Console audit view or export opens |

HTTP range requests have no end event, so a download is recorded by window. A replica
records the first range request of an actor for an artifact in each five-minute
window. A duplicate from another replica is harmless.

## Write Path

Writers call one library, `platform/audit`, with one of two modes:

- A transactional write commits the record inside the caller's domain transaction.
  Upload publication, Computers lifecycle changes, and Work Context transitions use it.
- A request write joins a group commit. The writer gathers records from concurrent
  requests for at most 5 ms or 64 records and commits them in one transaction. Each
  caller waits for its own commit.

A request fails when its required record cannot be committed. A completion record
after a tool call is the exception: the effect already happened, so the writer
retries it and emits an error metric instead of failing the response. The `allowed`
record written before dispatch already proves admission.

An authorization issuance, including a live-view authorization, requires its record.
When the audit store is unavailable, no new authorization is issued. An authorization
already issued keeps working until it expires or is revoked, and viewers and
simulation continue.

Audit writes produce no outbox events. Consumers that need audit changes read the
table's changefeed.

## Integrity

A sealer runs under a store lease, so one replica seals at a time. At most once per
second it takes each partition's unsealed records in `recorded_at` order and writes
an `audit_block`: partition, block sequence, first and last record ID, record count,
the RFC 9162 Merkle root over the records' canonical hashes, the previous block's
hash, and an Ed25519 signature over the block head. The signing key is a dedicated
installation audit key.

Changing, removing, or inserting a sealed record changes a Merkle root. Removing a
block breaks the next block's link. `audit_record` fields are `READONLY`, and only the
retention worker deletes records. `gateway audit verify` recomputes roots,
links, and signatures for a partition and time range.

The sealing interval bounds the unsealed window to about one second. Sealing runs
outside the request path, so it adds no latency to writes and needs no shared
sequence at commit time.

## Retention And Export

Each installation sets audit retention in days in its configuration. The gateway
refuses to start without it, so no default ever deletes records silently. Retention
applies to every class.

When export is configured, the retention worker deletes a record only after its
block reached the export destination. The exporter writes one OCSF JSON Lines object
per sealed block, with the block head and signature beside it, and records its cursor
in the store. An S3 bucket with Object Lock in compliance mode keeps exported blocks
beyond the reach of installation administrators.

| Record class | OCSF 1.9.0 event class |
|---|---|
| `api_activity`, `live_view_access`, `computer_activity` | API Activity (6003) |
| `authentication` | Authentication (3002) |
| `account_change` | Account Change (3001) |
| `artifact_activity` | File Hosting Activity (6006) |

The implementation confirms each class UID and required attribute against the OCSF
1.9.0 schema before it ships the mapping. The exporter can also send records
to an OpenTelemetry collector. Export delivery is at least once, and record IDs make
duplicates identifiable.

## Access

Reading the audit log requires the `audit:read` scope. A reader sees partitions of
its own tenant. Installation administrators and the auditor role also read the
`installation` partition. Every query is paged and bounded at the store with filters
for class, actor, target, outcome, trace, and time. The Console reads through this
query and exports the full filtered result on the server, not only the rows loaded in
the browser.

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
| `mcp/contract/src/audit.rs` | record, actor, authority, target, detail, and outcome types |
| `platform/audit` | writer with transactional and group-commit modes, sealer, exporter, and verification |
| `platform/store/src/audit.rs` and its migration | `audit_record` and `audit_block` tables, `READONLY` fields, indexes, bounded queries, and retention |
| `platform/gateway` | request IDs, trace context, request records, discovery aggregation, token lifecycle records, and the `audit verify` command |
| `platform/artifacts/service` | artifact activity records and download sessions |
| `servers/uav-sim-mcp` | live-view access records |
| `platform/computers` | Computer lifecycle records in domain transactions |
| `apps/console` | paged audit queries and server-side export |
