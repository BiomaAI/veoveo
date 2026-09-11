# Computers Gateway Admission

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| HTTP, RFC 9110 | Authenticated fixed control routes with closed public Computer JSON DTOs; no redirects |
| OAuth bearer, RFC 6750 | Existing profile authentication, current session-family validation and Work Context invocation authority |
| Veoveo internal assertions | Computers audience, verified actor and retained source request context; initial expiry bounded by the source access token |
| WebSocket, RFC 6455 | HTTP/1.1 through the shared terminal-v2 transport; deployment-owned TLS roots and optional client identity |
| Veoveo terminal v2 | Same-origin one-use attach, binary bytes, resize, replay fence and service-issued renewable deadlines |
| OpenShell CLI `0.0.116` adapter | Closed browser pairing projection and profile-bound internal binary tunnel; authority remains in the Computers ledger |

The gateway exposes `/computers/{profile}` for list and Create, with exact Computer
children for read, Start, Stop, terminal ticket and terminal upgrade. The selected
profile must admit the `computers` server. Its current manifest supplies the destination
and TLS trust. Public callers cannot choose a host, provider or internal route.

GET `/computers/{profile}/{id}/operations/{operation_id}` projects a stored operation
receipt under its parent's current `resources/read` authority. It requires no mutation
permission. Both returned IDs must match the route; inputs cannot add query authority.

The `/access` child exposes outstanding grants and POST `/access/{grant_id}/revoke`
reduces the owner's existing access under current parent read authority. It accepts
only an empty JSON body; contributor and new-attachment permission are unnecessary.
Returned parent/grant IDs and the bounded inventory are validated before forwarding.
Revocation stays within the Computer domain and never issues a provider command.
These are gateway-owned routes, independent of extension-owned route declarations.

Read uses ResourcesRead on the canonical collection or exact Computer resource.
Lifecycle actions use ToolsCall on the corresponding Computer tool. Terminal access
requires ComputerAttach and ResourcesRead on the exact Computer, contributor membership,
and a browser session family. Each policy decision is persisted before forwarding.
The Computers domain repeats current directory, policy, ownership and family checks;
the gateway cannot manufacture a durable grant or renew one.

Control bodies are capped at 64 KiB and responses at 2 MiB. The complete control
forwarding budget is thirty seconds. List admits only a typed `after` UUID; other
routes reject query strings. Mutation bodies use the closed shared DTOs. Responses
are decoded against their expected shape before returning JSON with `no-store`.
A ticket's Computer must match its route. Its endpoint becomes the gateway's own
uncredentialed terminal path. Source tokens, cookies and provider addresses are never
included in that projection.

Tickets and WebSocket upgrades require exactly one Origin equal to the configured
public origin. The WebSocket path accepts no query parameters. Initial admission uses
the existing Bearer middleware and an assertion addressed to Computers. The gateway
admits at most 128 local terminal relays. The service enforces durable installation
limits across replicas. Each relay uses the shared bounded queues and independently
enforces the service's current deadline during reads, writes and backpressure.

The HTTP and WebSocket pools share trust construction and configuration fingerprints.
Their connection pools remain separate because WebSocket selects HTTP/1.1. Updating
the catalog retires unused pool entries on the next corresponding acquisition.
Existing attachment authority still comes from current service renewal; a cached TLS
client cannot extend it. Provider code and retained-home credentials are absent here.

This is an unreleased route addition. The terminal contract's coordinated initial
release requires matching service, gateway and BFF versions. Installed acceptance
must include the real public chain, session-family revocation and the platform clock
bound specified by the shared transport design.

The CLI pairing POST and confirmation child use the same attachment actions,
contributor requirement, browser-family requirement and exact Origin as terminal
admission. The gateway validates the closed shared inputs and binds the returned
Computer/pairing IDs before forwarding a no-store response. Only the worker consumes
the challenge and issues its one-time credential.

GET `/computers/{profile}/cli/{id}/_ws_tunnel` and its root counterpart
`/computers/{profile}/cli/_ws_tunnel` accept the narrow internal CLI Bearer framing.
They sit outside ordinary OAuth JWT decoding. The configured profile must contain
Computers; its current manifest and trust select the sole upstream destination.
The worker compares that route profile with the grant's persisted profile and owns
current authorization. No assertion is synthesized from an expired browser token.
This internal relay preserves service-issued Ready/Lease controls for the BFF, which
removes them before the stock CLI sees bytes. The private provider SDK remains absent.

Environment update routes use the same fixed authenticated boundary. GET `/maintenance`
and `/maintenance/{operation_id}` beneath a Computer require its parent resource-read
policy. POST `/update-template` requires contributor membership and the `update_template`
tool policy. Its closed input must name the route's Computer and a non-nil request ID;
the optional template is an admitted catalog ID. The gateway validates receipt Computer
and Task identities, bounded distinct target IDs, and consistent recovery phases before
forwarding. No provider client or maintenance state machine belongs in this adapter.
