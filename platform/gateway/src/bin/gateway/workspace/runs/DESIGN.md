# Workspace Model Execution

## Standards And Protocols

This component exposes typed Veoveo HTTP JSON routes for agent admission, response
runs and cancellation. It uses the repository-qualified Rig revision
`eefffb118579cd4b1a48f8d7cd9bd04354b57634` and its OpenAI-compatible Chat Completions
stream adapter. This is an internal provider adapter, not an OpenAI Responses API
implementation. Existing gateway OAuth, session-family revocation and Work Context
contracts govern the initiating human. MCP Tasks remain separate durable operations;
this first response runner does not execute tools or claim Tasks support.

## Configuration And Disclosure

`VEOVEO_WORKSPACE_AGENTS` contains a bounded JSON array of typed definitions. Each
record names its installation-local identity, display name, description, provider,
tenant, admitted Work Contexts, instructions and model configuration. Model settings
contain an HTTP(S) base URL, model name, registered `provider_api_key` secret reference
and output-token budget. Embedded URL credentials, unknown contexts, unregistered
secrets, duplicate identities and unbounded budgets fail startup validation.

The model key is resolved through the existing gateway secret resolver immediately
before dispatch. No key, endpoint, system instructions or execution fence enters a
browser response. A human owner sees provider/model disclosure before admitting the
agent. Existing agent membership records bind the complete definition digest.
Configuration changes require new admission before another run uses that definition.
The default empty catalog permits human collaboration but provides no model fixture.
Installation configuration and deployed model qualification remain delivery work.

## Execution

The Rust store owns run identity, claim fencing, fixed context and cancellation as
specified in [`platform/store`](../../../../../../store/src/workspace/runs/DESIGN.md).
The gateway admits a direct human, resolves the configured definition and starts a
bounded background worker. Sixteen workers fit in one process; the store separately
enforces concurrent room capacity. Retrying admission preserves the same run ID.

A model call receives a bounded JSON history with stable author IDs and names. Its
triggering human request remains explicit. Older history can be dropped to fit the
64 KiB prompt bound, and the prompt marks that truncation. The trigger itself is never
silently truncated. Model output has a 32 KiB bound. Rig's total call budget is one,
which includes retries and continuations. Content telemetry is disabled explicitly.

The worker batches cumulative text into at most one store publication per second.
Each publication rechecks OAuth session and JWT revocation, current Work Context
membership and the store's run fence. Claim heartbeat, cancellation and output use
the same transaction boundary. The run ends within two minutes or token expiry.
Shutdown drops the provider stream; durable lease recovery later marks interruption.
A browser disconnect does not cancel work and cannot resubmit the model on reconnect.

## Browser Experience And Tasks Boundary

Owners admit agents through chat details. The composer explicitly selects recipients
and supports several agents. Each response has its own status and stop control while
humans retain the composer. The bounded activity response contains current agent
membership and the latest 64 runs; stable IDs replace updated output in place.
Human-history pagination remains independent from the active run window.

Stopping a response is distinct from requesting MCP Task cancellation. The accepted
first-class Task behavior is in
[`WORKSPACE_PLAN.md`](../../../../../../../docs/WORKSPACE_PLAN.md#first-class-mcp-tasks).
Task references, native Tasks input rounds, private results and cross-restart activity
are still implementation work and must pass before Workspace release acceptance.

## Qualification

A Rust HTTP fixture runs the actual Rig streaming client against an explicitly fake
provider and the real disposable database. It exercises two independent streams,
human writing during execution, one cancellation and retry without a second provider
call. Browser-edge tests reject caller-selected model settings and forged initiators.
A headed hardware browser fixture checks four authors, response-specific controls,
reconnect and mobile layout. None of these fixtures establishes deployed model or
MCP Task execution. Public acceptance remains mandatory.
