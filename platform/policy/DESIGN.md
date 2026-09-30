# Shared Policy Evaluation

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Veoveo gateway control plane | Existing typed profiles, policy sets, principal attributes, resource exposure and recording-ingest declarations |
| MCP 2026-07-28 | Canonical method/action names and domain resource ownership; evaluation itself performs no protocol I/O |
| JSON / JSON Schema 2020-12 | `mcp/contract` owns typed policy decisions and target kinds; selector configuration and policy versions keep their existing representation |
| SurrealDB 3.3.0 | Caller-owned current revision read in `platform/store`; the evaluator has no database dependency |
| Veoveo session-family authority | Internal read-only projection of the stored refresh family and verified request context; no new token or stored wire format |
| RFC 6570 declaration targets | Template discovery and completion carry the foundational `ResourceTemplateUri`. Policy selectors use their existing restricted lexical matcher, not RFC expansion or set containment. |

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

`resource_policy.rs` shares ownership and exposure checks between resource addresses
and template declarations without converting one type to the other. A template target
is admitted only for template discovery or completion; those actions reject concrete
resource targets. Scheme and prefix selectors compare the declaration's spelling.
The existing simple `{variable}` selector applies its lexical match to that spelling
through `matches_template`. This does not claim that every possible expansion is
allowed. Every later concrete read receives its own policy decision. Scope requirements,
server/scheme filters and deny precedence apply to both target forms.

`PlatformStore::active_gateway_control_revision` reads the active pointer and that
exact retained revision in one round trip. It distinguishes an absent installation
pointer from a dangling or inconsistent revision. The gateway uses this same reader.
Readers capture monotonic time before the request and charge all read and validation
latency against any authority lease they issue afterwards.

`session::SessionFamilyAuthority` owns the pure browser-family binding decision used
by gateway requests and Computer control. The caller reads the exact signed family ID
from its authority store and verifies the returned record ID. The projection validates
identity, authorization server, profile, client, Work Context, scopes and family state.
Refresh rotation preserves access within that family. Display metadata has no role in
this decision. Corrupt authority fields fail decoding without exposing their values.
Callers still authenticate the token and enforce its issuer, audience and time bounds.
This helper neither reads a database nor authorizes renewal from a cached family.
