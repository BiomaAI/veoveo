# Computers Worker And MCP Projection

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Shared Tasks | Qualified `provider_wait` observation leases, domain-first settlement, retained Task projection |
| Veoveo Computers | Provider-independent operation, Computer, owner and Work Context records in `platform/computers` |
| Native OpenShell | Private mTLS/protobuf adapter in `platform/runtimes/computers`; its exact provider patch graph governs the selected Docker profile |
| MCP 2026-07-28, repository contract revision 3 | Intended public facade; protocol implementation and installed conformance remain in progress |
| JSON Schema 2020-12 | Shared public DTOs in `platform/computers/contract`; raw provider messages are never public request inputs |

The worker and future MCP/relay run in one Computers deployment. The gateway owns
ordinary catalog and action policy without importing the provider SDK. The worker
library does not expose an HTTP endpoint at this checkpoint.

## Public Application Projection

`Application` owns the command/read projection shared by the forthcoming MCP and
Console HTTP adapters. Each request obtains one current policy and directory snapshot
from the domain. It serves all action flags for that response and expires within
thirty seconds, capped by the request's admission lifetime. A new request does not
reuse that snapshot. The worker independently obtains its dispatch permit later.

The collection reports Setup Required without a fabricated template or quota. Quota
exhaustion reflects the owner's usage across Work Contexts plus tenant/provider
counts; reservation remains transactional. A provider availability observation expires
after fifteen seconds. Storage unavailability does not prohibit Stop. Connect and
Delete remain unavailable until their grant and purge implementations are connected.

The installation retains admitted templates by fingerprint and selects a default for
new Create requests. A retry resolves its first reservation before consulting that
default. Concurrent replicas that select different defaults converge on the same
first accepted Computer and Task. Caller input cannot choose arbitrary images.
Closed public lifecycle inputs require stable request UUIDs. Native transport,
provider work and terminal grants are separate from this projection's store tests.

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

The current library has no public protocol surface or runnable installation entrypoint.
Discovery, tools, resources, subscriptions, completion/prompts, well-known resources,
conformance, live grant authority and production packaging remain delivery work.
Its native fixture uses the actual current-policy reader, isolated installation
revision, production retained allocator and native OpenShell provider. Two worker
replicas compete for Create, then Stop succeeds with the allocator offline. Start
after allocator replacement preserves the file and changes the process identity.
Lost dispatch/settlement and current-policy cases retain their domain fence assertions.
The fixture establishes this composition; it does not establish public MCP or
installation acceptance.
