# Managed Agent Lifecycle Manager

Status: governed authoring, revision adoption and managed lifecycle are deployed.
Installed qualification covers model execution, retained identity and memory,
credential renewal, stop, archive and revocation. The
[qualification limits](#qualification-limits) identify additional work.

## Standards And Protocols

The manager uses Kubernetes `apps/v1` Deployments and `core/v1` Secrets, PVCs,
ConfigMaps and Pods through typed HTTPS JSON requests. Native watches carry
resource versions and reconnect through a fresh inventory after watch loss.
Installation policy uses `admissionregistration.k8s.io/v1` ValidatingAdmissionPolicy
with CEL. These are selected Kubernetes APIs, not a general Kubernetes client.

SurrealDB transactions, lifecycle claims and managed generations are internal
Veoveo contracts. The exact published template and model connection use the shared
management contract's SHA-256 revision profile. Private-key credentials use RSA
2048-bit PKCS#1 DER (RFC 8017, the kernel signing boundary), RS256 assertions and public JWK parameters. RSA 0.9.10 was
verified as the latest stable release on September 19, 2026. HTTPS uses workspace
reqwest 0.13.5, verified against its upstream release catalog on the same date.

## Database Credential Revision

Configuration carries a checked, nonsecret `database_credential_revision`. The
composer records it on managed pod templates. Reconciliation compares this owned
annotation without changing agent identity, instance generation or retained PVCs.
A stale Ready workload is retired through existing UID and ownership checks. The
manager waits for authoritative lease and pod drain observations, then uses the
existing Ready-to-Workload recovery transition. Unknown or disconnected observations
keep recovery pending on the existing retry schedule. Owner codecs and configuration
cannot replace that drain proof with annotation equality.

## Reconciliation

The API admits immutable resource names and reserves identity and capacity before
this service acts. A generation-scoped durable claim owns each reconciliation.
Each external write verifies that claim and the existing object's ownership.
Creation uses deterministic names; updates preserve Kubernetes resourceVersion
preconditions. Cleanup claims the current generation and rejects resources from a
newer generation. Credential cleanup also verifies the registered public key. Uncertain Secret creation is recovered by reading the same Secret
and correlating its public key. The manager never rotates an uncertain key.

A reviewed immutable ConfigMap contains the kernel manifest and memory migrations.
Its data digest must match the admitted template before a workload is created.
Closed template parameters become fixed environment bindings. Model credentials
remain installation Secret references, while authored instructions are loaded
from the registry by the kernel.

Revision changes delete the owned prior Deployment with foreground propagation and
UID/resource-version preconditions. The controller waits for the Deployment, Pods and
runtime lease to disappear before activating the new generation. This retirement
remains available when an obsolete image can no longer pass executable admission.
The installation policy admits finalizer updates on an already deleting Deployment
only while its specification and ownership metadata remain unchanged.
Signing Secrets and memory claims have independent ownership and survive the drain.
Pause keeps an existing kernel
available for Task observation once its active episode is terminal. Archive revokes
dispatch, stops the owned workload and retains its PVC. Memory is never recreated
or force-deleted during recovery.

Readiness requires a Ready Pod and the same Pod UID and generation in the kernel's
lease-bound readiness record. Accepting an operation is distinct from completing it. Startup that fails to reach
readiness within ten minutes stops the workload and reports an actionable failure.
Four reconciliation workers consume native database and Kubernetes watch changes.
Database reconnect starts a namespace inventory, paged by typed operation cursors.
The controller observes definition, instance, runtime, episode and authority tables;
its own operation-claim updates cannot trigger another inventory. It persists the
native feed cursor under its replica identity and reconciles current intent after
replacement.

Pods, Deployments, PVCs, signing Secrets and reviewed ConfigMaps supply metadata-only
watches. Inventory pages establish a resource version; ordinary watch expiration
resumes its latest event or bookmark version. Watch loss and HTTP 410 relist before
reconciliation, following the [Kubernetes watch contract](https://kubernetes.io/docs/reference/using-api/api-concepts/#efficient-detection-of-changes).
Each inventory has a 30-second deadline, pages contain at most 200 entries, and watch
frames are limited to 4 MiB. Namespace RBAC grants list/watch for these resources.

Timers target the next claim expiry, draining runtime lease or ten-minute startup
limit, using database time. A failed database or retryable Kubernetes request schedules
another attempt with 250 ms exponential backoff capped at 30 seconds. Successful idle
observation issues no periodic inventory query. Reconciliation reads deterministic
resource identities before retrying an uncertain mutation and preserves their fencing.
The manager does not inspect provider jobs or call a model.

## Installation Authority

The gateway has no Kubernetes write credential. The manager has namespace-scoped
resource permissions. Admission policy also constrains kernel images, service
accounts, security context, volume and Secret references, resource limits and
network placement. Kernel Pods do not receive a Kubernetes API token. Private
credentials never enter lifecycle responses, audit payloads or diagnostic logs.

## Packaging And Qualification

The `agent-manager` Bake target uses Dockerfile frontend 1.27.0 and the September 19,
2026 Debian Trixie slim digest, verified against their authoritative image registries.
The runtime contains the manager binary and CA certificates. It has no DuckDB or
browser dependency. `agent-runtime-support` owns both manager and kernel images.

The explicit ignored Rust test `installed_admission` creates a temporary namespace,
service accounts, RBAC and admission policies in the selected Kubernetes context.
It requires empty policy type-check warnings, admits resources from the actual
workload composer, rejects privilege and credential changes, and removes its fixture.
Executable workloads use server dry-run. One zero-replica Deployment qualifies deletion
after the fixture revokes executable admission. This is admission evidence, not installed reconciliation,
network enforcement or model execution evidence. YAML parsing uses serde_yaml_ng
0.10.0, verified as its latest stable release on September 19, 2026.

## Qualification Limits

| Work | Required qualification |
|---|---|
| Simulation and Map mission completion | Complete an installed flight with the selected mobility profile; investigate the Isaac/Cesium crash and `ClimbLimitExceeded` handoff failures without replaying interrupted work or weakening admission |
| Current recording delivery | Resolve forwarder backlog and ingest batch-quota failures, then qualify live catalog delivery; archived playback alone does not establish it |
| Kubernetes API routing | Find why the reference cluster controller reinstates stale API endpoint addresses; a corrected manager egress rule does not resolve intermittent API routing |
| Managed Computer templates | Extend the Computers authority reader and grant choices to resolve governed managed registrations before admitting Computer tools; chat agents currently use the human's explicit grant |
| Installed multi-round input | Exercise a production tool that emits an input request; existing ownership, forms and continuation checks are native fixtures |
| Load and human collaboration | Measure p95 under load and run a separately authenticated second-person usability journey |

A standalone draft-test endpoint is optional future product work. Published chat
agents and managed messages already provide explicit runs under their admitted
budgets and authority. Immutable development images do not establish full release
qualification. The [iteration record](../../docs/DEVELOPMENT_ITERATION.md#installed-pilot-and-rerun-recheck--september-25-2026)
contains the simulator and recording diagnostic measurements.

## Persistence Observation

The manager composes Agents' schema-only `AgentObservationTable` declarations with
kernel identity tables. Native changefeeds retain the existing reconciliation cursor
and acknowledged checkpoint behavior. Observing a change requests current-state
reconciliation; it does not establish controller authority or start an Agent episode.
