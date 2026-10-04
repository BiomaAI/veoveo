# Audit Export

## Standards And Protocols

| Boundary | Profile |
|---|---|
| [OCSF 1.9.0](https://github.com/ocsf/ocsf-schema/tree/v1.9.0) | Typed Base Event, API Activity, Authentication, Account Change and File Hosting Activity mapping; JSON Lines, one event per source record |
| [S3 PutObject](https://docs.aws.amazon.com/AmazonS3/latest/API/API_PutObject.html) and [GetObject](https://docs.aws.amazon.com/AmazonS3/latest/API/API_GetObject.html) | SigV4, conditional object creation, SHA-256 request checksum and downloaded-byte reconciliation; optional Object Lock compliance mode |
| [OTLP/HTTP](https://opentelemetry.io/docs/specs/otlp/) | JSON Logs requests, string-encoded nanosecond timestamps, hexadecimal trace/span IDs and explicit collector acknowledgement |
| `veoveo.audit.export/v1` | Destination identity includes the wire mapping version and public destination configuration |
| RFC 8785, RFC 9162 and RFC 8032 | The companion JSON object contains the canonical signed audit block; its member records are preserved in each OCSF event |

## Configuration And Ownership

The gateway loads at most 64 KiB from `VEOVEO_AUDIT_EXPORT_CONFIG`. The closed JSON
configuration has optional `s3` and `otlp` destinations. Audit owns their types;
Store owns SQL and delivery state. S3 bucket names use lowercase letters, digits
and hyphens. Prefixes contain checked path segments. URL builders encode individual
segments, and endpoint validation rejects embedded credentials, queries and fragments.
HTTPS is required unless the installation explicitly selects `allow_http`.

S3 uses the existing `object_store` credential provider and signer. OTLP optionally
reads a bearer token from the configured environment variable. Neither credential
value enters destination identity, audit content, object keys or diagnostics. HTTP
redirects are disabled. A changed destination, prefix, Object Lock policy or mapping
version produces a new identity and requires delivery of every retained block to it.
A credential rotation preserves receipts for the same destination.

Export configuration changes require a coordinated gateway drain and restart. They
apply to retained blocks; already retired blocks must be copied from their archive
by the installation when moving to another destination.

## Delivery And Recovery

The active sealer polls the export future alongside its own LIVE subscription and
lease renewal. Export opens a separate native LIVE subscription before replaying
pending blocks. An idle exporter performs no periodic database scan. Each candidate
query compares partition heads with delivery and retention cursors in SQL before its
one-block limit. It reads the selected block by compound identity and rotates active
partitions by last delivery time; it never scans already exported block history. No request
handler waits for export.

Before sending data, the exporter checks the block's record root and commits its
destination and both payload hashes. The sealer lease fences that transaction, the
delivery acknowledgement and retention. Repeating an intent with different hashes
fails. A receipt advances the destination's partition cursor and adds its identity
to the block's delivered set in one transaction. Retention requires every configured
destination and removes per-block intents with the retired block. The last signed
retention anchor and each delivery cursor survive deletion.

Store retries an explicitly aborted transaction conflict up to eight times, with
1, 2, 4, 8, 16, 32 and 64 millisecond waits. Each attempt rechecks the lease and
preserves the block, destination and payload hashes. The retry covers only intent,
receipt or rejection persistence; it does not repeat a provider request. An expired
or replaced lease still fails its transaction.

S3 stores an OCSF JSON Lines object and a signed-block JSON object. Keys include the
destination identity, a partition digest, the block sequence and its hash. The
exporter first reads the key and compares bytes. An absent key permits a PUT with
`If-None-Match: *`; a lost response leads to another GET of that key. Both objects
must match before Store records delivery. No retry overwrites a different object.

Compliance mode adds retention headers and verifies the returned mode, retention
date and non-null version ID on GET. Its date is the block's sealing time plus the
configured days, making retries stable. An already expired export window requires
a longer configured period. A provider that ignores Object Lock cannot satisfy
this profile. The bundled RustFS destination uses `disabled`; it establishes no
write-once storage guarantee.

OTLP sends one request per block with events named `veoveo.audit.<class>`. Its body
contains the OCSF event, and attributes preserve record, partition and request IDs.
The exporter sends HTTP directly because a Store receipt must reflect collector
acknowledgement. HTTP 200 with zero rejected logs acknowledges delivery. Partial
rejection is persisted and stops automatic replay of that intent, including on
another replica. The installation must inspect the rejected destination and select
a corrected destination configuration; changing credentials alone does not erase
a recorded rejection. Partial acceptance never permits retention.

Network failures and uncertain acknowledgements keep their intent for retry after
five seconds. OTLP honors `Retry-After` on retryable statuses. Delivery is at least
once: a crash after collector acceptance but before Store acknowledgement can repeat
records. Consumers deduplicate by record ID. Rejection persistence also has this
crash window; an uncommitted response cannot establish what another replica observed.

Each HTTP operation has a five-second connection deadline and a fifteen-second total
deadline. A block network attempt has sixty seconds. A known rejection stays in memory while
its Store write retries; a network deadline cannot turn that response into a resend. Content is limited to 32 MiB, the signed
head to 2 MiB, and collector responses to 64 KiB. Provider bodies and error text never
enter logs. Shutdown cancels export after stopping producers and sealing committed
records. Unacknowledged intents resume on the elected replica; shutdown does not
wait for an unavailable archive to recover.

## OCSF Mapping

`cargo test -p veoveo-audit --test ocsf -- --ignored --nocapture` submits synthetic
fixtures for every mapped class and outcome to the upstream
[OCSF 1.9.0 validation API](https://schema.ocsf.io/1.9.0/doc/swagger.json).
The harness checks the reported schema version, limits each request to 15
seconds and the run to 180 seconds, and prints structured validation results.
It reads no installation records. This network qualification complements the local
mapping tests; default test runs do not contact the public validator.

Every event preserves the complete typed source as `unmapped.veoveo`, including
attribution, reason and timestamps. This supplies the record bytes for verification
against the signed block. Admission maps to status Other with label `Allowed`, since
an admission cannot establish operation completion.

API Activity and File Hosting require an actor and source endpoint. They are emitted
only when the source record supplies both. Authentication requires an identified
subject and names the gateway service. Account Change uses the target principal;
Work Context changes use API Activity. When required facts are absent, Base Event
preserves the source without inventing an actor, username or address. Domain actions
without a matching OCSF activity use Other and their closed Veoveo activity name.

Account Change 3001 is supported but deprecated by OCSF 1.9.0. This mapping keeps the
foundation plan's selected class for principal changes; adopting User Management
requires a new export mapping version. It does not affect the stored audit model.

## Qualification

`cargo test --locked --workspace --all-features --test s3_export -- --ignored --nocapture`
qualifies actual S3 delivery with the hosted writer and sealer against a disposable
RocksDB Store. `VEOVEO_AUDIT_TEST_S3_CONFIG` names a public export configuration with
an S3 destination, an existing bucket and Object Lock disabled. The normal S3
credential chain supplies credentials. Docker must have the pinned SurrealDB image.
The fixture adds a unique prefix, compares downloaded source records and signed blocks,
checks receipts from a second Store connection, and restarts the audit worker. It
allows 180 seconds for acceptance and 30 seconds for prefix cleanup, including after
a failed assertion. The configured S3 provider is externally managed. This test can
use the bundled RustFS endpoint; it does not qualify the gateway deployment or Object
Lock. Default test runs leave this provider test ignored.

The fixture also accepts an OTLP destination in that configuration. It waits for both
destinations' receipts before restarting the worker and checks both delivery cursors
after restart. The collector operator must verify that its logs pipeline received the
two synthetic records; an acknowledgement alone proves receiver acceptance.

The foundations batch owns compilation, native protocol tests, independent OCSF
schema validation, Store failure cases and installed delivery. The source fixtures
cover uncertain S3 PUTs, mismatched bytes, missing Object Lock proof, destination
changes, immutable intents, partial rejection and lease-fenced retention. The native
HTTP protocol fixtures pass. Store and installed qualification are tracked in the
[consolidated plan](../../../../docs/CONTRACT_CONSISTENCY_PLAN.md#unified-audit-log).
The reference configuration selects bundled S3 export.
Compliance-mode provider acceptance is pending under regulated-readiness gap G9.
