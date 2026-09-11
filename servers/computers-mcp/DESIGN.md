# Computers Service And MCP Projection

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Shared Tasks | Qualified `provider_wait` observation leases, domain-first settlement, retained Task projection |
| Veoveo Computers | Provider-independent operation, Computer, owner and Work Context records in `platform/computers` |
| Native OpenShell | Private mTLS/protobuf adapter in `platform/runtimes/computers`; its exact provider patch graph governs the selected Docker profile |
| MCP 2026-07-28, repository contract revision 3 | Stateless authenticated lifecycle tools, resources, Tasks and request-scoped subscriptions; installed conformance remains pending |
| WebSocket RFC 6455 and Veoveo terminal v2 | Browser-only first-frame ticket, bounded binary terminal, resize, replay fence and sequenced renewal deadlines; the gateway authenticates the upgrade |
| Stock OpenShell CLI `0.0.116`, gRPC over HTTP/2 over WebSocket | Restricted internal adapter for five qualified SSH methods; private Ready/Lease controls are removed by the public edge before reaching the stock client |
| JSON Schema 2020-12 | Shared public DTOs in `platform/computers/contract`; raw provider messages are never public request inputs |
| `veoveo.io/computers-service/v2` | Closed installation JSON with template fingerprints and private trust-file references; distinct from public Computer inputs |
| OCI Linux AMD64 | `computers-mcp` Bake target, shared Veoveo Rust compiler and digest-pinned Debian trixie runtime with signed archive snapshot `20260910T000000Z` |

The worker and MCP/relay compose in one Computers deployment. The gateway owns
ordinary catalog and action policy without importing the provider SDK. The `computers-mcp` executable serves the same HTTP router used by fixtures.

## Retained Maintenance Worker

`MaintenanceWorker` composes the domain journal with the pinned provider, retained-home
allocator and installation-owned key ring. Each private step uses one original dispatch
ticket and renews the shared Task lease during provider work. A lost provider mutation
reply enters the domain's finite observation budget. Stop and Create recovery read the
original binding. Retirement recovery checks provider absence, while the independent
allocator still has to prove physical writer exclusion. An initial Create with unknown
effect uses the allocator's qualified unclaimed abandonment profile and retains its
original provider history and quota.

Allocator transfer is the one qualified idempotent replay: every attempt uses the same
durable maintenance, source and target identities. The allocator can return its original
receipt after a lost reply or helper restart and refuses superseded targets. Reacquiring
that receipt for policy verification grants no new instance or dispatch identity.

Capture writes its encrypted checkpoint through the domain transaction. The worker
reopens and validates it before retiring a known source. Restore reads the same private
row after source deletion and accepts only the exact retained handoff receipt. A lost
policy-update reply uses read-only reconciliation and cannot resubmit UpdateConfig.
The worker verifies the recorded target resource, process and restored policy before
adoption, consuming a final bounded observation ticket. It publishes the typed
`MaintenanceResult` and repairs retention acknowledgement across replicas.

`MaintenanceProfiles` accepts at most 256 directed installation-admitted template
transitions. Both endpoints must exist in the admitted catalog and pass the runtime's
image-only transition preflight. Declaring a pair is not qualification of its image
data compatibility. The native fixture supplies one same-image pair. The scheduler
runs at most four jobs per replica, pages the domain journal and defers Task-store
conflicts without repeating an uncertain dispatch. Shutdown ends local futures while
the journal retains its fences. The executable starts this scheduler from the validated
installation configuration. Different-image, native resumption and installed acceptance
remain integration work.

## Public Environment Updates

The `update_template` tool and POST `/admin/computers/{id}/update-template` share
one application command. Inputs contain `computerId`, `requestId` and an optional
installation `templateId`. Template IDs are unique within a catalog. Omitting the
ID selects the current default on first admission; retries resolve the saved target
before inspecting a changed default or current capacity. A changed explicit target
conflicts. Concurrent selection races preserve the first accepted target.

Current contributor membership, owner visibility and `update_template` action policy
are required. Agents do not inherit this authority from an Execute grant. The worker
independently rechecks current policy before each new step. An active command prevents
maintenance admission; finishing or explicitly stopping it precedes the update.

GET `/admin/computers/{id}/maintenance` and the subscribable resource
`computer://computers/{id}/maintenance` return admitted targets, current eligibility
and the active maintenance projection. GET `/admin/computers/{id}/maintenance/{task}`
returns an exact retained receipt. Projections expose template IDs and progress phases,
including an explicit recovery reason. They contain no provider, instance, checkpoint
or policy fingerprint. Recovery does not release the fence or present success.

MCP requires Tasks support before admission. The result is the same durable Task as
HTTP, whose queued/running receipt uses status 202. Completed or paused receipts use
status 200. Task reads and subscriptions require current Computer read authority;
cancellation additionally requires current update authority. Maintenance Task authority
has a five-second observation window and subscriptions revalidate every five seconds.
The existing Computer resource remains the canonical completed result.

`resume_update` and POST `/admin/computers/{id}/maintenance/{task}/resume` use the
same domain resumption command. Current named recovery policy and ownership govern
admission. Exact retries resolve the saved request before capacity or profile checks;
a new window requires the original directed template transition to remain admitted.
The closed input binds both route identities and the paused update timestamp. Pending
cancellation requires its exact timestamp as explicit acknowledgement. The returned
Task keeps its original identity and result contract.

The public receipt exposes `canResume` and `pendingCancellationAt`. Eligibility combines
current recovery policy, a recoverable shared Task and admitted available capacity.
The domain still checks the exact paused epoch and current authority transactionally.
Missing Task linkage disables recovery eligibility until normal worker repair. A
maintenance Task cancellation invalidates its Computer resources even while the worker
is paused, allowing the Console to refresh current cancellation consent. That path
resolves the owned maintenance journal and does not expose another owner's Task.

The Console and gateway consume these fixed routes. The service wire fixture establishes
cross-replica admission, default rotation, Task access and revocation using a synthetic
capacity profile. It makes no installed or image-compatibility claim.

## Public Command And Grant Admission

MCP `execute` accepts explicit `arguments`, a home-relative `directory`, bounded
`environment`, standard padded base64 `stdin`, execution `limits`, and the exact
Computer, grant and request UUIDs. The service admits at most 2 MiB of serialized
MCP request and 1 MiB of decoded stdin. The guest codec additionally bounds argv
to 32 KiB and environment to 16 KiB. Arguments are not interpreted by an implicit
shell. A caller can explicitly select an admitted shell inside the Computer.

Tasks capability admission precedes reservation. The application checks the current
named principal/client grant and the installation's execution-qualified template set,
then seals the payload and creates one metadata-only Task. Exact retries return that
Task across replicas. An altered command under the same request ID is rejected.
Provider reconnect does not disable access to the application key ring or Artifact
client. The worker independently authorizes dispatch.

Output preparation forwards the verified request's actual internal bearer to Artifacts.
Its Task-bound capability is sealed separately; the bearer is never persisted. A failed
five-second preparation attempt returns the admitted Task. An identical request can
repair preparation while the caller remains present. Without repair, the queued
preparation budget expires without executing the command. Authority and Task ownership
are checked again before returning the Task seed.

A known result has one `result_uri`, `computer://executions/{execution_id}`, and one
resource link. Reading it requires current command Task authority. The original agent
loses access after revocation; the direct owner retains oversight. stdout and stderr
contain Artifact occurrence IDs and byte counts. Their bytes are read through
`artifact://{artifact_id}` or the governed Artifact HTTP download, under independent
Artifact authority. Computers introduces no duplicate byte route.

`grant_automation` and `revoke_automation` share the domain management checks with
GET/POST `/admin/computers/{id}/automation`, exact GET
`/admin/computers/{id}/automation/{grant_id}`, and POST of an empty JSON object to
`/admin/computers/{id}/automation/{grant_id}/revoke`. Issuance carries the same
`computerId` as its route. The collection reports current management hints and
installation ceilings. Hints authorize no mutation. Each terminal grant result has
one canonical `computer://computers/{computer_id}/automation/{grant_id}` link;
exact owner reads preserve revoked and expired state. The collection and exact
grant resources support current-authority invalidations. The gateway projects
management as the named tool policy, and the Console BFF retains session/CSRF
ownership. Agent lifecycle and governed file movement remain separate delivery work.

Migration 0067 backfills canonical result URIs in completed command journals and
successful shared Tasks in one migration transaction. It rejects mismatched Task
identity, stored outcome or error semantics before rewriting either record. Drain
old readers/writers before upgrading; preserve the migration history on rollback.
Unfinished Tasks project their result with the new worker. This change does not
rewrite audit history or claim installed migration acceptance.

## Governed Command Worker

`CommandWorker` composes the command journal, live authority and qualified framed
guest launcher. It requires an explicit set of execution-qualified template
fingerprints, the installation key ring and the internal Artifact client. Its
provider-scoped scheduler admits four active commands per replica. Captured payload
bytes are limited to 256 MiB at that concurrency; allocator and transport overhead
are additional. Each Computer also retains its durable exclusive execution slot.
Configured startup runs the command scheduler beside lifecycle observation. It reads
only enough pending command envelopes to fill its open slots. Public execution admission uses the same domain journal. Installed command
qualification remains required.
Each scheduler task boxes its command-step future to bound the task's stack footprint.

Only the original dispatch receipt can launch the command. A successor that finds
Dispatched contains the saved run without replaying command bytes. Current authority
is checked each second, with an independent five-second expiry that stays active
during blocked reads. The original runtime deadline remains independent as well.
The worker recovers the current same-worker Task lease after an interrupted renewal
before writing settlement. Lost ownership leaves the journal for a successor.

Known foreground exit publishes stdout and stderr as separate governed Artifacts.
Publication uses the prepared Task-bound capability and fixed per-stream idempotency
keys, then validates occurrence size, descriptor, tenant, Work Context and inherited
labels. Output identity and grants belong to the Artifact service. Resource links
convey references only; their public resolution and authorization remain part of the
pending execution projection. Nonzero exit retains its outputs and sets tool error
status. This worker makes one publication attempt per stream. An unavailable result
enters containment; a partially published authorized Artifact may remain.

Cancellation, authority loss and unknown outcomes close local foreground I/O before
entering the durable containment journal. Confirmed termination follows the native
Stop profile, which includes the pinned Docker driver's ten-second grace interval.
The five-second I/O authority bound is not a five-second process-termination claim.
Database or provider unavailability keeps the original run fenced until positive
Stop evidence or explicit Recovery Required. Successful foreground completion leaves
the Computer running under its independent lifecycle policy.

The native fixture owns its real provider, retained allocator, database and HTTP
Artifact service with filesystem bytes. Its foreground journey grants and executes
through the production HTTP router with the workspace SDK, follows Task subscriptions,
and reads the canonical result resource. Two continuous command schedulers compete for
the same journal; an exact retry after completion returns the original Task. The fixture
requires the selected image digest in the host cache before creating resources.
It checks binary stdin,
private environment, nonzero exit output, current Work Context ownership, grant
revocation, Task cancellation, retained restart and lost-ticket refusal without
command replay. Fixed phase/error-class diagnostics exclude command and credential
contents. These cases do not establish installed agent usability.
The command fixture now starts from a real allocator-adopted replacement home and
persists that instance in its isolated domain. Lifecycle Create/Stop/Start, SDK command
dispatch and containment must all address that same replacement. Fixture adoption is
explicit setup; it does not qualify the still-pending product maintenance workflow.

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

The configuration schema is `veoveo.io/computers-service/v2`. Its closed root fields
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
`allocator`, `limits`, `templates`, `defaultTemplate`, `execution` and
`maintenanceTransitions`. Gateway contains `workspace`
and `transport`; allocator is itself a transport. Each transport provides `endpoint`
as a private host:port plus absolute `caFile`, `certificateFile` and `keyFile` paths.
Provider and allocator trust are separate installation inputs. No credential belongs
in the JSON document. Kubernetes supplies the referenced secrets through mounted files.

Limits use `perOwner`, `perTenant` and `provider`. Every template has a unique `id` and contains
`fingerprint`, `image`, `cpus`, `memoryMib`, `homeCapacityMib`, `temporaryMib` and
`policy`. The policy uses the pinned provider's protobuf JSON mapping. The service
selects the canonical retained login command and verifies the entire template against
its declared fingerprint. `defaultTemplate` names one admitted fingerprint; historical
fingerprints remain available for retained Computers. A new default does not alter them.

`execution` contains `policy`, `artifactEndpoint`, `activeKeyId`, `keys` and
`templateFingerprints`. Policy limits are `maxGrants`, `maximumLifetimeSeconds`,
`maximumExecutionSeconds` and `maximumOutputBytes`. Startup installs that policy
through the domain's initial-or-exact-retry transaction. Changes require an explicit
compare-and-set transition. Artifact uses a private HTTP(S) origin without credentials,
query or path. The default template must be in the distinct execution-qualified set;
retained templates without the framed launcher remain admitted for their other actions.

`maintenanceTransitions` declares at most 256 distinct directed pairs of
`sourceFingerprint` and `targetFingerprint`. Both endpoints must name admitted
retained templates with compatible resource, home, command and static policy profiles.
Only the image may differ. The empty list admits no new environment update. This
installation declaration records a separately tested transition; execution qualification
alone does not establish retained data compatibility. Bioma keeps the list empty until
its exact old-to-new image transition passes the native retained-home qualification.

Each key has a non-nil `id` and an absolute `file` reference. Accept one to four
32-byte regular files, with no access for other users or group writers. Kubernetes
may grant read access through the worker fsGroup. The active key must be present.
Keys are read into zeroizing buffers; no key or protected command is printed.
All replicas share active and retained keys. Add a new key to every replica before
selecting it for writes; retain old keys until all dependent envelopes have expired
under the retention policy. Never remove keys while pending Tasks still need them.
Installation-owned encrypted backup includes these keys alongside the encrypted store.

This private configuration is a coordinated v2 hard cut. Drain v1 workers, apply
migrations through 0070, provision command keys, and start v2 workers with the matching
configuration. A changed default also requires the compute host to admit that exact
template through its qualified retained-maintenance procedure. A service rollout alone
does not upgrade a retained Computer. Downgrade must drain command admission and
in-flight work; v1 cannot recover command Tasks. Preserve keys and schema on rollback.

Lifecycle, CLI and browser attachment resolve the persisted replacement instance as
well as the stable Computer/home UUID. Command dispatch and containment use the
instance authenticated in the command envelope. These paths cannot fall back to the
original provider name after maintenance. Persisting an instance identity alone does
not authorize provider replacement or storage adoption.

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

The process connects to the qualified native provider in the background and supervises
lifecycle, command and maintenance schedulers together. A scheduler exit withdraws
compute availability and cancels its peers before reconnecting. Readiness is
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

GET `/admin/computers/{id}/operations/{operation_id}` reads that same durable
operation as a typed receipt. A successful read returns HTTP 200 regardless of
operation status. Current Computer read authority and original operation ownership
apply; a mismatched parent is not found. The read neither dispatches work nor queries
the provider. MCP clients retain `tasks/get` as their canonical Task read path.

GET `/admin/computers/{id}/access` and resource
`computer://computers/{computer_id}/access` read the same owned grant inventory.
POST `/admin/computers/{id}/access/{grant_id}/revoke` accepts a closed empty JSON body;
the synchronous `revoke_access` tool accepts the same IDs as arguments. Both return a
typed revocation receipt. Revocation is idempotent and needs no provider mutation or
Tasks extension. The domain applies current owner/read authority even when new access
is no longer admitted. Current service leases close the attachment; execution remains
running. Grant outbox events invalidate the collection, Computer and access resource.

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

The gateway and BFF compose the shared relay. Native fixture evidence and installed
headed-browser qualification are recorded separately in the Computers plan and
build/deploy audit.

## Stock CLI Access

GET `/computers/cli/{profile}/{id}/_ws_tunnel` binds an exact Computer and retained
profile. GET `/computers/cli/{profile}/_ws_tunnel` resolves the parent from the authenticated retained grant
because the stock SSH ProxyCommand reconnects through the root tunnel route. Both
require exactly one opaque CLI credential in the Authorization Bearer header. Query
parameters, Origin, Cookie, WebSocket subprotocols and extensions are rejected.
An ordinary gateway assertion cannot replace the narrow credential. The public edge
owns the stock client's edge-token header adaptation; it must strip cookies before
forwarding and cannot inherit browser-cookie authority.

Admission uses the shared 128-stream replica limit and atomically allocates one of
at most sixteen live connections for the named grant. Every connection binds a
current provider resource and process. Closing it preserves the parent grant.
Failed upgrades release the exact connection; its durable deadline bounds cleanup
when the service disappears. No provider I/O begins before fresh domain renewal.

Browser and CLI attachments share `attachment_authority.rs` and one `AccessEvents`
observer per replica. Authority renewal, observer loss and monotonic expiry retain
the browser enforcement bounds. Each relay independently enforces Ready/Lease
controls. The public relay consumes those controls because the stock client treats
WebSocket text as gRPC data. CLI binary frames are bounded at 64 KiB, internal duplex
storage at 64 KiB, gRPC messages at 1 MiB and concurrent HTTP/2 streams at sixteen.

The facade supports Health, GetGatewayInfo, exact GetSandbox, exact CreateSshSession
and SSH-only ForwardTcp. All other methods return Unimplemented. Provider token
revocation cannot be exposed without proving the token belongs to this Computer;
the admitted stock sandbox-connect workflow does not require that RPC. Provider
administration, general TCP forwarding and cross-Computer targets remain denied.
GetSandbox accepts the public Computer UUID in the stock `default` workspace and
maps it to the admitted private provider binding. Its returned name stays the public
UUID. A copied command never needs the provider's hashed resource name.

Only incoming SSH data marks CLI activity. HTTP/2 and WebSocket keepalives, output
and lease controls cannot extend idle time. Encrypted SSH data can itself contain
SSH keepalives, so this profile measures channel activity rather than human
keystroke inactivity. The independent absolute and browser-family deadlines still
apply. Installed public CLI qualification remains delivery work.

POST `/admin/computers/{id}/cli-pairings` requires the admitted browser Origin and
current attach authority, then creates a two-minute one-use challenge. POST
`/admin/computers/{id}/cli-pairings/{pairing_id}/confirm` takes a closed empty body.
It repeats Origin and authority admission before consuming the retained challenge.
Both routes return HTTP 201 and `no-store`; only confirmation returns the opaque
credential and its persisted callback port. Replay cannot recover a lost credential.
The BFF supplies CSRF enforcement and the browser's explicit code-confirmation UI.
The native fixture exercises these HTTP routes across two replicas before running
the real CLI. It does not establish installed SSO or public ingress.

## Command Task Authority

`protocol/task_authority.rs` composes lifecycle and command authority for the standard
Tasks handlers. Command lookups use the domain's current named-grant or direct-owner
policy. The stored execution actor remains the Task owner even when the Computer owner
requests cancellation. Task reads and updates have an independent authority deadline
that also bounds blocked persistence work; a late response cannot extend access.
Command subscriptions revalidate the same authority every five seconds and send no
command bytes or output capability. Public grant and execution admission remain work.

## Resource And Task Subscriptions

Every listener has its own sink and authorization. The process admits at most sixty-four
listeners, each with at most sixty-four resource/Task targets. It validates the entire
accepted set before sending private updates. A new resource listener receives an
invalidation baseline and reads current state; transport replay IDs are absent.

The shared platform outbox is authoritative across replicas. LIVE notifications wake a
bounded persisted-page drain. LIVE loss closes the listener and requires a new baseline.
Task notifications use one shared durable Task update source for all independently
authorized targets. The listener establishes that source before projecting a current
Task baseline, then uses the shared Task projector for updates. A Computer owner may
observe Tasks owned by several execution actors without creating a watcher per actor.
All notifications remain filtered by the exact authorized Task owner and requested IDs. Provider events and terminal
contents never enter these streams.

Capacity health transitions also invalidate requested resources. Expiration of the
fifteen-second health observation invalidates availability even without another
provider event. Repeated health observations with unchanged availability produce no
extra notification. This signal prompts a canonical read and grants no authority.

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

The native command fixture now admits a real maintenance Task after adding a scoped
fixture grant to the running source. Two worker instances compete through the shared
Task lease. The production journal drives Stop, protected Capture, Retire, Transfer,
Create and Restore. The target loads the captured grant from the database checkpoint,
and retained command markers remain intact. This qualifies the worker's private
composition; public update requests and installed template transitions remain separate.
