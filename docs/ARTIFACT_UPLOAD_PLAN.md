# Public And Console Artifact Upload Plan

Status: implementation active. Shared HTTP types, checked layout policy, immutable blob
registration, part accounting, fenced finalization, and atomic receipt publication pass
native checks. Integrated S3 transfer/recovery, public routes, Console, and deployed
acceptance remain in progress. Baseline:
main `fe6ad0fb`, fetched and checked on 2026-09-08. The first release includes multi-GB resumable
uploads and an upload form in the Console Artifacts page. Existing component designs
remain normative until implementation lands.

An external application or Console user uploads a file through authenticated HTTP
and receives a canonical Artifact URI. Rust and Python MCP servers consume that URI
under the caller's access rules. File bytes stay outside MCP messages.

## Standards And Protocols

| Boundary | Proposed supported profile |
|---|---|
| HTTP, RFC 9110 | JSON admission/completion, streamed raw-body part PUT, authenticated status GET, cancellation DELETE, and explicit status codes |
| Existing gateway OAuth profile | Registered human/machine clients, bearer tokens, exact profile protected-resource binding, and existing `private_key_jwt` client credentials |
| JSON Schema 2020-12 | Closed request, policy, session, part, receipt, error, and browser-projection models generated from Rust types |
| SHA-256 | Verified part digests and a separately computed whole-object digest; multipart ETags and composite checksums are not whole-file SHA-256 |
| UUIDv7 | Upload, occurrence, and client request identities; repository-owned UUIDv7 `Idempotency-Key` semantics |
| Veoveo Work Context authority | Gateway-resolved tenant, actor, provenance, owner, initial grants, classification, labels, and policy revision |
| Veoveo resumable upload API | Repository-owned part/completion protocol below; no claim of tus or browser multipart/form-data conformance |
| S3-compatible multipart storage | Private create, part upload, complete, and abort adapter; provider IDs and object keys remain internal |
| Browser File/Blob, workers, and HTTP upload | Bounded file slices, incremental hashing, progress, and cancellation in the Console origin |
| Console authentication | Existing BFF session, same-origin requests, and CSRF protection on mutations |
| MCP `2026-07-28` | Downstream tool inputs and artifact resources; upload is an independent HTTP contract |

The current storage dependency is `object_store` 0.14.1. Its low-level
[`MultipartStore`](https://docs.rs/object_store/0.14.1/object_store/multipart/trait.MultipartStore.html)
exposes provider upload IDs and part receipts suitable for durable sessions. It
accepts a materialized `PutPayload` per part, which bounds memory by part size but
does not provide a streaming request-body API. Preserve that distinction in performance
claims. Verify authoritative upstream releases and exact pins when a dependency is
introduced or touched. Select a maintained incremental browser hash implementation
at implementation time if existing dependencies do not provide one.

## Current Implementation And Required Changes

| Existing code | Reusable behavior or required change |
|---|---|
| `platform/artifacts/service/src/http.rs` | Internal puts have a 256 MiB ceiling; replace the fixed total-file ceiling with upload policy and independent per-request bounds |
| `platform/artifacts/service/src/service.rs` | Trusted ownership and committed occurrence retry checks exist; add durable multipart sessions, reservations, finalization, and recovery |
| `platform/artifacts/service/src/store.rs` | Streaming reads and multipart-backed writes exist; expose durable multipart operations through focused modules |
| `platform/store/src/artifacts.rs` | Blob identity is tenant plus whole-file SHA-256; replace mutable blob UPSERT behavior with conflict-safe immutable reuse when upload object keys differ |
| `platform/gateway/src/bin/gateway/recording_layer_publication.rs` | Reuse authenticated streaming proxy mechanics while retaining recording-specific authorization in its route |
| `platform/gateway/src/bin/gateway/auth.rs` and `runtime.rs` | Profile bearer authentication and `/artifacts/{profile}/...` profile extraction already exist |
| `apps/console/web/src/views/Artifacts.tsx` | Existing searchable artifact table; add the upload action and form here |
| `platform/gateway/src/bin/gateway/admin/console/{mod,stream}.rs` | Snapshot and live projection substitute zero when a blob length is absent; resolve referenced blob metadata so completed uploads retain exact displayed sizes |
| `apps/console/bff/src/api.rs` | Session/CSRF and streamed download patterns exist; add upload behavior in a separate module |
| `sdk/python/src/veoveo_mcp/artifacts.py` | URI resolution exists but materializes the object; add streaming download/file materialization for large consumers |

There is no general public or Console upload route at this baseline. In-memory
multipart writers cannot resume across processes. Explicit writer aborts do not
establish recovery after process death or storage success followed by database failure.

## First-Release Scope And Size Policy

Multi-GB files and the Console Artifacts upload form are required deliverables.
There is no fixed small product-level file limit. Installations declare supported
object size, tenant storage quota, upload concurrency, and transfer budgets within
their actual backend limits. Use checked 64-bit byte counters throughout Rust and
Store, and reject unsafe numeric conversions at the browser boundary.
Upload enablement requires an explicit installation policy; omitted limits must not
silently restore a small default cap or grant unlimited storage.

Admission negotiates part size and parallelism. Start performance measurement at
16 MiB parts with up to four transfers per upload, then tune against the installed
store and shared memory budget. These are transfer units, not file-size caps.
Increase part size as needed for the declared object size and backend maximum part
count. The admitted layout stays fixed. Enforce aggregate memory admission and tenant
fairness across all concurrent sessions.

A known-size file reserves its total bytes before transfer. A streaming producer may
omit total length and reserve a bounded window of additional parts under tenant quota.
Charge new parts before accepting bytes and release unused reservation at completion.
Admission declares the session's maximum total bytes and part count. Unknown-length
streams cannot silently exceed those bounds; callers expecting larger streams request
an appropriate layout at admission. Only the final part may be shorter than part size.

Interrupted transfers retry missing parts. A refreshed token resumes the same upload.
Gateway or Artifact service restarts never require retransmission of the entire file.
Console, command-line clients, and services share the same ingestion foundation.
Importing a stored artifact into a domain database remains an explicit MCP operation.

## Public HTTP API

All paths are relative to the installation origin. Every request authenticates
against the selected profile. An upload ID is an identity, not a credential.

| Method and path | Purpose |
|---|---|
| `GET /artifacts/{profile}/upload-policy` | Effective permission, admitted types, object/quota limits, and transfer constraints |
| `POST /artifacts/{profile}/uploads` | JSON descriptor and UUIDv7 `Idempotency-Key`; `201` session with negotiated layout/expiry; matching replay returns `200` |
| `PUT /artifacts/{profile}/uploads/{upload_id}/parts/{part_number}` | Raw streamed part with declared size and SHA-256; durable part receipt after storage acknowledgement and ledger commit |
| `GET /artifacts/{profile}/uploads/{upload_id}` | State, progress, completed receipt, and a bounded cursor-paginated page of accepted parts |
| `POST /artifacts/{profile}/uploads/{upload_id}/complete` | Final byte length, part count, and optional expected whole-file SHA-256; seal manifest and return `202` while verifying, or `200` for an existing receipt |
| `DELETE /artifacts/{profile}/uploads/{upload_id}` | Record cancellation and return `204`; cannot delete a completed artifact |

Public part numbers are one-based. Offsets follow from the negotiated layout. Use
typed repository-owned `x-veoveo-part-byte-len` and `x-veoveo-part-sha256` headers.
Enforce size with a streaming counter. Content-Length must match when present;
the protocol also accepts streams where that header is absent.

Admission accepts `filename`, `mime_type`, optional `byte_len`, and optional expected
whole-file `sha256`. Reject unknown fields and unsafe filenames. Apply installation
media policy; a MIME declaration does not certify domain-format validity. Tenant,
Work Context, owner, grants, provenance, release state, and storage paths come from
trusted authority rather than form fields or caller JSON.

Clients compute each part digest in bounded memory before transferring that part.
Console hashes slices in a worker and pipelines hashing with transfers. A streaming
producer buffers at most its admitted part window. There is no mandatory preliminary
whole-file read. The service independently checks each part before accepting its
storage receipt. Bind each part number to immutable size and digest. Matching retries
reuse the receipt; different content conflicts.

Completion rejects holes, wrong totals, conflicting manifests, and nonfinal short
parts. Freeze accepted part identities before storage completion. Return the artifact
URI only after verification and durable publication. The final receipt contains upload
ID, artifact ID, canonical `artifact://{uuidv7}` URI, verified whole-file SHA-256,
exact byte length, MIME type, filename, and creation time.

The client flow is admission, bounded parallel part PUTs, completion, and receipt.
Status recovery reads this platform's upload ledger. Storage operations are direct
HTTP requests; this design introduces no external provider-job polling.

For example, admission of a 10 GiB file uses this JSON body; clients need not calculate
its whole-file digest before starting:

```json
{
  "filename": "observations.parquet",
  "mime_type": "application/vnd.apache.parquet",
  "byte_len": 10737418240
}
```

With a negotiated 16 MiB layout, the client sends 640 parts. A retry sends only parts
missing from the accepted-part ledger. The completion body is
`{"byte_len":10737418240,"part_count":640}`. MIME admission still depends on the
installation policy. Small files use one part through the same canonical API.

## Console Artifacts Upload Experience

The user selects files, sees where they will be available, and starts the upload.
They can continue working elsewhere in Console while the transfer runs. Each file
becomes an artifact only after the service finishes verification and publishes its
receipt. Uploading does not release the artifact or import it into a domain database.

### Entry, Selection, And Destination

Place **Upload files** beside the Artifacts search and release filter. Use the same
action in the empty-library state. The filtered-empty state offers **Clear filters**;
it must not imply that an existing library is empty. Artifact filters offer artifact
release states, while transfer states belong to the upload queue. The current shared
Toolbar mixes task and release states; give this page an explicit artifact filter.

Open a side panel using Console components and theme tokens. Its first line names
the authenticated destination, such as **Upload to Operations**, followed by a short
effective-access description from policy. Keep ownership, release, and classification
authority server-derived. Do not ask users for MIME strings, hashes, Artifact IDs,
storage paths, or grants to perform an ordinary upload.

Fetch effective upload policy through the BFF before enabling transfer. Show a clear
permission or policy explanation when upload is unavailable, and retry policy discovery
after a transient failure. Client validation helps selection; server admission remains
authoritative when quota or policy changes before transfer starts.

File picker and drag/drop selection use one validation path. A drop selects files
for review and does not start a transfer. Show each filename, size, and recognized
type, plus total selected bytes and available quota when policy provides it. Explain
type, size, or permission rejection beside the affected file. Valid files can proceed
when another selection is invalid; label the action with the valid file count.
Users can remove a selection before admission without creating a server session.

Same-named files never imply replacement. A deliberate new upload creates a new
artifact occurrence. Suppress accidental duplicate selections within the current
queue, explain the existing queue entry, and let the user choose whether to add
another copy. Network retries retain the original admission identity.

### Persistent Queue And Progress

Mount the typed transfer controller in the authenticated Console shell. The side
panel is a view of that controller; unmounting Artifacts or closing the panel must
not cancel or pause it. A compact **Uploads** control stays available across Console
routes and reports counts needing attention, transferring, verifying, and ready.
Queue rows retain selection order while files finish independently.

| User-visible state | Meaning and available action |
|---|---|
| Ready to upload | Valid selection; **Upload files** starts admission |
| Queued | Admitted and waiting for a shared transfer slot; **Pause** or **Cancel** |
| Preparing | Hashing bounded slices; show activity without a misleading whole-file scan requirement |
| Uploading | Show bytes sent out of total, transfer percentage, and **Pause** or **Cancel** |
| Paused | Local transfer stopped and accepted parts retained; **Resume** or **Cancel** |
| Waiting for connection | Explain automatic bounded retry and retained progress; **Retry now** or **Pause** |
| Sign in to continue | Retain the session identity, require the same actor and destination, then recheck authority |
| Select file to resume | Local file access was lost; **Choose file** checks the file before any missing parts are sent |
| Checking selected file | Verify size and accepted-part digests; explain that saved upload progress is being checked |
| Verifying | All bytes sent; service is checking and saving the file. Show **Finishing upload**, with no invented percentage or ETA |
| Ready | Durable receipt received; **View artifact** and secondary **Copy artifact URI** |
| Needs attention | Name the failure and its recovery action; preserve other files' progress |
| Cancelled | Cancellation acknowledged; this entry cannot resume |

Keep bytes sent and durably accepted bytes separate in controller state. Ordinary
progress copy shows transfer progress, and paused/recovery copy states how much is
saved. A transfer reaching 100% moves to **Finishing upload**; it never means the
artifact is already available. Display rate and remaining time only after a stable
sample, label the estimate as transfer time, and hide it during stalls or verification.
Batch progress distinguishes **3 of 5 files ready** from bytes transferred.

Pause stops scheduling and aborts in-flight requests while retaining durable parts.
Resume reconciles accepted parts before sending missing ones. **Cancel upload** calls
DELETE and becomes terminal only after the service acknowledges it. While cancellation
is pending, show that state explicitly. If completion wins the race, recover the
receipt and show **Ready**; cancelling an upload cannot delete a completed artifact.
Clearing a completed queue row only dismisses it from the queue.

### Navigation, Identity, And Recovery

Closing the panel or changing Console routes keeps transfers running. Explain once
that the user can continue using Console but must keep its tab open for file transfer.
Reloading or closing the browser may interrupt local work; never promise background
transfer after the page exits. Server verification continues independently and settles
through upload events in the existing Console event stream. Reconnect and reload
reconcile authoritative status, including receipts completed while the browser was away.

Bind each queue entry to its admitted actor and Work Context. Signing out or switching
Work Context pauses local scheduling and clears accessible file handles from the old
session. Returning to that actor and context offers recovery. An old upload must never
be redirected into the newly selected context or shown to another signed-in actor.

Persist only upload IDs and small descriptors locally, scoped to installation origin,
actor, and Work Context. Never persist bearer tokens or whole files there. After reload,
reconcile through the authenticated API and request file reselection only for unfinished
transfers that have lost their handle. Completed and verifying sessions need no file.
Verify size and recompute accepted-part digests before resuming to prevent mixing two
same-named files. A mismatch keeps the original progress and offers **Choose another
file**; it must not silently replace the session or erase accepted parts.

Use recovery copy that tells the user what happens next. Quota failures show the
required and available space when authorized. Expired sessions offer **Start again**
and explain that another transfer is required. Integrity failures identify the affected
file without exposing internal paths. Non-retryable authority or policy changes stop
that file and require a fresh permitted admission; automatic retry cannot restore access.
One file's failure must not discard or restart the rest of the queue.

### Completion And Accessibility

Patch the artifact list by the receipt's artifact identity and reconcile its canonical
metadata. The row and ArtifactDrawer must show the exact verified byte length; missing
metadata must not masquerade as a zero-byte file. The baseline live page displays
`0 B` for many populated artifacts. On September 8, the snapshot reports zero for
`artifact_delivery_acceptance_export.csv`, while authenticated download HEAD returns
`Content-Length: 14288899`. Both gateway snapshot and live projection use a zero
fallback for absent blob lengths. Resolve the size projection when wiring the upload
receipt into the existing list, including after a fresh snapshot replaces local state.

Provide **View artifact** on every completed row. It opens the existing ArtifactDrawer
on demand; background completion never steals focus or opens several drawers. If the
current search or release filter hides the new artifact, keep the completion row and
its direct action visible. Do not reset the user's filters silently. URI copying is a
secondary handoff for tools; domain import and App-specific actions retain their own
authorization and workflows.

Use a native labelled file input alongside the drop area. Every row action includes
the filename in its accessible name. Announce phase changes and final results without
announcing every progress tick. Restore focus to the opening control when the panel
closes, preserve keyboard access to the persistent queue, and support a narrow viewport
without hiding recovery actions. If the panel is modal, contain focus and let Escape
close it while the queue continues. Status must remain understandable without color.

Add `apps/console/bff/src/artifact_upload.rs`. Same-origin routes mirror public operations
under `/console/api/artifacts/`, including upload-policy, uploads, parts, status,
completion, and cancellation. Derive profile/bearer from the session and rewrite URLs
to the BFF origin. Require existing CSRF protection on every mutation. Stream part
bodies through BFF and Gateway without collecting or duplicating them.

Use `components/ArtifactUploadForm.tsx`, a persistent queue view, an authenticated-shell
provider with a typed transfer controller, and a hashing worker. Keep queue logic out
of `Artifacts.tsx` and shared `api.ts`. Use native bounded
File/Blob uploads with observable progress. Share negotiated concurrency across the
queue. A successful part PUT is not whole-file success; wait for the final receipt.

## Authorization, Quotas, And Expiry

Add `GatewayAction::ArtifactUpload` (`artifact_upload`) targeting Artifact. Require
`artifact:upload`, explicit profile policy, and Contributor-or-higher Work Context
membership. It maps to no MCP method. Authenticated policy discovery must report
unavailable permission without granting transfer authority.

Recheck authorization on each operation and before occurrence commit. Bind sessions
to tenant, actor, profile, Work Context, provenance, and upload-policy revision.
Changed governing policy invalidates unfinished admission. Foreign sessions return
a non-disclosing `404`. Only Gateway signs internal assertions and reconstructs trusted
descriptors. Object storage remains private; public clients receive no internal bearer.

Scope admission idempotency to actor, tenant, profile, and Work Context. Conflicting
descriptors return `409`; receipt recovery still requires current access. Reserve
bytes/session counts atomically across replicas. Cleanup accounting includes retained
physical bytes even after cancellation releases the logical reservation.

Use configurable inactivity expiry and maximum session lifetime. Start with 24 hours
of inactivity and seven days total, with installation overrides. Short per-part
deadlines and token expiry bound each HTTP request; refreshed authentication continues
remaining parts. A single request timeout is never the whole-file deadline.

## Storage, Integrity, And Performance

Parts flow through Console BFF when applicable, Gateway, and Artifact service into
native S3 multipart storage. External clients bypass only the BFF. Persist provider
upload IDs and part receipts in Store; these are private implementation details.

`MultipartStore::put_part` materializes one bounded part. Reuse shared byte chunks
where possible and acquire memory budget before reading. Gateway/BFF stay streaming.
Memory scales with admitted in-flight parts, not object size. Measure copies and
throughput before choosing a more complex streaming S3 adapter. If bounded-part
materialization is the bottleneck, adopt a maintained streaming adapter with exact
current pins. Do not hand-roll signing or claim zero-copy from the current API.

Complete multipart storage at an opaque tenant-scoped upload object key. Compute
whole-file SHA-256 with one bounded sequential read in the installation network and
compare the expected digest when supplied. Retain the verified object at that key
and attach it to the content ledger. Do not materialize the file on local disk or
rewrite it under a hash-derived key.

This verification pass is an explicit internal read cost. It preserves whole-file
identity while allowing parallel uploads without a preliminary client file scan.
Part hashes cannot be combined into ordinary whole-file SHA-256. S3's SHA-256
multipart checksum is composite, as documented in the
[S3 integrity profile](https://docs.aws.amazon.com/AmazonS3/latest/userguide/checking-object-integrity-upload.html).
Do not store a composite checksum or ETag in Veoveo's whole-file `sha256` field.

Verification is durable service-owned work with a lease. It survives client disconnect
and can restart an internal read after a crash without client retransmission. Any later
hash-checkpoint optimization needs versioned state and corruption tests. Benchmarks
include verification in time-to-usable-artifact; wire throughput alone is insufficient.

Register blobs through immutable create-or-reuse by tenant/digest. Concurrent equal
uploads converge on one retained object key without overwriting a referenced mapping.
Attach the occurrence to the winning blob, then remove the unreferenced duplicate.
Update all shared writers, including recording publication, to honor this invariant.
This is a required cross-component contract change.

## Durable State And Recovery

Use typed states `open`, `finalizing`, `verifying`, `completed`, `cancelled`, `expired`,
and `failed`. Persist per-part size/digest, storage receipt, lease, and attempt generation.
Fence stale attempts/finalizers. Only an accepted immutable part descriptor can reach
storage. Reconcile uncertain acknowledgements without accepting different content.

Freeze the manifest transactionally before storage completion. Recover from completion
whose response or database update was lost. After verification/current-policy checks,
commit occurrence, grants, receipt, quota settlement, and completion audit/outbox in
one Store transaction. Exactly one occurrence becomes visible per upload identity.

Storage and Store have separate transactions. Track multipart handles and unpublished
completed objects for service-owned cleanup. Abort unfinished uploads after expiry or
cancellation. Delete unreferenced failed/abandoned objects. Backend lifecycle cleanup
is defense in depth and cannot delete committed objects because their key originated
from an upload. Test cancellation/finalization and cleanup/shared-blob races.

Cancellation is terminal before `204`; physical cleanup may finish asynchronously.
Repeated cancellation is idempotent. DELETE of completed uploads returns `409`.
Keep completed idempotency records for the artifact retention lifetime. Expired request
identities are never silently reused.

Use typed bounded errors with safe correlation IDs: malformed input `400`, failed
authentication `401`, policy denial `403`, conflict `409`, expiry `410`, policy size
violation `413`, disallowed type `415`, integrity failure `422`, quota/concurrency
saturation `429`, and retryable dependency failure `503`. Omit file contents,
credentials, and private paths from errors and telemetry.

## Implementation Checkpoints

| Checkpoint | Changes | Completion evidence |
|---|---|---|
| 1. Contract/policy | Typed upload models in `mcp/contract/src/artifact_service/upload.rs`, gateway action/validation, quota/layout policy, schemas, designs | Schema, authority, 64-bit size, and deployment checks |
| 2. Durable storage | Service/HTTP/ledger/store upload modules, next migration, multipart adapter, leases, finalization, verification, immutable blob reuse, cleanup | Native SurrealDB and real S3-compatible concurrency/restart/failure tests |
| 3. Gateway | `platform/gateway/src/bin/gateway/artifact_upload.rs`, routing, auth/audit, policy discovery, streamed parts, status/completion | Configured-ingress HTTP acceptance, bounded memory, partial retries |
| 4. Console UX | BFF module, Artifacts action/panel, persistent shell queue/controller, worker, progress, recovery, accurate list update, ArtifactDrawer handoff | Session/CSRF tests; navigation continuity, identity boundaries, partial-failure recovery, and headed hardware-GPU browser acceptance |
| 5. Consumers/smoke | Python streaming download/file materialization, consumer-limit docs, Rust upload scenarios | URI-to-Python interoperability, exact bytes, tenant denial, bounded large download |
| 6. Installation/performance | Chart policy/schema, integration guides, Console docs, CODEMAP, component deployment plan | Multi-GB throughput/memory, restart/resume, ingress/quota/cleanup evidence and committed test report |

First release is incomplete until the Console form and multi-GB acceptance pass.
Use focused modules rather than expanding large gateway, service, or BFF files.
Update Artifact service and shared contract designs with implementation; repository-wide
technical documentation owns the gateway/Console flow.

Python `resolve` currently materializes the object. Keep it as an explicitly bounded
convenience and add streaming download with byte ceilings, cancellation, and temporary
file cleanup. Datasheet retains its own input/pandas memory limits; large upload
support does not certify pandas at that scale. Test small CSV/Parquet interoperability
separately from large transfer with a streaming consumer.

## Acceptance And Rollout

Run actual multi-GB transfers, including a file larger than 4 GiB to expose narrow
counters. Use 10 GiB in repeatable acceptance and 100 GiB for scale qualification on
a suitably provisioned installation. These are test sizes, not product caps.
Compare against native multipart throughput on the same store. Record upload plus
verification time, memory per process, concurrency, internal read traffic, retries,
and cleanup latency. Certify only capacity actually tested.

Test unknown/known total lengths, truncated parts, checksum failures, missing parts,
conflicting retries, completion races, quotas, token refresh, changed policy, replica
restarts, slow receivers, and database failure after storage commit. Accepted parts
must survive interruption. Prove memory stays bounded as file size grows and concurrent
sessions cannot bypass aggregate memory admission.

Console acceptance covers selection, validation, keyboard operation, progress,
pause/resume/cancel, reselection after reload, receipt recovery, list insertion,
Copy artifact URI, ArtifactDrawer, and denied access. It also proves that closing
the panel and changing Console routes preserve transfer, while a context or actor
change cannot transfer or reveal another session's files. Exercise a mixed-validity
batch, one failed file among successful files, a same-named wrong file on resume,
completion hidden by active filters, exact displayed sizes, and a completion/cancel
race. Capture transferring, finishing, recoverable-error, and ready states at desktop
and narrow widths. Before any browser run prove
headed mode and probe both WebGPU and WebGL where exposed; at least one must reach
hardware. Stop on loss of the last hardware-backed API. HTTP tests cannot substitute
for UI acceptance.

All smoke orchestration/assertions/retries/cleanup belong in Rust. Record affected
checks through `cargo xtask test-report run --name <check> -- <command>`, inspect
`cargo xtask test-report show`, and commit passing evidence with build-input changes.
Include Console build/lint, BFF tests, affected Rust crates, and Python enforcement.

Deploy through the component-scoped workflow: bootstrap Store, activate Artifact,
Gateway, and Console, then enable explicit policy. Validate actual ingress/proxy part
limits, timeouts, and buffering. Generic request limits cannot masquerade as total
artifact limits. Existing whole-body helpers retain bounded-request guards; large-file
paths use multipart. This documentation change needs no build evidence refresh.

## MCP App Follow-On

The Console Artifacts form is first-release work. Embedded MCP Apps requesting a host
picker remain request `017`, gated on exact App authority. Reuse the transfer machinery
but intersect the explicit App grant with installation policy. Bind App URI and grant
revision to trusted session authority. File bytes stay in the host origin and the
App receives only the receipt. Generic upload scope cannot bypass a missing App grant.

## Coding-Agent Handoff

Read `AGENTS.md`, `docs/CODEMAP.md`, `docs/WORK_CONTEXT_GOVERNANCE.md`,
`mcp/contract/DESIGN.md`, and `platform/artifacts/service/DESIGN.md` before implementation.
Inspect branch/worktree status and preserve other agents' work. Recheck the baseline
and dependency releases before modifying build inputs.

Implement the six checkpoints as coherent commits with recorded passing evidence.
Start by closing the typed API, quota policy, durable session/part schema, and immutable
blob-registration contract. The external endpoint and Console form are both required;
neither completes the task alone. The MCP App host-picker integration is the only
deferred UI scope in this plan.

Do not substitute larger body buffers for resumable transfer, leave Python's large-file
path as a whole-object allocation, skip crash/cleanup tests, or count API-only checks
as Console acceptance. Report the exact deployed size/throughput evidence and any
unmet acceptance item. Update this plan's status and component designs as each
checkpoint lands; the documentation commit itself supplies no runtime evidence.
