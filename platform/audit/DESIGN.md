# Platform Audit Writer

## Standards And Protocols

| Boundary | Profile |
|---|---|
| `veoveo.ai/audit-record/v1` | Checked records from the lightweight audit contract |
| SurrealDB 3.3.0 | Store-owned atomic batches and caller-owned domain transactions |
| RFC 8785 | Canonical JSON bytes for hashing, with `serde_json_canonicalizer` 0.3.2 |
| RFC 9162 section 2.1 | SHA-256 Merkle leaves and interior nodes |
| RFC 8032 | Ed25519 signing through `ed25519-dalek` 3.0.0 |
| OCSF 1.9.0 and S3 Object Lock | [Destination export](src/export/DESIGN.md), JSON Lines and optional compliance-mode retention |
| W3C Trace Context and OTLP/HTTP | Request correlation and policy, audit and upstream histograms |

Request callers share a process-local writer. Its queue holds at most 1,024 records;
a sender waits at most one second for capacity. After receiving the first record, the
writer yields to ready tasks and drains up to 64 queued records into one Store transaction.
It adds no timer delay. Records arriving during that commit form the next group.
Each caller waits for that transaction's result. Cancelling one caller cannot cancel
other records in its group. The writer owns its worker handle. Closing all senders drains
the queue, and an explicit shutdown closes both receivers even while caller clones exist.
Gateway, Artifact, Speech and UAV hosts stop HTTP admission and then wait up to 30 seconds
for the writer. A worker failure closes admission; a missed drain deadline returns an
error and reports that records may be uncommitted. Repeated shutdown calls return the
same result.

Domain callers bind checked `AuditTransactionWrite` values and call Store's
`fn::append_audit` inside the branch that changes their state. This path opens no
independent transaction. Its maximum is 4,096 records per domain transaction.
Store compares an existing record's complete typed
draft before accepting a repeated identity; a mismatched retry fails the transaction.
Audit writes emit no outbox records.

Upload publication constructs its audit draft from the completion request context
stored on the upload and calls the append function inside the guarded publication
branch. Source review selected this call over a `DEFINE EVENT` on artifact occurrences:
the event would need an additional link to upload request context and would also run
for occurrence insertions owned by other producers. The shared append call keeps
attribution construction in Rust and uses the same transaction as the receipt and
grants. Publication rollback and idempotent retry require native qualification.

The [record contract](contract/DESIGN.md) sits below Store and protocol adapters.
Separating it from this writer prevents a dependency cycle. MCP exposes the same
canonical types, and imports no server vocabulary into its generic scope machinery.

Completion records use a separate 1,024-record queue because their effects have already
happened. The worker keeps record identities stable across failed commits and retries
with delays from 100 ms to five seconds. A caller waits for queue capacity and receives
its original tool result. Speech stops and joins dictation session workers before
draining audit. UAV stops expiry workers and closes retained live-view sessions first.
Their terminal records therefore enter the queue before the writer closes it.

## Integrity And Readers

The sealer owns a 30-second database lease, renewed every ten seconds. Every block
transaction checks its owner, generation, expiry and previous cursor. A new replica
continues from that cursor. Native LIVE notifications open a one-second batching
window. The worker then drains consecutive feed pages without another timer delay,
yielding between pages for shutdown, export and lease renewal. Initial recovery reaches
the end of the feed before reporting active readiness. Idle timers renew the lease
without scanning records.
The worker refuses a recovery cursor left unprocessed for six days, before the
seven-day change-feed window can disappear silently.

`service.rs` owns replica election and recovery. A replica with a valid competing lease
reports standby; it retries election every two seconds. Transport recovery backs off
from one to ten seconds. Integrity errors and expired recovery windows stop the worker
and fail readiness. An active or standby observation expires after 20 seconds, which
also catches a worker panic. Idle renewal refreshes recovery time only while LIVE is
connected and its cursor is caught up.

One seal page holds at most 4,096 audit records and advances only across complete
database commits. A shutdown drains every pending page, then releases the lease.
Gateway first drains its request writer and gives the sealer a further 30 seconds.
Dropping an undrained service aborts its task and reports the incomplete shutdown.

SurrealDB 3.3.0 applies the change-feed limit before its table filter. The trusted sealer
therefore advances through database change-feed pages and selects audit mutations. User
readers use partition-specific SQL and LIVE queries. The sealer's global feed is never
an audit-reader API. Blocks list record identities in commit order; their sequence and
versionstamp values serialize as decimal strings to preserve every bit through JCS.
A record-to-block table rejects sealing the same identity twice.

Verification hashes the complete typed record, including its database timestamp, using
RFC 8785. RFC 9162 leaf and node prefixes prevent the two node kinds from colliding.
Ed25519 signs a domain-separated canonical block head. Verification checks the selected
partition, sequence continuity, the previous hash, membership order and signatures. It
reports record timestamps outside the caller's clock-skew allowance. An external tail
checkpoint detects rollback of both the database blocks and its current-head pointer;
a database-only verification cannot establish that external fact.

Retention admission holds the sealer lease, advances through whole blocks in order,
and preserves the last removed signed head. When export is configured, an unexported
block cannot be deleted. The CLI's filtered export fixes the sealed tail before reading
and applies content filters in SQL. Each block read checks that its records still exist;
concurrent retention fails the export instead of silently omitting records.

The hosted retention pass admits at most 128 blocks in two seconds once per minute.
SQL selects each partition's next removable block before applying its page limit.
Download deduplication guards older than one day are removed in 128-row batches;
their audit records follow ordinary block retention. The hosted pass binds all configured
destination identities, and SQL admits only blocks acknowledged by every destination.

The gateway requires `VEOVEO_AUDIT_SIGNING_KEY_B64`, a dedicated 32-byte Ed25519 seed.
Its decoder and the key-generation command clear temporary seed buffers. The command
creates a new mode-0600 file and prints only the public key and its SHA-256 key ID.
The Helm installation supplies a separate Secret; the assertion signer owns a different
key. Keep public keys for all blocks that remain in the verification range.

The [export worker](src/export/DESIGN.md) owns S3/OTLP delivery and persisted receipts.
Qualification, remaining producer/reader cuts and installed measurements are tracked in the
[foundations plan](../../docs/PLATFORM_FOUNDATIONS_PLAN.md#phase-4-unified-audit-log).
