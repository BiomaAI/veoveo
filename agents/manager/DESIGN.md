# Managed Agent Lifecycle Manager

Status: implementation in progress; installation qualification is pending.

## Standards And Protocols

The manager uses Kubernetes `apps/v1` Deployments and `core/v1` Secrets, PVCs,
ConfigMaps and Pods through typed HTTPS JSON requests. Native watches carry
resource versions and reconnect through a fresh inventory after watch loss.
Installation policy uses `admissionregistration.k8s.io/v1` ValidatingAdmissionPolicy
with CEL. These are selected Kubernetes APIs, not a general Kubernetes client.

SurrealDB transactions, lifecycle claims and managed generations are internal
Veoveo contracts. The exact published template and model connection use the shared
management contract's SHA-256 revision profile. Private-key credentials use RSA
2048-bit PKCS#8 DER, RS256 assertions and public JWK parameters. RSA 0.9.10 was
verified as the latest stable release on September 19, 2026. HTTPS uses workspace
reqwest 0.13.5, verified against its upstream release catalog on the same date.

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

Revision changes scale the prior Deployment down and wait for its Pods and runtime
lease to end before activating the new generation. Pause keeps an existing kernel
available for Task observation once its active episode is terminal. Archive revokes
dispatch, stops the owned workload and retains its PVC. Memory is never recreated
or force-deleted during recovery.

Readiness requires a Ready Pod and the same Pod UID and generation in the kernel's
lease-bound readiness record. Accepting an operation is distinct from completing it. Startup that fails to reach
readiness within ten minutes stops the workload and reports an actionable failure.
Bounded reconciliation workers consume outbox and Kubernetes watch hints. A bounded
inventory recovery pass covers lost hints and controller replacement. This service
does not inspect provider jobs or call a model.

## Installation Authority

The gateway has no Kubernetes write credential. The manager has namespace-scoped
resource permissions. Admission policy also constrains kernel images, service
accounts, security context, volume and Secret references, resource limits and
network placement. Kernel Pods do not receive a Kubernetes API token. Private
credentials never enter lifecycle responses, audit payloads or diagnostic logs.
