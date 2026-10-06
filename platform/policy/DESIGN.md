# Shared Policy Evaluation

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Veoveo gateway control plane | Typed profiles, policy sets, principal attributes, resource exposure and registered owner sections |
| MCP 2026-07-28 | Canonical method/action names and domain resource ownership; evaluation itself performs no protocol I/O |
| JSON / JSON Schema 2020-12 | `mcp/contract` owns policy decisions and core targets; owner contracts supply registered target codecs. The protocol schema adapter composes their bound declarations |
| SurrealDB 3.3.0 | Caller-owned current revision read in `platform/store`; the evaluator has no database dependency |
| Veoveo session-family authority | Internal read-only projection of the stored refresh family and verified request context; no new token or stored wire format |
| RFC 6570 declaration targets | Template discovery and completion carry the foundational `ResourceTemplateUri`. Policy selectors use their existing restricted lexical matcher, not RFC expansion or set containment. |

Gateway requests and Computers workers need the same policy decision. This library
owns the pure evaluator previously embedded in the gateway. It depends on canonical
contract types without the gateway's agent runtime, transport and analytics graph.
The gateway keeps its indexed catalog and delegates through `PolicyCatalogView`.
Other services can use `PolicyCatalog`, which validates and indexes one immutable
control-plane revision before it can be evaluated.

PolicyCatalog requires the composition registry when it validates a revision. The
indexed gateway catalog carries the same binding through PolicyCatalogView. Action
handles and contributed targets must match that registry before authorization.

The shared evaluator owns deny precedence, principal conditions and missing-requirement
diagnostics. Owner adapters supply their target and label checks through these helpers;
Recording's policy feature owns producer and ingest-resource evaluation. Evaluation
never loads credentials or makes a network request. An Allow decision alone is not a
current authority lease.

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

## Resource Read Selection

`admit_resource_reads` evaluates a principal's read access to a registered server's
own URI scheme. It shares principal validation, rule matching and deny precedence
with `decide`. Resource-read rules select a server and scheme; profile exposure supplies
the remaining URI predicates. The result is a foundational `ResourceSelection` that
intersects ownership with those predicates. A database consumer must apply the entire
selection before ranking, pagination and decoding. The API excludes projected App
schemes, whose ownership requires the concrete gateway path.

The policy fixture compares selection with concrete `ResourcesRead` decisions across
scheme, prefix and template exposure, principal requirements and deny rules. Extending
resource rules with URI-specific predicates requires extending this API and its parity
cases in the same change. It cannot silently approximate a stronger rule.

## Internal Client Authority Port

`internal_clients` declares an object-safe current-authority resolver and typed
membership/tool result. The pure evaluator keeps no Store or Agent dependency.
The static implementation evaluates installed registration against the current
profile and rejects managed attribution. A composition selects it only when the
admitted plan excludes Agents. Agent adapters check live registration and collision
before selecting static or managed authority and declare the owner tables that wake
subscribers. Callers authenticate assertions, validate identity/control freshness,
and repeat resolution before delivery. Resolver results grant no cached lease.
