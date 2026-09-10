# Computers Public Types

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| JSON and JSON Schema | Serde DTOs and Schemars-generated schema bundle using the workspace's qualified pins; closed request objects and RFC 3339 timestamps |
| Veoveo Computers projection | Collection snapshots, lifecycle receipts and public phases; this library does not serve an HTTP or MCP endpoint |
| Veoveo terminal v2 | Bounded authenticated first frame, resize, ready, sequenced lease deadlines and explicit replay-complete controls; raw terminal bytes remain a separate frame type |

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

The native Console and MCP projections will share this schema. Implemented endpoint
coverage belongs in their own designs. Terminal tokens deliberately cannot be
formatted through Debug or Display; serialization is an explicit secret boundary.

Terminal Ready establishes the connection and its initial short authority deadline.
A Lease control carries a strictly increasing connection-local sequence and a current
service-issued expiry. Relays preserve these values and enforce expiry with the clock
allowance in `platform/computers/transport/DESIGN.md`. A client-originated Lease control
is invalid. This addition is coordinated within unreleased terminal v2.
