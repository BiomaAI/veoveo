# Computers Service And MCP Projection

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Shared Tasks | Qualified `provider_wait` observation leases, domain-first settlement, retained Task projection |
| Veoveo Computers | Provider-independent operation, Computer, owner and Work Context records in `platform/computers` |
| Native OpenShell | Private mTLS/protobuf adapter in `platform/runtimes/computers`; its exact provider patch graph governs the selected Docker profile |
| MCP 2026-07-28, repository contract revision 3 | Stateless authenticated lifecycle tools, resources, Tasks and request-scoped subscriptions; installed conformance remains pending |
| WebSocket RFC 6455 and Veoveo terminal v2 | Browser-only first-frame ticket, bounded binary terminal, resize, replay fence and sequenced renewal deadlines; the gateway authenticates the upgrade |
| JSON Schema 2020-12 | Shared public DTOs in `platform/computers/contract`; raw provider messages are never public request inputs |
| `veoveo.io/computers-service/v1` | Closed installation JSON with template fingerprints and private trust-file references; distinct from public Computer inputs |
| OCI Linux AMD64 | `computers-mcp` Bake target, shared Veoveo Rust compiler and digest-pinned Debian trixie runtime with signed archive snapshot `20260910T000000Z` |

The worker and MCP/relay compose in one Computers deployment. The gateway owns
ordinary catalog and action policy without importing the provider SDK. The `computers-mcp` executable serves the same HTTP router used by fixtures.

## Installation Configuration And Process

The service image runs as UID 10001 and contains the production executable and
runtime package inventory. The core chart mounts configuration and dedicated
worker trust as described in [`deploy/helm/veoveo/DESIGN.md`](../../deploy/helm/veoveo/DESIGN.md).
It gives the service no compute-host socket or retained-home mount. Core presets
include its control deployment even when capacity is explicitly unconfigured.

`computers-mcp --config /etc/veoveo/computers.json` starts the service. The equivalent
configuration reference is `VEOVEO_COMPUTERS_CONFIG`. It uses the installation's
`VEOVEO_SURREAL_ENDPOINT`, `VEOVEO_SURREAL_NAMESPACE`, `VEOVEO_SURREAL_DATABASE`,
`VEOVEO_SURREAL_AUTH_LEVEL`, `VEOVEO_SURREAL_USERNAME`, `VEOVEO_SURREAL_PASSWORD` and
`VEOVEO_INTERNAL_TRUST_JWKS`. Store credentials must be database-scoped. Apply store
migrations through the installation owner before starting this process.

The configuration schema is `veoveo.io/computers-service/v1`. Its closed root fields
are `schema`, `listen`, `allowedHosts`, `allowedOrigins`, `access`, `providerInstanceId`
and `capacity`. `allowedOrigins` contains distinct canonical HTTPS origins. Explicit
HTTP loopback origins are admitted for local fixtures. Opaque origins, credentials,
paths, fragments, queries and duplicate Origin headers are rejected. `access` contains
`maxGrants`, `absoluteSeconds` and `idleSeconds`; the domain validates and installs
these limits through an exact retry or explicit compare-and-set transition.
The provider UUID identifies the installation's retained capacity boundary and survives
ordinary service restarts. Explicit `capacity: {"kind":"unconfigured"}` keeps the
core collection available with Setup Required. It creates no capacity policy.

The qualified local variant uses `kind: "openshell_docker"`. Its fields are `gateway`,
`allocator`, `limits`, `templates` and `defaultTemplate`. Gateway contains `workspace`
and `transport`; allocator is itself a transport. Each transport provides `endpoint`
as a private host:port plus absolute `caFile`, `certificateFile` and `keyFile` paths.
Provider and allocator trust are separate installation inputs. No credential belongs
in the JSON document. Kubernetes supplies the referenced secrets through mounted files.

Limits use `perOwner`, `perTenant` and `provider`. Every template contains `id`,
`fingerprint`, `image`, `cpus`, `memoryMib`, `homeCapacityMib`, `temporaryMib` and
`policy`. The policy uses the pinned provider's protobuf JSON mapping. The service
selects the canonical retained login command and verifies the entire template against
its declared fingerprint. `defaultTemplate` names one admitted fingerprint; historical
fingerprints remain available for retained Computers. A new default does not alter them.

Configuration loading and validation each have a ten-second deadline. The regular JSON
file is capped at 1 MiB. Validate all referenced trust material and selected templates
before connecting to or writing the platform store. Operator errors identify the
configuration boundary or template index without echoing policy or credential bytes.
A provider outage does not turn complete configuration into a malformed installation.

The service installs capacity through the existing compare-and-set transaction. Startup
admits an initial policy or its exact retry. Changing a persisted quota requires the
installation owner to supply the previous policy to the domain's explicit transition;
a stale replica cannot restore its former limits. Binding or configuration failure
never starts a worker.

The process connects to the qualified native provider in the background. Readiness is
observed every five seconds and expires through the shared availability projection.
Allocator readiness checks each admitted template with at most eight concurrent calls.
These are readiness probes; lifecycle completion retains its operation-correlated watch
and recovery profile. An unavailable allocator still permits Stop.

Provider readiness loss cancels local worker futures and reconnects through the pinned
handshake. The journal retains every outstanding dispatch and Task lease. Shutdown and
outer-future cancellation propagate to background workers. Their cancellation ends local
observation and access, leaving provider execution and retained homes under domain policy.
The worker never owns a Computer's lifetime through a process-local connection.

## Public Application Projection

`Application` owns the command/read projection shared by the MCP and
Console HTTP adapters. Each request obtains one current policy and directory snapshot
from the domain. It serves all action flags for that response and expires within
thirty seconds, capped by the request's admission lifetime. A new request does not
reuse that snapshot. The worker independently obtains its dispatch permit later.

The collection reports Setup Required without a fabricated template or quota. Quota
exhaustion reflects the owner's usage across Work Contexts plus tenant/provider
counts; reservation remains transactional. A provider availability observation expires
after fifteen seconds. Storage unavailability does not prohibit Stop or an attachment
to an already Ready Computer. Connect requires a live qualified provider connection,
current browser-family/action authority, enabled grant policy and an admitted unfenced
Ready Computer. Grant exhaustion remains transactional. Delete awaits retained purge.

The installation retains admitted templates by fingerprint and selects a default for
new Create requests. A retry resolves its first reservation before consulting that
default. Concurrent replicas that select different defaults converge on the same
first accepted Computer and Task. Caller input cannot choose arbitrary images.
Closed public lifecycle inputs require stable request UUIDs. Native transport,
provider work and terminal grants are separate from this projection's store tests.

## Canonical Protocol Projection

The authenticated mount is `/computers`. `/mcp` serves MCP and `/admin` serves the
native Console projection over the same `Application`. Only `/healthz` is anonymous;
it checks the platform store with a five-second deadline. Provider outages appear in
the collection independently of control-plane liveness. Every secured request requires
a gateway-signed Computers-audience assertion and an admitted Host authority.

The collection is `computer://computers`. Exact resources use
`computer://computers/{computer_id}` and subsequent pages use
`computer://computers?after={after}`. UUIDs in these resource URIs must use the
canonical lowercase hyphenated representation. Completion queries the current owner's
indexed collection with a bounded prefix query. It returns at most one hundred IDs.
Current resource policy and persisted ownership apply independently.

Create, Start and Stop are durable tools. Each requires per-request Tasks extension
support before reservation; rejection uses the final protocol capability error.
An accepted call returns the shared Task seed. Tasks/get and update require current
read authority over that Task's Computer. Cancellation additionally requires the
current lifecycle action authority. Cancellation after dispatch remains a request
whose effect is observed by the worker. Output schemas cover completed lifecycle
results and typed domain rejections. Task results use the canonical Computer URI.

The HTTP collection supports GET and POST. Exact reads use
`/admin/computers/{id}`, with POST `/start` and `/stop` below that path. Queued or
running operations return HTTP 202 with the same Task ID as MCP. Known completed
outcomes return HTTP 200. Inputs cannot select an owner, provider or arbitrary image.
Create may name an existing owned Reserved Computer through `computerId`; this lets
reload recover an interrupted reservation without its original browser state.
Omitting the ID requests a new reservation. Reservation request IDs are owner-scoped;
lifecycle request IDs are scoped to the selected Computer. Exact retries retain the
first accepted operation and template even if the installation default changes.

Each request admits at most 64 KiB and has a thirty-second response deadline.
A timeout does not certify whether admission committed; retry uses the same request
ID. The shared MCP middleware enforces the final serialized 8 MiB response cap.
The `develop` prompt describes the retained-work journey. Catalogs are static and
advertise no list-change support. Embedded docs and contract declarations are served
under `computer://docs`, `computer://contract` and authenticated `/admin/docs`.

## Browser Terminal Access

POST `/admin/computers/{id}/terminal-ticket` accepts the closed empty ticket input,
checks the exact Origin and current domain authority, and returns HTTP 201 with
`Cache-Control: no-store`. The response carries a one-use opaque token and an
uncredentialed terminal path. The BFF owns its public path and cookie/CSRF boundary.
GET `/admin/computers/{id}/terminal` requires a current Computers-audience assertion
and admitted Origin before upgrade. Each replica admits at most 128 terminal streams.

The first WebSocket message must be a terminal-v2 attach control within five seconds.
Its text is capped at 1 KiB and binds the route Computer, ticket and terminal size.
The durable domain redeems it once across replicas. The service discards the token
before provider setup. Every later setup failure or disconnect closes that exact
redeemed grant; cleanup has the domain's five-second deadline. An interrupted process
still leaves the retained grant's idle and absolute limits in force.

A fresh grant baseline supplies the attachment lease before private provider I/O.
The connected runtime, returned resource and main process must match the retained
Computer before forwarding bytes. The runtime connection is published only after a
qualified handshake and successful current readiness probe. A lost publisher, stale
probe or changed provider prevents attachment and renewal.

The service owns one set of outbox, browser-family and active-policy LIVE listeners
per replica. Their bounded fanout contains invalidations, never permission. Computer
and family wakes select the relevant attachments; policy changes invalidate all.
An overrun requires a fresh authoritative baseline. Losing a listener ends its epoch,
including when it reconnects before a consumer runs. The replacement epoch admits a
new attachment. No old attachment silently adopts a recovered observer.

Renewal rereads current domain grant, family, directory, policy and Computer state.
A five-second baseline handles a missed wake. Input and wake bursts coalesce for
500 milliseconds; only acknowledged terminal input can extend idle activity. Resize,
output and keepalive frames do not extend it. Grant absolute expiry remains fixed.
Both the runtime and outer service guard enforce the monotonic lease independently
of authority reads, provider setup and blocked WebSocket directions. Lease failure,
observer loss and shutdown close access while native execution remains governed by
its separate lifecycle policy.

Binary messages preserve terminal bytes and are capped at 64 KiB. The WebSocket write
buffer is bounded at 128 KiB; provider queues retain their existing bounds. Independent
input and output futures share the same lease. The server sends Ready followed by
bounded history and ReplayComplete. Input is rejected until that fence has been sent.
The Console must additionally drain historical rendering before enabling keyboard
input or terminal responses. Ready's expiry is the initial short authority projection;
the service continues enforcing subsequent renewals. Successful renewal emits a Lease control with a strictly increasing connection-local
sequence and the current expiry. The public relays preserve these controls and enforce
the deadline with their declared clock allowance. The client must not treat an
attachment deadline as the Computer's lifetime.

The gateway relay, Console renderer and public ingress remain separate delivery work.
Local native qualification exercises real shell bytes through this service; it is
not headed-browser or installed-user evidence.

## Resource And Task Subscriptions

Every listener has its own sink and authorization. The process admits at most sixty-four
listeners, each with at most sixty-four resource/Task targets. It validates the entire
accepted set before sending private updates. A new resource listener receives an
invalidation baseline and reads current state; transport replay IDs are absent.

The shared platform outbox is authoritative across replicas. LIVE notifications wake a
bounded persisted-page drain. LIVE loss closes the listener and requires a new baseline.
Task notifications use the shared durable Task subscription helper. All notifications
remain filtered by current ownership and requested targets. Provider events and terminal
contents never enter these streams.

Current policy and the signed browser family are checked every five seconds, with a
shared five-second authority-read bound. The earliest assertion, token, known family
expiry or current-policy deadline caps the entire listener, including initialization
and a blocked sink. The guard polls cancellation and expiry while the data pump or
an authority read is blocked. Loss of current
authority closes the stream; a client must present renewed authority on a new request.
This is control-state subscription behavior, distinct from renewable terminal grants.

## Lifecycle Execution

A bounded queue repairs accepted operations whose shared Task link was lost. It
walks UUID pages and runs at most sixteen operations concurrently. Each operation
claims the shared observation lease for sixty seconds and renews every ten seconds.
A renewal failure abandons local provider work while retaining the domain fence.
The next worker reads the same journal and cannot acquire another dispatch ticket.
After a bounded observation ends, the worker releases its exact lease receipt. A
successor can use the persisted schedule immediately. Recovery Required leaves the
automatic work queue after its waiting projection, avoiding repeated claims forever.

Create, Start and Stop resolve the recorded template fingerprint. A new default
cannot replace it. Configured runtime and store provider identities must agree.
Every worker template requires a retained home. `RetainedHomes` binds the provider,
admitted template fingerprint and capacity to the private allocator client. Create
prepares the allocation; Start restores that exact home and instance. The domain enforces current action authority during dispatch;
the storage adapter cannot override it. Stop does not depend on allocator availability.
Authority is checked after potentially slow storage preparation. The ticket includes
read latency in its thirty-second maximum
and covers journal admission and provider submission. Observation can continue after
that submission permit expires because it records an existing effect.

Start checks the source resource and stopped process before mutation. Stop checks
the source resource and running process. Full binding checks and the persisted
lifecycle checkpoint also apply to synchronous replies and watch events. A healthy
watch performs no reconciliation reads. Loss of reply or watch enters the domain's
charged, persisted observation budget. Exhaustion projects Recovery Required and
keeps the operation protected.

Cancellation before dispatch restores the previous Computer phase and records a
known undispatched outcome under the Task lease. A dispatch/cancellation race is
serialized on the domain journal. Cancellation after dispatch remains a request;
the original effect is observed and a known result can still complete successfully.

Domain settlement precedes Task completion. A durable projection marker precedes
release of the Task retention pin; queue discovery also repairs an interrupted pin
acknowledgement. Completed Tasks cannot be recreated after ordinary retention cleanup.
Audit events contain identities and provenance, without commands or credentials.

## Contract Compliance

The router implements the protocol and HTTP projection described above. Registration, the public browser relay/Console, execution/file tools and production
packaging remain delivery work. The runnable entrypoint and its startup/shutdown path
are qualified with an isolated store and unavailable provider endpoints. That evidence
proves truthful control availability and validated configuration, not native capacity. The checklist in AGENTS.md declares those gaps.
Real-store HTTP fixtures use distinct database connections and service replicas. They
exercise missing-capability admission, exact retry identity, private Task/resource
access, schema bounds, current cancellation policy, reconnect baselines and subscription
closure after policy removal, logout, family expiry or assertion expiry. Capacity health is synthetic in these
fixtures; they establish neither provider execution nor installed acceptance.
Its native fixture uses the actual current-policy reader, isolated installation
revision, production retained allocator and native OpenShell provider. Two worker
replicas compete for Create, then Stop succeeds with the allocator offline. Start
after allocator replacement preserves the file and changes the process identity.
Lost dispatch/settlement and current-policy cases retain their domain fence assertions.
The fixture establishes this composition; it does not establish public MCP or
installation acceptance.
