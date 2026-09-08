# Public Artifact Upload Plan

Status: proposed for review; implementation has not started. Baseline: main
`1ea74896`, fetched on
2026-09-08. Existing component designs remain normative until implementation lands.

An external application uploads a file through authenticated HTTP and receives a
canonical Artifact URI. A Rust or Python MCP server then consumes that URI under the
calling principal's existing access rules. File bytes stay outside MCP messages.

## Standards And Protocols

| Boundary | Proposed supported profile |
|---|---|
| HTTP, RFC 9110 | JSON admission, streamed raw-body PUT, authenticated receipt GET, cancellation DELETE, explicit status codes, and bounded request lifetimes |
| Existing gateway OAuth profile | Registered human and machine clients, bearer tokens, exact profile protected-resource binding, and existing `private_key_jwt` client credentials; reuse the repository's supported profile |
| JSON Schema 2020-12 | Closed upload request, session, receipt, error, and installation-policy models generated from Rust types |
| SHA-256 | Required client-declared digest, independently verified by Artifact service during transfer |
| UUIDv7 | Upload and occurrence identities; repository-owned UUIDv7 `Idempotency-Key` semantics, not a claim of implementing an external idempotency standard |
| Veoveo Work Context authority | Gateway-resolved tenant, actor, invocation provenance, output owner, initial grants, classification, labels, and policy revision |
| Internal Artifact HTTP | Existing signed-identity transport and verified streaming storage, extended with durable upload operations; internal headers remain an adapter detail |
| MCP `2026-07-28` | Existing downstream tool input and artifact-resource projection only; upload is an independent HTTP contract |

No new dependency is proposed. Implementation must verify authoritative upstream
releases and exact pins if it introduces or touches a dependency.

## Current Implementation

| Existing code | Reusable behavior or gap |
|---|---|
| `platform/artifacts/service/src/http.rs` | Internal `POST /artifacts` and `POST /artifacts/stream`; current upload ceiling is 256 MiB |
| `platform/artifacts/service/src/service.rs` | `put_stream` stamps trusted authority and recognizes committed retries using occurrence identity and immutable publication data |
| `platform/artifacts/service/src/store.rs` | `put_verified_stream` hashes bounded chunks, verifies exact length, and aborts explicit transfer/verification failures before writer completion |
| `platform/gateway/src/bin/gateway/recording_layer_publication.rs` | Public authenticated streaming proxy for recording layers, including policy and gateway assertion issuance |
| `platform/gateway/src/bin/gateway/auth.rs` and `runtime.rs` | Profile bearer authentication and `/artifacts/{profile}/...` profile extraction already exist |
| `sdk/python/src/veoveo_mcp/artifacts.py` | `ArtifactRepository.resolve` reads a URI with the forwarded caller identity |
| `templates/python-mcp/src/datasheet_mcp/server/mcp_server.py` | CSV/Parquet artifact input is implemented |

There is no general public gateway or Console upload route at this baseline. The
streaming implementation does not supply a durable public admission lifecycle,
in-flight reservation, or public receipt API. Cancellation, process death, and an
object-store success followed by database failure require explicit recovery design.

## First Release

The proposed first release assumes one nonempty file per upload, up to the current
256 MiB ceiling, pending confirmation of the required dataset sizes. Installations
may set a lower limit. Interrupted transfers restart
from byte zero under the same upload identity. Range-based resumption and multipart
public uploads are separate work if multi-GB input is required from day one.

Supported initial callers are external services and command-line clients with a
gateway access token. The endpoint does not require a particular downstream MCP
server. Uploading a CSV stores an immutable artifact; table ingestion or dataset
profiling remains an explicit domain operation.

Console and MCP App file pickers follow the endpoint milestone. They consume the
same admission and storage primitives. App-mediated authority remains subject to
the exact App resolution gate in `PLATFORM_IMPROVEMENTS_PLAN.md`, request `017`.
That gate does not block direct authenticated API clients.

## Public API

All paths are relative to the installation origin. Every operation authenticates a
fresh request against the selected gateway profile. An upload ID is not a credential.

| Method and path | Request | Result |
|---|---|---|
| `POST /artifacts/{profile}/uploads` | JSON descriptor and UUIDv7 `Idempotency-Key` | `201` reserved session; matching replay returns `200` with the same session or completed receipt |
| `PUT /artifacts/{profile}/uploads/{upload_id}/content` | Raw file body with matching `Content-Type` | `201` completed receipt; a completed matching replay returns `200` |
| `GET /artifacts/{profile}/uploads/{upload_id}` | No body | Current session state and completed receipt when present |
| `DELETE /artifacts/{profile}/uploads/{upload_id}` | No body | `204` after cancellation is recorded; cannot delete a completed artifact |

The JSON descriptor contains `filename`, `mime_type`, `byte_len`, and `sha256`.
Reject unknown fields. Bound filename length and reject path components and control
characters. Apply an installation-owned media-type allowlist. Content-Type agreement
checks transport consistency; the platform does not claim that a filename or MIME
declaration validates the file's domain format. Domain consumers validate their input.

Keep tenant, owner, grants, Work Context, provenance, storage keys, and release state
out of this descriptor. Derive governance from authenticated authority and an
installation-owned upload policy. A client that needs a different Work Context uses
the existing registered-client authority mechanism.

Admission allocates a server-owned upload ID and occurrence ID. It returns an
`upload_url` under the same origin, `expires_at`, and a typed session state. The
proposed reservation lifetime is one hour. Return the canonical artifact URI only
after durable completion. A receipt contains:

```json
{
  "upload_id": "<uuidv7>",
  "artifact_id": "<uuidv7>",
  "artifact_uri": "artifact://<uuidv7>",
  "sha256": "<verified-lowercase-hex-digest>",
  "byte_len": 12345,
  "mime_type": "text/csv",
  "filename": "dataset.csv",
  "created_at": "<UTC timestamp>"
}
```

The placeholders above describe the wire shape. A CLI flow will be:

```sh
# upload.json contains the filename, MIME type, exact size, and computed digest.
curl --fail-with-body "$VEOVEO_ORIGIN/artifacts/$PROFILE/uploads" \
  -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H "Idempotency-Key: $REQUEST_ID" \
  -H 'Content-Type: application/json' \
  --data-binary @upload.json

# UPLOAD_ID comes from the admission response.
curl --fail-with-body \
  "$VEOVEO_ORIGIN/artifacts/$PROFILE/uploads/$UPLOAD_ID/content" \
  -X PUT -H "Authorization: Bearer $ACCESS_TOKEN" \
  -H 'Content-Type: text/csv' --data-binary @dataset.csv
```

The client computes the digest through bounded file reads. The server verifies it
again while streaming. This entails a preliminary read by the client; it never
requires loading the entire file into memory. For HTTP/2 or clients without a
Content-Length header, the admitted `byte_len` remains the exact streaming bound.
When Content-Length is present, reject a mismatch before consuming the body.

## Authorization And Admission

Add `GatewayAction::ArtifactUpload` with serialized name `artifact_upload`, targeted
at the Artifact server. Require the new `artifact:upload` scope plus an explicit
profile policy rule. The action covers admission, transfer, receipt recovery, and
cancellation, each scoped to the session's original actor. It maps to no MCP method.

Require at least Contributor membership in the resolved Work Context. Verify profile
exposure, tenant, and label clearance before reservation. Repeat current authorization
on each operation. Artifact service rechecks the signed authority and the current
Work Context policy before committing. A changed policy invalidates an unfinished
reservation; the client must request a new one. A replay never bypasses current access
checks. Foreign sessions return a non-disclosing `404`.

The new typed upload policy bounds file bytes, admitted media types, active sessions,
and reserved bytes per tenant and actor. Reserve count and byte quota atomically at
admission. Concurrent gateway replicas must share the same accounting. The declared
size controls reservation and the observed byte count controls transfer enforcement.
Validate deployment values against the Artifact service ceiling.

Only the gateway issues internal assertions. It replaces incoming internal headers
with a descriptor derived from the durable session and trusted admission. Public
clients never obtain object-store keys, internal bearers, or task write capabilities.
Existing task write capabilities remain specific to asynchronous task output.

Reuse the selected profile's existing OAuth protected resource and discovery. Do not
invent an upload-only audience or accept a token issued for another profile. Machine
clients use the existing client-credentials flow. Later browser callers use the
Console BFF session and CSRF protection.

## Streaming, Durability, And Failure Handling

Artifact service owns upload records in the shared Store. Use a focused upload module
with typed states `reserved`, `receiving`, `completed`, `cancelled`, and `expired`.
A receiving record carries an expiring lease and fencing generation. A live competing
transfer gets `409`; a stale worker cannot complete after cancellation or takeover.
An interrupted attempt returns to `reserved` after cleanup and lease recovery.

Scope the admission idempotency key to tenant, actor, profile, and Work Context. Bind
the immutable descriptor and upload-policy revision to that record. Repeating the key
with different input returns `409`. The completed receipt survives service recreation
and a lost HTTP response. Refreshing a token for the same authority does not create a
second occurrence. Session cleanup preserves completed deduplication records for as
long as their artifact occurrence is retained; do not silently reuse expired keys.

Stream through Gateway into Artifact service using backpressure. Share the verified
writer with recording publication, while keeping recording-specific authorization in
its existing route. No hop may collect the file with `Bytes`, `to_bytes`, or a cloned
whole-body buffer. Apply a byte counter, idle timeout, total transfer deadline, and
cancellation propagation at each hop. Bound transfer admission by token expiry and
return an actionable expired-authorization error when a fresh request is required.

Extend storage around attempt-specific temporary objects or multipart handles with
durable cleanup ownership. After exact length and digest verification, promote the
blob to tenant-scoped content storage. Commit the occurrence, initial grants, receipt,
quota settlement, and completion audit/outbox event atomically in Store. Fence the
completion against the current session generation and current policy.

Object storage and Store do not share a transaction. Recovery must reconcile a crash
after blob completion and before occurrence commit. Record enough state to finish a
valid commit or remove an unreferenced temporary object. Never delete a content blob
that another occurrence references. Cancellation is durable before returning `204`;
cleanup converges after crashes through a service-owned recovery loop. No incomplete
upload may expose an artifact URI or become readable. Explicit abort tests alone do
not establish this guarantee.

Use bounded typed errors with a safe correlation ID: malformed descriptors `400`,
authentication failures `401`, policy denials `403`, immutable conflicts `409`, expired
sessions `410`, byte ceilings `413`, disallowed media types `415`, integrity failures
`422`, and quota saturation `429`. An upstream outage is retryable `503`. A completed
upload cannot be cancelled and returns `409` to DELETE. Response bodies and telemetry
must omit file bytes, credentials, and internal storage paths.

## Implementation Sequence

| Checkpoint | Changes | Completion evidence |
|---|---|---|
| 1. Typed contract and policy | Add `mcp/contract/src/artifact_service/upload.rs`, public wire models, gateway action/validation, upload policy, schemas, and component design updates | Closed-schema tests, policy admission/denial tests, deployment configuration validation |
| 2. Durable Artifact ingestion | Add focused service/HTTP/ledger upload modules, `platform/store/src/artifact_uploads.rs`, the next ordered migration, streaming client methods, leased reservations, atomic completion, and cleanup | Native SurrealDB concurrency/restart tests and actual S3-compatible storage interruption/cleanup tests |
| 3. Public gateway endpoint | Add `platform/gateway/src/bin/gateway/artifact_upload.rs`; wire routes in `server.rs`; compose existing auth, policy, audit, and streaming helpers | HTTP admission/transfer/receipt/cancel tests with real streamed bodies and bounded buffering |
| 4. External-to-Python acceptance | Add a Rust smoke scenario under `testing/smoke/src/bin/smoke/scenarios/`; upload through the public gateway, pass the URI to the Python datasheet server, verify output and denied access | Gateway-authenticated CSV and Parquet round trips, exact digest, single occurrence after retry, and cross-tenant denial |
| 5. Installation and client guide | Update chart values/schema, installation-owned policy examples, external integration docs, Python template guide, and CODEMAP; verify ingress size/timeouts and request buffering | Recorded affected checks and a deployment plan naming only changed components |

The SDK's current `resolve` path already supports first-release consumption. Do not
add upload tools to each MCP server. Consumer limits remain independent: Python
`resolve` currently materializes bytes in memory, and datasheet has its own dataset
limit. Passing upload acceptance does not prove that every server can process a file
at the platform maximum. Use small known fixtures for the Python round trip and test
the upload ceiling separately.

Update `platform/artifacts/service/DESIGN.md` with upload state and cleanup semantics.
Keep gateway-level authority and public endpoint documentation in the repository-wide
technical design, with contract types beside `mcp/contract`. This plan remains indexed
as proposed work until those implementation checkpoints land.

## Verification And Rollout

Test unauthorized and viewer-only admission, incorrect audience/profile, forged
ownership fields, label denials, media/size rejection before transfer, truncated and
oversized bodies, digest mismatch, cancellation, expired leases, policy changes,
concurrent identical and conflicting retries, quota release, lost responses, restart
recovery, and database failure after blob completion. Measure bounded memory with a
slow consumer. Exercise the configured ingress path as well as loopback HTTP.

Run affected checks through `cargo xtask test-report run --name <check> -- <command>`,
then inspect `cargo xtask test-report show` and commit passing evidence with each
build-input checkpoint. Start with the affected contract, Artifact, Store, and gateway
crate tests. Use `cargo xtask enforce python` if Python build inputs change. All smoke
lifecycle, assertions, retries, and cleanup belong in the Rust smoke harness.

The plan itself changes documentation only and needs no build evidence refresh.
Implementation rollout uses the latest component-scoped deploy workflow: bootstrap
the Store migration, activate Artifact service, then Gateway and explicit upload
policy. Keep the route disabled by policy until both service and gateway contracts
are present. Upload API acceptance is nonvisual. A later Console/App file-picker
milestone requires separate headed hardware-backed browser acceptance under the
repository GPU policy.

## Console And MCP App Follow-On

Add a same-origin Console BFF upload module that streams to these routes with session
authentication and CSRF checks. A Console file picker receives progress and completion
receipts. MCP Apps request the host picker only under an exact resolved App upload
grant and browser-observed user activation. The host intersects that grant with the
installation upload policy before admission. File bytes remain in the host origin;
the App receives only the receipt. Include exact App URI and grant revision in the
trusted reservation authority and idempotency identity for that path.

This follow-on implements the host-mediated portion of request `017` using the public
ingestion foundation. It must not allow a generic upload scope to bypass a missing
App grant. Reconcile the current platform plan when that phase starts rather than
creating a second App upload design.
