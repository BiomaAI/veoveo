# Computers Public Types

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| JSON and JSON Schema | Serde DTOs and Schemars-generated schema bundle using the workspace's qualified pins; closed request objects and RFC 3339 timestamps |
| Veoveo Computers projection | Collection snapshots, lifecycle receipts and public phases; this library does not serve an HTTP or MCP endpoint |
| Veoveo terminal v2 | Bounded authenticated first frame, resize, ready, sequenced lease deadlines and explicit replay-complete controls; raw terminal bytes remain a separate frame type |
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
available to projections, while public routes and command Tasks remain integration
work in the domain plan.
