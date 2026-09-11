# Gateway Task Routes

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| MCP 2026-07-28 Tasks extension | Opaque canonical Task identifiers projected from upstream CreateTaskResult |
| Veoveo invocation authority | Tenant, actual actor, Work Context, profile and retained authorization fingerprint; every use also checks current policy |
| SurrealDB 3.2.4 | Durable mapping and unique source index from migration 0037; optional shared Task reference from migration 0041 |
| Veoveo `gtr_` route identifiers | 32 random bytes encoded as base64url without padding; identifiers convey no access authority |

`task_routes.rs` owns the mapping between an upstream Task and its public gateway
identifier. The source tuple is server, opaque upstream ID, owner and profile. Each
mapping also retains its tenant, Work Context, authorization digest and optional
shared-runtime Task link. Those fields must match before an existing route is reused.

An upstream tool retry can return its original Task. Gateway projection returns that
Task's existing public route, including across replicas and fresh authentication.
It preserves the original creation time and expiry. A concurrent CREATE or an
uncertain database response triggers one exact source lookup; this lookup never
redispatches the upstream tool. The unique source index arbitrates competing writers.
An expired mapping is not recreated or extended by a retry.

This does not loosen Tasks authorization. Reads, updates, cancellations and listeners
still compare the retained authority and current profile policy before contacting
the owning server. That server enforces current domain and named-grant authority.

The isolated store tests cover twelve competing projections, retained expiry,
changed Work Context and authority, source-link substitution, expired mappings,
independent servers and overflow-safe retention input. Public Computers lifecycle
acceptance found the original unconditional CREATE defect. Release 157 repeated the
original Stop and Start requests with fresh authentication and returned both original
canonical IDs. Neither effect repeated. Subsequent grant revocation denied the retry
at the owning Computers service.
