# Shared Policy Evaluation

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Veoveo gateway control plane | Existing typed profiles, policy sets, principal attributes, resource exposure and recording-ingest declarations |
| MCP 2026-07-28 | Canonical method/action names and domain resource ownership; evaluation itself performs no protocol I/O |
| JSON / JSON Schema 2020-12 | Models remain owned by `mcp/contract`; this extraction adds no wire fields or policy versions |
| SurrealDB 3.2.4 | Caller-owned current revision read in `platform/store`; the evaluator has no database dependency |

Gateway requests and Computers workers need the same policy decision. This library
owns the pure evaluator previously embedded in the gateway. It depends on canonical
contract types without the gateway's agent runtime, transport and analytics graph.
The gateway keeps its indexed catalog and delegates through `PolicyCatalogView`.
Other services can use `PolicyCatalog`, which validates and indexes one immutable
control-plane revision before it can be evaluated.

The policy algorithm retains deny precedence, profile exposure, domain ownership,
scopes, labels and principal requirements. Recording ingress uses its existing
resource and producer profile. Evaluation never loads credentials or makes a network
request. An Allow decision alone is not a current authority lease.

The owning service must authenticate the actor, load current grant/revocation state,
resolve current Work Context membership and select an authoritative control-plane
revision. It must account for that read's latency before issuing a bounded lease.
Caching immutable revisions is compatible with this boundary; renewing from a stale
head or unreachable authority store is not.

`PlatformStore::active_gateway_control_revision` reads the active pointer and that
exact retained revision in one round trip. It distinguishes an absent installation
pointer from a dangling or inconsistent revision. The gateway uses this same reader.
Readers capture monotonic time before the request and charge all read and validation
latency against any authority lease they issue afterwards.
