# Computers Public Types

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| JSON and JSON Schema | Serde DTOs and Schemars-generated schema bundle using the workspace's qualified pins; closed request objects and RFC 3339 timestamps |
| Veoveo Computers projection | Collection snapshots, lifecycle receipts and public phases; this library does not serve an HTTP or MCP endpoint |
| Veoveo terminal v2 | Bounded authenticated first frame, resize, ready, sequenced lease deadlines and explicit replay-complete controls; raw terminal bytes remain a separate frame type |
| Veoveo execution result | Known foreground exit code and stdout/stderr Artifact occurrence references; byte counts are bounded metadata, without command text or capability secrets |
| Veoveo regular-file handoff | Closed import/export request, canonical retained-relative path, explicit whole-run interruption scope and bounded Artifact result; typed public Task and result projection |
| Veoveo maintenance projection | Closed update input, admitted target inventory, progress/recovery phases and completed Task identity with the existing Computer resource URI; provider instances and protected policy remain private |
| Veoveo automation grant v1 | Named principal and OAuth-client scope, explicit permissions and bounded execution limits; generated JSON projection, no bearer authority |
| OpenShell CLI `0.0.116` pairing adapter | Custom confirmation-code and IPv4 loopback JSON callback; public requests select no host or authority |

These types carry public state without provider resource identifiers or authority
envelopes. Computer limits describe installation policy; a default of one does not
change the collection shape. Recovery Required describes an unresolved operation
whose domain fence remains held. A new Create requires a stable request UUID.
Start and Stop require that UUID as well. Create may supply an owned Reserved
`computerId` to finish interrupted provisioning from the visible collection. Omitting
it requests a new reservation. Reservation idempotency is owner-scoped; subsequent
lifecycle idempotency is scoped to the Computer. Their unreleased optional-request-ID
shape is removed before client generation. Lifecycle inputs never select an owner
or provider. Collection availability distinguishes Setup Required, quota exhaustion
and unavailable capacity. An unconfigured installation has no default template or
capacity limits. Action flags combine current policy, admitted state and availability;
the server still arbitrates concurrent admission.

`access.rs` defines the bounded inventory and idempotent revocation receipt. An access
grant ID locates an owned record and grants no authority. The inventory includes at
most 128 outstanding browser and CLI grants, with their kind and display name.
Redemption records a past event; it does not
claim a live attachment. `currentSession` identifies the caller's sign-in family,
which may include other tabs. `expiresAt` is an upper bound; policy, idle expiry and
revocation can close access sooner. Public values omit tokens, provider identifiers
and session-family IDs. These additions reuse the existing stored grant format and
terminal v2. Older endpoints reject the new routes without mutating a grant; retries
use the same Computer and grant IDs after the coordinated application rollout.

`pairing.rs` defines the stock CLI confirmation inputs and no-store responses. The
comparison code uses the qualified eight-character alphabet; names are trimmed,
contain no controls and fit 64 UTF-8 bytes. A callback port must be 1024–65535.
No input accepts a callback hostname, actor, profile or destination. The browser
delivers the one-use result only to `http://127.0.0.1:{port}/callback`. The result
binds the exact Computer, pairing and grant IDs. Its token has no Debug or Display
implementation and remains absent from inventories and URLs. This is a custom
OpenShell `0.0.116` adapter, without a claim of standardized device authorization.

The native Console and MCP projections share this schema. Implemented endpoint
coverage belongs in their own designs. Terminal tokens deliberately cannot be
formatted through Debug or Display; serialization is an explicit secret boundary.

`files.rs` defines regular-file import and export requests and their public Task
projection. The first profile caps each file at 64 MiB and each provider transfer at
300 seconds. A canonical path is relative to the retained home; the wire decoder
rejects traversal, empty components and controls while preserving Unicode and spaces.
The guest helper independently enforces kernel confinement. Import creates a new
file without overwriting. Archives remain opaque files. Neither extraction options
nor provider/owner selectors exist in this request.

The owner may omit `grantId`; delegated work must present current named authority.
Both paths require the existing Artifact read or write capability and current
Computer policy. `onInterruption: "stop_computer"` declares the active transfer's
whole-run containment scope. Task results contain a governed Artifact occurrence,
exact bytes and SHA-256. They expose no path, body or capability secret. These types
and generated clients do not activate the public tool or qualify its domain worker.

Terminal Ready establishes the connection and its initial short authority deadline.
A Lease control carries a strictly increasing connection-local sequence and a current
service-issued expiry. Relays preserve these values and enforce expiry with the clock
allowance in `platform/computers/transport/DESIGN.md`. A client-originated Lease control
is invalid. This addition is coordinated within unreleased terminal v2.

`automation.rs` defines named-principal grant input, inventory and revocation DTOs.
`principalId` identifies the grantee; it never selects the Computer owner.
`oauthClientId` binds the application through which that principal may use the grant.
Permissions are a nonempty unique set of Read, Execute, Start and Stop. Execute
requires explicit time and output limits plus `onInterruption: "stop_computer"`.
Cancellation, expiry or uncertain execution can stop other processes on that Computer
run while keeping retained files. This scope does not grant an independent Stop
action. The wire deserializer rejects duplicate and empty permission arrays. The domain enforces that conditional rule
and current installation ceilings in addition to schema validation. Public views
show the original scope and expiry; current policy can narrow them. These types are
shared by the public grant routes and command Tasks. The collection reports current
management hints and installation ceilings; each mutation checks current policy.


`execution.rs` defines a completed foreground result. Each stream has its own UUIDv7
Artifact occurrence, including an empty stream, and an exact byte count. The command
Task finishes with the standard tool result envelope. A nonzero exit sets `isError`
while preserving the known exit code and output references. Read authority is checked
by Artifacts; a result link does not confer it. The native profile reserves exit 124
for unknown execution, which cannot produce this result.

Foreground completion leaves the Computer running. Programs may leave detached
children, and revoking a grant does not undo completed writes or terminate those
children retroactively. The owner can Stop the Computer to end its run. Interruption
of an active command uses the grant's explicit whole-run Stop consent. A cancellation
received after a known foreground exit remains recorded in Task history; it cannot
replace that known result with a claim that the command never ran. An independently
admitted owner Stop keeps its own lifecycle fence through result settlement.

`ExecuteInput` is a closed command envelope with explicit argv, home-relative directory,
environment, standard padded base64 stdin and bounded execution limits. It accepts no
owner, tenant, image, provider endpoint or credential selector. It has no Debug surface.
The service and native codec enforce byte limits in addition to the JSON schema.
`ExecutionResultUri` accepts only the canonical UUIDv7 result path. Completed commands
use `computer://executions/{execution_id}` as their single addressable product. Artifact
IDs resolve through `artifact://{artifact_id}` under Artifact read authority.

`AutomationGrantResult` wraps one grant and its canonical result URI. Exact grant reads
include revoked/expired records even when the live inventory no longer lists them.
The empty `RevokeAutomationGrantBody` is closed and cannot carry additional authority.

`UpdateTemplateInput` selects an admitted template ID or the first request's default.
`MaintenanceState` exposes permitted targets and one active update. `MaintenanceView`
retains its Task identity and explicit progress/recovery phase. A saved target never
changes because a default changes. These types accept no image, fingerprint, owner or
provider selector. Cancellation and exhausted recovery do not imply source retirement
or release of retained capacity.

`ResumeUpdateInput` names the existing Computer/Task, a fresh request ID and the exact
paused `updatedAt`. A pending cancellation requires its timestamp in
`acknowledgedCancellationAt`. This input selects no new target or provider identity.
The same request cannot renew a recovery budget twice. Service and Console projections
use the qualified domain journal. `MaintenanceView.canResume` is an eligibility hint,
and `pendingCancellationAt` identifies the exact pending Task cancellation to acknowledge.
An acknowledged cancellation stays in private Task history while this public pending
field clears. A newer cancellation produces a new timestamp.

`FileTransferView` contains the domain stage and the shared Task's message, cancellation
request and completion times. It carries no path or capability. A Completed stage can
precede Task projection; consumers wait for its result before offering the Artifact.
`FileTransferResultUri` accepts only the exact canonical UUIDv7 resource address.
