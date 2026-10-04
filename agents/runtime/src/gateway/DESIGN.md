# Managed Agent Authentication

## Standards And Protocols

| Boundary | Profile |
|---|---|
| OAuth 2.0 client credentials | Existing gateway access tokens, private-key JWT client assertions, automated invocation provenance and bounded scopes |
| RSA JWK | Public verification key in durable registration; signing keys remain Kubernetes Secrets |
| Veoveo templates | `VEOVEO_AGENT_TEMPLATES` contains closed installation-reviewed runtime packages, model/authority ceilings and parameter bindings |
| MCP `2026-07-28` | Full MCP client profile; managed agents retain native Tasks, resource notifications and subscriptions |
| HTTP JSON | Authenticated managed dispatch check validates current model/template permission before each kernel model dispatch |

## Gateway Adapter Ownership

The `gateway` feature exposes `ManagedOAuthClientResolver` and
`ManagedTemplateCatalog` without changing the runtime's default feature profile.
The adapter depends on the reusable gateway library. Composition supplies the Store
connection and an immutable template catalog, then binds the resolver once to
`GatewayState`. Authoring surfaces keep the same template catalog for their choices.
The generic port carries OAuth registrations and authority behavior; managed Store
records and template packages stay in this module. The owner-only
`AdmittedManagedOAuthClient` exposes a checked registration to managed dispatch.
That entrypoint shares the resolver's current-registration and template admission;
it does not reload the row after admission.

The Agents-owned `contract` feature declares the public `managed_agent` claim with
its existing JSON fields and schema identity. Composition reserves the claim name
and binds its typed codec in the same immutable registry used for verification and
issuance. Current registration and template admission precede execution attribution.
The signed generation and epoch become checked positive counters in the internal
`AuditManagedExecution`; task observation preserves the token's epoch, while tool
dispatch checks it against the current instance. Authoring HTTP vocabulary still
belongs to the shared MCP contract and requires a separate ownership transfer.

## Effective Registration

A client-credentials profile can be installed before its first managed client exists.
Control-plane validation does not require a placeholder static service client.
Interactive authorization modes still require their configured clients.

One resolver reads the installation catalog and durable managed registration. Any
collision fails closed, including a disabled or archived managed registration.
The token HTTP entrypoint resolves this effective registration before selecting a
protected resource. It passes that same registration to client-credentials
validation; a static-catalog lookup cannot reject a durable client first.
Managed clients obtain exactly their approved template's automated identity, scopes
and Work Context membership. A definition author cannot choose these fields.

Token issuance and later authenticated requests recheck durable registration and the
current installed template revision. MCP requests on existing sessions also pass
current admission. A removed template, archived instance, disabled definition or
revoked service principal closes access without waiting for token expiry.

Template parameters have closed shapes and installation-owned environment bindings.
Only the reviewed template loader expands deployment variables. Authored instructions
are assigned afterward as literal text; `${NAME}` in a prompt cannot read an environment
variable. Template images and storage/credential destinations are absent from browser
mutation input. The template revision binds both the kernel image digest and a
SHA256 digest of the immutable ConfigMap data map. The manager verifies canonical
JSON of that sorted string map before creating a workload. A same-name configuration
replacement cannot silently change an admitted runtime.

## Model Dispatch

The kernel checks current gateway model admission immediately before a model call.
The gateway also validates active generation and dispatch epoch in the store. This
small authenticated request shares the existing connection and avoids a second model
configuration authority in the kernel. It never invokes a model itself. If permission
cannot be confirmed, dispatch stops. Tool calls remain subject to the gateway's current
MCP policy and the revision's exact allowlist.

This check governs dispatch; stopping an agent does not cancel an already accepted
domain operation. Native Task observation and domain cancellation retain their own
contracts. The manager and kernel integrations must qualify these guarantees before
managed registrations are enabled in an installation.

## HTTP Routes

[`http/DESIGN.md`](http/DESIGN.md) owns Agents authoring, publication, conversation and input-request routes. The native capability reader composes shared MCP transport without a Workspace dependency. Workspace consumes typed publication facts and owns its chat-facing presentation.
