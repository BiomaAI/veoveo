# Managed Agent Authentication

Status: implementation in progress; this module is not yet installed.

## Standards And Protocols

| Boundary | Profile |
|---|---|
| OAuth 2.0 client credentials | Existing gateway access tokens, private-key JWT client assertions, automated invocation provenance and bounded scopes |
| RSA JWK | Public verification key in durable registration; signing keys remain Kubernetes Secrets |
| Veoveo templates | `VEOVEO_AGENT_TEMPLATES` contains closed installation-reviewed runtime packages, model/authority ceilings and parameter bindings |
| MCP `2026-07-28` | Full MCP client profile; managed agents retain native Tasks, resource notifications and subscriptions |
| HTTP JSON | Authenticated managed dispatch check validates current model/template permission before each kernel model dispatch |

## Effective Registration

One resolver reads the installation catalog and durable managed registration. Any
collision fails closed, including a disabled or archived managed registration.
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
