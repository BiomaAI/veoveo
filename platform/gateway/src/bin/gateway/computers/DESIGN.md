# Computers Gateway Admission

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| HTTP, RFC 9110 | Authenticated fixed control routes with closed public Computer JSON DTOs; no redirects |
| OAuth bearer, RFC 6750 | Existing profile authentication, current session-family validation and Work Context invocation authority |
| Veoveo internal assertions | Computers audience, verified actor and retained source request context; initial expiry bounded by the source access token |
| WebSocket, RFC 6455 | HTTP/1.1 through the shared terminal-v2 transport; deployment-owned TLS roots and optional client identity |
| Veoveo terminal v2 | Same-origin one-use attach, binary bytes, resize, replay fence and service-issued renewable deadlines |

The gateway exposes `/computers/{profile}` for list and Create, with exact Computer
children for read, Start, Stop, terminal ticket and terminal upgrade. The selected
profile must admit the `computers` server. Its current manifest supplies the destination
and TLS trust. Public callers cannot choose a host, provider or internal route.

GET `/computers/{profile}/{id}/operations/{operation_id}` projects a stored operation
receipt under its parent's current `resources/read` authority. It requires no mutation
permission. Both returned IDs must match the route; inputs cannot add query authority.
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
